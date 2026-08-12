//! The jq-compatibility classifier behind `tq jq-check`.
//!
//! It answers one question: can this build execute a jq 1.7.1 invocation with
//! jq-compatible observable behavior? The answer is derived from the pieces
//! evaluation itself uses — the query parser and the builtin registry — so a
//! filter tq cannot dispatch, or a builtin the divergence ledger records, is
//! refused without a second allowlist to keep in step.
//!
//! A positive decision is the contract stated in `docs/tq-jq-parity.md`: for
//! every input on which jq 1.7.1 succeeds, tq produces jq's exact result.

use reddb_io_toon::Value;

use super::ast::{BinaryOp, Expr};
use super::builtins::{self, Support};
use super::parser::Parser;

/// The filter does not parse, so tq cannot run it at all.
pub(crate) const UNSUPPORTED_SYNTAX: &str = "unsupported-syntax";
/// The filter names something the builtin registry does not dispatch.
pub(crate) const UNSUPPORTED_BUILTIN: &str = "unsupported-builtin";
/// The filter names a builtin the divergence ledger records.
pub(crate) const DIVERGENT_BUILTIN: &str = "divergent-builtin";
/// The filter uses a construct jq 1.7.1 reads differently, or not at all.
pub(crate) const DIVERGENT_SYNTAX: &str = "divergent-syntax";
/// A command-line option tq does not honor with jq-compatible behavior.
pub(crate) const UNSUPPORTED_OPTION: &str = "unsupported-option";

/// Why an invocation cannot be executed jq-compatibly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reason {
    /// One of the stable `*-syntax`/`*-builtin`/`*-option` kinds above.
    pub(crate) kind: &'static str,
    pub(crate) detail: String,
}

impl Reason {
    pub(crate) fn new(kind: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

/// Every reason this filter cannot be executed jq-compatibly, in the order the
/// walk meets them. An empty result is a positive decision.
pub(crate) fn classify(filter: &str) -> Vec<Reason> {
    let expression = match Parser::new(filter).and_then(Parser::parse) {
        Ok(expression) => expression,
        Err(error) => return vec![Reason::new(UNSUPPORTED_SYNTAX, error)],
    };

    let mut walker = Walker::default();
    walker.visit(&expression);
    walker.reasons
}

#[derive(Default)]
struct Walker {
    /// The filters `def` brought into scope, as `(name, arity)`. A definition
    /// shadows a builtin of the same shape, exactly as it does in evaluation.
    scope: Vec<(String, usize)>,
    reasons: Vec<Reason>,
}

impl Walker {
    /// Records a reason once. A filter that calls `sin` twice has one problem,
    /// not two.
    fn report(&mut self, kind: &'static str, detail: impl Into<String>) {
        let reason = Reason::new(kind, detail);
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    fn visit_all(&mut self, expressions: &[Expr]) {
        for expression in expressions {
            self.visit(expression);
        }
    }

    fn visit_option(&mut self, expression: Option<&Expr>) {
        if let Some(expression) = expression {
            self.visit(expression);
        }
    }

    fn visit(&mut self, expression: &Expr) {
        match expression {
            Expr::Alternative(left, right) | Expr::Pipe(left, right) => {
                self.visit(left);
                self.visit(right);
            }
            Expr::Array(items) | Expr::Comma(items) => self.visit_all(items),
            Expr::Assign(_, target, value) => {
                self.visit(target);
                self.visit(value);
            }
            Expr::Bind(source, _, body) => {
                self.visit(source);
                self.visit(body);
            }
            Expr::Binary(operator, left, right) => self.binary(*operator, left, right),
            Expr::Call(name, arguments) => {
                self.call(name, arguments);
                self.visit_all(arguments);
            }
            Expr::Conditional(branches, fallback) => {
                for (condition, body) in branches {
                    self.visit(condition);
                    self.visit(body);
                }
                self.visit(fallback);
            }
            Expr::Def {
                name,
                parameters,
                body,
                rest,
            } => self.definition(name, parameters, body, rest),
            Expr::Empty
            | Expr::Environment
            | Expr::Identity
            | Expr::Literal(_)
            | Expr::Variable(_) => {}
            Expr::Field(base, _) | Expr::Iter(base) | Expr::Optional(base) => self.visit(base),
            Expr::Foreach {
                generator,
                initial,
                update,
                extract,
                ..
            } => {
                self.visit(generator);
                self.visit(initial);
                self.visit(update);
                self.visit(extract);
            }
            Expr::Index(base, index) => {
                self.visit(base);
                self.visit(index);
            }
            Expr::Object(entries) => {
                for (_, value) in entries {
                    self.visit(value);
                }
            }
            Expr::Reduce {
                generator,
                initial,
                update,
                ..
            } => {
                self.visit(generator);
                self.visit(initial);
                self.visit(update);
            }
            Expr::Slice(base, start, end) => {
                self.visit(base);
                self.visit_option(start.as_deref());
                self.visit_option(end.as_deref());
            }
            Expr::Try(body, handler) => {
                self.visit(body);
                self.visit_option(handler.as_deref());
            }
        }
    }

    /// jq's comparison operators are non-associative, so it rejects `a < b < c`
    /// outright while tq's grammar folds the chain to the left.
    fn binary(&mut self, operator: BinaryOp, left: &Expr, right: &Expr) {
        if is_comparison(operator) && (is_compared(left) || is_compared(right)) {
            self.report(
                DIVERGENT_SYNTAX,
                "chained comparison; jq 1.7.1 rejects `a < b < c` as a syntax error",
            );
        }
        self.visit(left);
        self.visit(right);
    }

    fn call(&mut self, name: &str, arguments: &[Expr]) {
        let arity = arguments.len();
        if self
            .scope
            .iter()
            .any(|(defined, defined_arity)| defined == name && *defined_arity == arity)
        {
            return;
        }

        match builtins::classify(name, arity) {
            Support::Compatible => self.numeric_key(name, arguments),
            Support::Divergent(reason) => self.report(DIVERGENT_BUILTIN, reason),
            Support::Unknown => self.report(
                UNSUPPORTED_BUILTIN,
                format!("`{name}/{arity}` is not implemented"),
            ),
        }
    }

    /// `has` matches jq for the string keys jq accepts. A numeric key is the
    /// ledgered divergence, and a literal argument is enough to see it coming.
    fn numeric_key(&mut self, name: &str, arguments: &[Expr]) {
        if name == "has" && matches!(arguments.first(), Some(Expr::Literal(Value::Number(_)))) {
            self.report(
                DIVERGENT_BUILTIN,
                "`has/1` stringifies a numeric key; jq 1.7.1 raises on one",
            );
        }
    }

    /// `def name(parameters): body; rest`. The body sees the definition itself
    /// and its parameters; everything after the semicolon sees only the
    /// definition.
    fn definition(&mut self, name: &str, parameters: &[String], body: &Expr, rest: &Expr) {
        let outer = self.scope.len();
        self.scope.push((name.to_owned(), parameters.len()));
        // Both parameter spellings bind a filter under the bare name.
        self.scope
            .extend(parameters.iter().map(|parameter| (parameter.clone(), 0)));
        self.visit(body);
        self.scope.truncate(outer + 1);
        self.visit(rest);
        self.scope.truncate(outer);
    }
}

const fn is_comparison(operator: BinaryOp) -> bool {
    matches!(
        operator,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}

fn is_compared(expression: &Expr) -> bool {
    matches!(expression, Expr::Binary(operator, ..) if is_comparison(*operator))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(filter: &str) -> Vec<&'static str> {
        classify(filter)
            .into_iter()
            .map(|reason| reason.kind)
            .collect()
    }

    #[test]
    fn an_ordinary_filter_is_compatible() {
        for filter in [
            ".users[0].name",
            "[.[]|select(.age>30)]|length",
            "reduce .[] as $item (0; .+$item)",
            "foreach .[] as [$a,$b] (0; .+$a; .+$b)",
            "to_entries|map({key:.key,value:(.value*2)})|from_entries",
            ".a // \"fallback\"",
            "try error(\"x\") catch .",
            "if .a then .b else .c end",
            ".[1:3], .[]?, ..",
            "\"n=\\(.a)\"",
            ".a as $x | $x + 1",
            ".a |= . + 1 | .b += 2",
            "env|has(\"PATH\")",
            "empty",
            "$__loc__",
        ] {
            assert_eq!(classify(filter), Vec::new(), "{filter}");
        }
    }

    #[test]
    fn a_filter_that_does_not_parse_is_unsupported_syntax() {
        assert_eq!(kinds("{\"\\(.a)\": 1}"), vec![UNSUPPORTED_SYNTAX]);
        assert_eq!(kinds(".["), vec![UNSUPPORTED_SYNTAX]);
    }

    #[test]
    fn a_builtin_the_registry_lacks_is_unsupported() {
        assert_eq!(kinds("sin"), vec![UNSUPPORTED_BUILTIN]);
        assert_eq!(kinds("utf8bytelength"), vec![UNSUPPORTED_BUILTIN]);
        assert_eq!(kinds("input_line_number"), vec![UNSUPPORTED_BUILTIN]);
        assert_eq!(kinds("repeat(.;2)"), vec![UNSUPPORTED_BUILTIN]);
        assert_eq!(kinds("range"), vec![UNSUPPORTED_BUILTIN]);
        assert_eq!(
            classify("sin")[0].detail,
            "`sin/0` is not implemented".to_owned()
        );
    }

    #[test]
    fn a_ledgered_builtin_is_divergent() {
        for filter in [
            "toarray",
            "trim",
            "trimstr(\"x\")",
            "[leaf_paths]",
            "1,halt",
        ] {
            assert_eq!(kinds(filter), vec![DIVERGENT_BUILTIN], "{filter}");
        }
    }

    #[test]
    fn a_numeric_has_key_is_divergent_but_a_string_one_is_not() {
        assert_eq!(kinds("has(1)"), vec![DIVERGENT_BUILTIN]);
        assert_eq!(kinds("has(\"a\")"), Vec::<&str>::new());
        assert_eq!(kinds("has(.k)"), Vec::<&str>::new());
    }

    #[test]
    fn a_chained_comparison_is_divergent_syntax() {
        assert_eq!(kinds("1 < 2 < 3"), vec![DIVERGENT_SYNTAX]);
        assert_eq!(kinds(".a == .b == .c"), vec![DIVERGENT_SYNTAX]);
        assert_eq!(kinds("(1 < 2) and (2 < 3)"), Vec::<&str>::new());
        assert_eq!(kinds("1 + 2 < 3"), Vec::<&str>::new());
    }

    #[test]
    fn a_definition_shadows_the_registry_and_leaves_its_scope() {
        assert_eq!(classify("def twice(f): f|f; twice(.+1)"), Vec::new());
        assert_eq!(classify("def sin: 1; sin"), Vec::new());
        assert_eq!(classify("def f($a): $a+1; f(2)"), Vec::new());
        // `f` is gone once its own body ends, so the inner call is unknown.
        assert_eq!(kinds("def f: g; f"), vec![UNSUPPORTED_BUILTIN]);
        // A parameter is only callable inside the body it belongs to.
        assert_eq!(kinds("def f(g): g; f(.)|g"), vec![UNSUPPORTED_BUILTIN]);
    }

    #[test]
    fn one_problem_is_reported_once_and_several_are_all_reported() {
        assert_eq!(kinds("sin, sin"), vec![UNSUPPORTED_BUILTIN]);
        assert_eq!(
            kinds("[sin, trim]"),
            vec![UNSUPPORTED_BUILTIN, DIVERGENT_BUILTIN]
        );
    }

    #[test]
    fn the_walk_reaches_every_nested_position() {
        for filter in [
            "sin|.",
            ".|sin",
            "[sin]",
            "sin,.",
            ".a = sin",
            "sin as $x | $x",
            "sin + 1",
            "length(sin)",
            "if sin then . else . end",
            "if . then sin else . end",
            "if . then . else sin end",
            "def f: .; sin",
            ".[sin]",
            ".[sin:]",
            ".[:sin]",
            "[.[]|sin]",
            "(sin)?",
            "{a:sin}",
            ".a|sin",
            "try sin",
            "try . catch sin",
            "reduce sin as $x (0; .)",
            "reduce .[] as $x (sin; .)",
            "reduce .[] as $x (0; sin)",
            "foreach sin as $x (0; .; .)",
            "foreach .[] as $x (sin; .; .)",
            "foreach .[] as $x (0; sin; .)",
            "foreach .[] as $x (0; .; sin)",
            "sin // 1",
            "1 // sin",
            "sin.a",
        ] {
            assert!(!classify(filter).is_empty(), "{filter}");
        }
    }

    #[test]
    fn a_reason_carries_its_kind_and_detail() {
        let reason = Reason::new(UNSUPPORTED_OPTION, "`--stream` is not supported");
        assert_eq!(reason.kind, UNSUPPORTED_OPTION);
        assert_eq!(reason.detail, "`--stream` is not supported");
        assert!(format!("{reason:?}").contains("unsupported-option"));
    }
}
