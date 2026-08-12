use std::rc::Rc;

use reddb_io_toon::Value;

use super::ast::{BinaryOp, Expr, Pattern};
use super::builtins;
use super::lexer::{lex, Token};

pub(super) struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

/// A declared parameter. Both spellings bind a filter under the bare name; the
/// `$name` spelling additionally binds one value per generated output.
enum Parameter {
    Filter(String),
    Value(String),
}

impl Parameter {
    fn name(&self) -> &String {
        match self {
            Self::Filter(name) | Self::Value(name) => name,
        }
    }
}

impl Parser {
    pub(super) fn new(query: &str) -> Result<Self, String> {
        Ok(Self {
            tokens: lex(query)?,
            index: 0,
        })
    }

    pub(super) fn parse(mut self) -> Result<Expr, String> {
        let expression = self.parse_pipe()?;
        if self.peek().is_some() {
            return Err("unexpected trailing filter input".to_owned());
        }
        Ok(expression)
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        if self.consume_keyword("def") {
            return self.parse_definition(false);
        }
        let expression = self.parse_comma()?;
        if self.consume_keyword("as") {
            let pattern = self.parse_pattern()?;
            self.expect(Token::Pipe)?;
            let body = self.parse_pipe()?;
            return Ok(Expr::Bind(Box::new(expression), pattern, Box::new(body)));
        }
        if self.consume(&Token::Pipe) {
            let right = self.parse_pipe()?;
            return Ok(Expr::Pipe(Box::new(expression), Box::new(right)));
        }
        Ok(expression)
    }

    fn parse_comma(&mut self) -> Result<Expr, String> {
        let mut expressions = vec![self.parse_alternative()?];
        while self.consume(&Token::Comma) {
            expressions.push(self.parse_alternative()?);
        }
        if expressions.len() == 1 {
            Ok(expressions.pop().expect("one expression exists"))
        } else {
            Ok(Expr::Comma(expressions))
        }
    }

    fn parse_alternative(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_assignment()?;
        while self.consume(&Token::Alternative) {
            let right = self.parse_assignment()?;
            expression = Expr::Alternative(Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let expression = self.parse_or()?;
        if self.consume(&Token::Assign) {
            return Err("assignment operators are not supported yet".to_owned());
        }
        Ok(expression)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_and()?;
        while self.consume_keyword("or") {
            let right = self.parse_and()?;
            expression = Expr::Binary(BinaryOp::Or, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_comparison()?;
        while self.consume_keyword("and") {
            let right = self.parse_comparison()?;
            expression = Expr::Binary(BinaryOp::And, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_additive()?;
        while let Some(operator) = self.match_comparison_operator() {
            let right = self.parse_additive()?;
            expression = Expr::Binary(operator, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_multiplicative()?;
        loop {
            let operator = if self.consume(&Token::Plus) {
                BinaryOp::Add
            } else if self.consume(&Token::Minus) {
                BinaryOp::Subtract
            } else {
                break;
            };
            let right = self.parse_multiplicative()?;
            expression = Expr::Binary(operator, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_unary()?;
        loop {
            let operator = if self.consume(&Token::Star) {
                BinaryOp::Multiply
            } else if self.consume(&Token::Slash) {
                BinaryOp::Divide
            } else if self.consume(&Token::Percent) {
                BinaryOp::Modulo
            } else {
                break;
            };
            let right = self.parse_unary()?;
            expression = Expr::Binary(operator, Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.consume(&Token::Minus) {
            let expression = self.parse_unary()?;
            return Ok(Expr::Binary(
                BinaryOp::Subtract,
                Box::new(Expr::Literal(Value::Number("0".to_owned()))),
                Box::new(expression),
            ));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.consume(&Token::FieldDot) {
                let key = self.expect_ident()?;
                expression = Expr::Field(Box::new(expression), key);
                continue;
            }

            if self.consume(&Token::LBracket) {
                if self.consume(&Token::RBracket) {
                    expression = Expr::Iter(Box::new(expression));
                    continue;
                }

                let start = if self.peek() == Some(&Token::Colon) {
                    None
                } else {
                    Some(Box::new(self.parse_pipe()?))
                };
                if self.consume(&Token::Colon) {
                    let end = if self.peek() == Some(&Token::RBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_pipe()?))
                    };
                    self.expect(Token::RBracket)?;
                    expression = Expr::Slice(Box::new(expression), start, end);
                } else {
                    let index = start.ok_or_else(|| "expected array index".to_owned())?;
                    self.expect(Token::RBracket)?;
                    expression = Expr::Index(Box::new(expression), index);
                }
                continue;
            }

            if self.consume(&Token::Question) {
                expression = Expr::Optional(Box::new(expression));
                continue;
            }

            break;
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Dot) => Ok(Expr::Identity),
            Some(Token::FieldDot) => {
                Ok(Expr::Field(Box::new(Expr::Identity), self.expect_ident()?))
            }
            Some(Token::Ident(value)) => self.parse_identifier(value),
            Some(Token::LBracket) => self.parse_array_constructor(),
            Some(Token::LBrace) => self.parse_object_constructor(),
            Some(Token::LParen) => {
                let expression = self.parse_pipe()?;
                self.expect(Token::RParen)?;
                Ok(expression)
            }
            Some(Token::Number(value)) => Ok(Expr::Literal(Value::Number(value))),
            Some(Token::String(value)) => Ok(Expr::Literal(Value::String(value))),
            Some(Token::Variable(name)) => Ok(Expr::Variable(name)),
            token => Err(format!("unexpected token `{token:?}`")),
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern, String> {
        match self.next() {
            Some(Token::LBracket) => {
                let mut items = Vec::new();
                if !self.consume(&Token::RBracket) {
                    loop {
                        items.push(self.parse_pattern()?);
                        if !self.consume(&Token::Comma) {
                            self.expect(Token::RBracket)?;
                            break;
                        }
                    }
                }
                Ok(Pattern::Array(items))
            }
            Some(Token::LBrace) => {
                let mut fields = Vec::new();
                if !self.consume(&Token::RBrace) {
                    loop {
                        let (key, pattern) = match self.next() {
                            Some(Token::Variable(name)) => (name.clone(), Pattern::Variable(name)),
                            Some(Token::Ident(key)) | Some(Token::String(key)) => {
                                self.expect(Token::Colon)?;
                                (key, self.parse_pattern()?)
                            }
                            token => {
                                return Err(format!("expected pattern key, got `{token:?}`"));
                            }
                        };
                        fields.push((key, pattern));
                        if !self.consume(&Token::Comma) {
                            self.expect(Token::RBrace)?;
                            break;
                        }
                    }
                }
                Ok(Pattern::Object(fields))
            }
            Some(Token::Variable(name)) => Ok(Pattern::Variable(name)),
            token => Err(format!("expected binding pattern, got `{token:?}`")),
        }
    }

    fn parse_identifier(&mut self, name: String) -> Result<Expr, String> {
        match name.as_str() {
            "empty" => Ok(Expr::Empty),
            "env" => Ok(Expr::Environment),
            "false" => Ok(Expr::Literal(Value::Bool(false))),
            "foreach" => self.parse_foreach(),
            "if" => self.parse_conditional(),
            "null" => Ok(Expr::Literal(Value::Null)),
            "reduce" => self.parse_reduce(),
            "true" => Ok(Expr::Literal(Value::Bool(true))),
            "try" => self.parse_try(),
            _ => self.parse_call(name),
        }
    }

    fn parse_conditional(&mut self) -> Result<Expr, String> {
        let mut branches = Vec::new();
        let condition = self.parse_pipe()?;
        self.expect_keyword("then")?;
        branches.push((condition, self.parse_pipe()?));

        while self.consume_keyword("elif") {
            let condition = self.parse_pipe()?;
            self.expect_keyword("then")?;
            branches.push((condition, self.parse_pipe()?));
        }

        let fallback = if self.consume_keyword("else") {
            self.parse_pipe()?
        } else {
            Expr::Identity
        };
        self.expect_keyword("end")?;
        Ok(Expr::Conditional(branches, Box::new(fallback)))
    }

    fn parse_try(&mut self) -> Result<Expr, String> {
        let expression = self.parse_assignment()?;
        let handler = self
            .consume_keyword("catch")
            .then(|| self.parse_assignment())
            .transpose()?
            .map(Box::new);
        Ok(Expr::Try(Box::new(expression), handler))
    }

    fn parse_reduce(&mut self) -> Result<Expr, String> {
        let generator = self.parse_comma()?;
        self.expect_keyword("as")?;
        let pattern = self.parse_pattern()?;
        self.expect(Token::LParen)?;
        let initial = self.parse_pipe()?;
        self.expect(Token::Semicolon)?;
        let update = self.parse_pipe()?;
        self.expect(Token::RParen)?;
        Ok(Expr::Reduce {
            generator: Box::new(generator),
            pattern,
            initial: Box::new(initial),
            update: Box::new(update),
        })
    }

    fn parse_foreach(&mut self) -> Result<Expr, String> {
        let generator = self.parse_comma()?;
        self.expect_keyword("as")?;
        let pattern = self.parse_pattern()?;
        self.expect(Token::LParen)?;
        let initial = self.parse_pipe()?;
        self.expect(Token::Semicolon)?;
        let update = self.parse_pipe()?;
        let extract = if self.consume(&Token::Semicolon) {
            self.parse_pipe()?
        } else {
            Expr::Identity
        };
        self.expect(Token::RParen)?;
        Ok(Expr::Foreach {
            generator: Box::new(generator),
            pattern,
            initial: Box::new(initial),
            update: Box::new(update),
            extract: Box::new(extract),
        })
    }

    /// `def name(parameters): body; rest`. The body always ends at the
    /// semicolon, and jq scopes the definition over everything after it, so the
    /// tail parses in the caller's context: a full pipeline at the top level,
    /// one comma-delimited item inside an array or object constructor.
    fn parse_definition(&mut self, item: bool) -> Result<Expr, String> {
        let name = self.expect_ident()?;
        let parameters = self.parse_definition_parameters()?;
        self.expect(Token::Colon)?;
        let body = self.parse_pipe()?;
        self.expect(Token::Semicolon)?;
        let rest = if item {
            self.parse_pipe_item()?
        } else {
            self.parse_pipe()?
        };
        Ok(Expr::Def {
            name,
            parameters: parameters
                .iter()
                .map(|parameter| parameter.name().clone())
                .collect(),
            body: Rc::new(bind_value_parameters(&parameters, body)),
            rest: Box::new(rest),
        })
    }

    fn parse_definition_parameters(&mut self) -> Result<Vec<Parameter>, String> {
        if !self.consume(&Token::LParen) {
            return Ok(Vec::new());
        }

        let mut parameters = Vec::new();
        loop {
            parameters.push(match self.next() {
                Some(Token::Ident(name)) => Parameter::Filter(name),
                Some(Token::Variable(name)) => Parameter::Value(name),
                token => return Err(format!("expected function parameter, got `{token:?}`")),
            });
            if self.consume(&Token::Semicolon) {
                continue;
            }
            self.expect(Token::RParen)?;
            break;
        }
        Ok(parameters)
    }

    fn parse_call(&mut self, name: String) -> Result<Expr, String> {
        // Keep the historic bare spelling for zero-arity builtins. Leaving a
        // following parenthesis untouched preserves its existing diagnostic.
        if builtins::supports(&name, 0) && self.peek() != Some(&Token::LParen) {
            return Ok(Expr::Call(name, Vec::new()));
        }
        if !self.consume(&Token::LParen) {
            return Ok(Expr::Call(name, Vec::new()));
        }
        if self.peek() == Some(&Token::RParen) {
            return Err(format!("unexpected token `{:?}`", self.peek().cloned()));
        }

        let mut arguments = Vec::new();
        loop {
            arguments.push(self.parse_pipe()?);
            if self.consume(&Token::Semicolon) {
                continue;
            }
            self.expect(Token::RParen)?;
            break;
        }
        Ok(Expr::Call(name, arguments))
    }

    fn parse_array_constructor(&mut self) -> Result<Expr, String> {
        if self.consume(&Token::RBracket) {
            return Ok(Expr::Array(Vec::new()));
        }

        let mut items = Vec::new();
        loop {
            items.push(self.parse_pipe_item()?);
            if self.consume(&Token::Comma) {
                continue;
            }
            self.expect(Token::RBracket)?;
            break;
        }
        Ok(Expr::Array(items))
    }

    fn parse_object_constructor(&mut self) -> Result<Expr, String> {
        if self.consume(&Token::RBrace) {
            return Ok(Expr::Object(Vec::new()));
        }

        let mut fields = Vec::new();
        loop {
            let key = match self.next() {
                Some(Token::Ident(value)) | Some(Token::String(value)) => value,
                token => return Err(format!("expected object key, got `{token:?}`")),
            };
            self.expect(Token::Colon)?;
            fields.push((key, self.parse_pipe_item()?));
            if self.consume(&Token::Comma) {
                continue;
            }
            self.expect(Token::RBrace)?;
            break;
        }
        Ok(Expr::Object(fields))
    }

    fn parse_pipe_item(&mut self) -> Result<Expr, String> {
        if self.consume_keyword("def") {
            return self.parse_definition(true);
        }
        let expression = self.parse_alternative()?;
        if self.consume_keyword("as") {
            let pattern = self.parse_pattern()?;
            self.expect(Token::Pipe)?;
            let body = self.parse_pipe_item()?;
            return Ok(Expr::Bind(Box::new(expression), pattern, Box::new(body)));
        }
        if self.consume(&Token::Pipe) {
            let right = self.parse_pipe_item()?;
            return Ok(Expr::Pipe(Box::new(expression), Box::new(right)));
        }
        Ok(expression)
    }

    fn match_comparison_operator(&mut self) -> Option<BinaryOp> {
        let operator = match self.peek()? {
            Token::EqualEqual => BinaryOp::Equal,
            Token::Greater => BinaryOp::Greater,
            Token::GreaterEqual => BinaryOp::GreaterEqual,
            Token::Less => BinaryOp::Less,
            Token::LessEqual => BinaryOp::LessEqual,
            Token::NotEqual => BinaryOp::NotEqual,
            _ => return None,
        };
        self.index += 1;
        Some(operator)
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Token::Ident(value)) => Ok(value),
            token => Err(format!("expected identifier, got `{token:?}`")),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        let actual = self.next();
        if actual == Some(expected.clone()) {
            Ok(())
        } else {
            Err(format!("expected `{expected:?}`, got `{actual:?}`"))
        }
    }

    fn consume(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn consume_keyword(&mut self, expected: &str) -> bool {
        if matches!(self.peek(), Some(Token::Ident(value)) if value == expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, expected: &str) -> Result<(), String> {
        if self.consume_keyword(expected) {
            Ok(())
        } else {
            Err(format!("expected keyword `{expected}`"))
        }
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }
}

/// jq reads `def f($a): body` as `def f(a): a as $a | body`, so a value
/// parameter is a filter parameter whose output is bound one value at a time.
fn bind_value_parameters(parameters: &[Parameter], body: Expr) -> Expr {
    parameters
        .iter()
        .rev()
        .fold(body, |body, parameter| match parameter {
            Parameter::Filter(_) => body,
            Parameter::Value(name) => Expr::Bind(
                Box::new(Expr::Call(name.clone(), Vec::new())),
                Pattern::Variable(name.clone()),
                Box::new(body),
            ),
        })
}
