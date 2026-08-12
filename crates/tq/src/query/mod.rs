use reddb_io_toon::Value;

mod ast;
mod builtins;
mod eval;
mod indexing;
mod lexer;
mod parser;
mod paths;

/// The named `$variables` a query starts with: the ones `--arg`/`--argjson`
/// supplied, plus the `$ARGS` object jq also exposes them through.
#[derive(Debug)]
pub(crate) struct Variables {
    bindings: Vec<(String, Value)>,
}

impl Variables {
    pub(crate) fn new(named: &[(String, serde_json::Value)]) -> Self {
        // A repeated name keeps its first binding, as jq does.
        let mut object = serde_json::Map::new();
        for (name, value) in named {
            object.entry(name.clone()).or_insert_with(|| value.clone());
        }
        let mut bindings = object
            .iter()
            .map(|(name, value)| (name.clone(), Value::from_json_value(value.clone())))
            .collect::<Vec<_>>();
        bindings.push((
            "ARGS".to_owned(),
            Value::from_json_value(serde_json::json!({"positional": [], "named": object})),
        ));
        Self { bindings }
    }
}

pub(crate) fn evaluate(
    document: &Value,
    query: &str,
    variables: &Variables,
) -> Result<Vec<Value>, String> {
    let expression = parser::Parser::new(query)?.parse()?;
    expression.eval(document, &eval::Env::with_variables(&variables.bindings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: usize = 200;

    /// The codec's row-decode counter is process-global, so the tests that
    /// read it — and the ones that would perturb it — take turns.
    static ROW_DECODES: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn counting() -> std::sync::MutexGuard<'static, ()> {
        let guard = ROW_DECODES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reddb_io_toon::reset_tabular_row_decode_count_for_tests();
        guard
    }

    /// Only the compatibility parser builds a row-backed array today; the v4.1
    /// event decoder materialises every array as a list. The laziness under
    /// test here is the query engine's, so the test takes the one input shape
    /// that can still expose it.
    fn tabular_document() -> Value {
        let mut input = format!("users[{ROWS}]{{id,name}}:\n");
        for index in 0..ROWS {
            input.push_str(&format!("  {index},name-{index}\n"));
        }
        Value::parse_legacy(&input).expect("tabular document parses")
    }

    /// ADR 0002's laziness contract, held to across the path layer: a field or
    /// index read walks the codec's lazy accessors, so naming one row of a
    /// 200-row table decodes exactly that row — in the value evaluator and in
    /// the path evaluator alike.
    #[test]
    fn a_field_or_index_read_decodes_only_the_touched_row() {
        let _counting = counting();
        let document = tabular_document();
        let variables = Variables::new(&[]);
        let filters = [
            ".users[7].name",
            "path(.users[7].name)",
            "[path(.users[7].name)]|length",
            "getpath([\"users\",7,\"name\"])",
        ];

        for filter in filters {
            reddb_io_toon::reset_tabular_row_decode_count_for_tests();
            let values = evaluate(&document, filter, &variables).expect("query succeeds");
            assert!(!values.is_empty(), "{filter}");
            assert_eq!(
                reddb_io_toon::tabular_row_decode_count_for_tests(),
                1,
                "{filter}"
            );
        }
    }

    /// The counterexample that keeps the assertion above honest: a query that
    /// has to look everywhere really does decode every row.
    #[test]
    fn enumerating_every_path_decodes_every_row() {
        let _counting = counting();
        let document = tabular_document();
        let variables = Variables::new(&[]);

        evaluate(&document, "[paths]|length", &variables).expect("query succeeds");

        assert_eq!(reddb_io_toon::tabular_row_decode_count_for_tests(), ROWS);
    }

    /// The write side of the same contract: editing one row materialises the
    /// table it touches, and the edit lands where it was asked for.
    #[test]
    fn a_write_materializes_the_table_it_touches() {
        let _counting = counting();
        let document = tabular_document();
        let variables = Variables::new(&[]);
        let filter = "setpath([\"users\",7,\"name\"];\"Ada\")|[.users[7].name,(.users|length)]";
        let values = evaluate(&document, filter, &variables).expect("query succeeds");

        assert_eq!(
            values[0].to_json_value(),
            serde_json::json!(["Ada", ROWS as i64])
        );
    }
}
