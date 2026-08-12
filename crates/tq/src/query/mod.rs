use reddb_io_toon::Value;

mod assign;
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

    fn tabular_document() -> Value {
        table_document(&["users"])
    }

    fn table_document(names: &[&str]) -> Value {
        let mut input = String::new();
        for name in names {
            input.push_str(&format!("{name}[{ROWS}]{{id,name}}:\n"));
            for index in 0..ROWS {
                input.push_str(&format!("  {index},name-{index}\n"));
            }
        }
        Value::parse_toon(&input).expect("tabular document parses")
    }

    /// A field or index read reaches the row it names, through the value
    /// evaluator and the path evaluator alike.
    #[test]
    fn a_field_or_index_read_reaches_the_row_it_names() {
        let document = tabular_document();
        let variables = Variables::new(&[]);
        let filters = [
            ".users[7].name",
            "path(.users[7].name)",
            "[path(.users[7].name)]|length",
            "getpath([\"users\",7,\"name\"])",
        ];

        for filter in filters {
            let values = evaluate(&document, filter, &variables).expect("query succeeds");
            assert!(!values.is_empty(), "{filter}");
        }
        assert_eq!(
            evaluate(&document, ".users[7].name", &variables).expect("query succeeds")[0]
                .to_json_value(),
            serde_json::json!("name-7")
        );
    }

    /// A write lands where it was asked for and leaves the table's length alone.
    #[test]
    fn a_write_lands_on_the_row_it_names() {
        let document = tabular_document();
        let variables = Variables::new(&[]);
        let filter = "setpath([\"users\",7,\"name\"];\"Ada\")|[.users[7].name,(.users|length)]";
        let values = evaluate(&document, filter, &variables).expect("query succeeds");

        assert_eq!(
            values[0].to_json_value(),
            serde_json::json!(["Ada", ROWS as i64])
        );
    }

    /// The assignment family writes through the path layer: each operator
    /// combines with the current value the way jq does.
    #[test]
    fn the_assignment_family_writes_through_the_path_layer() {
        let document = table_document(&["users", "orders"]);
        let variables = Variables::new(&[]);
        let filters = [
            (".users[7].name = \"Ada\" | .users[7].name", serde_json::json!("Ada")),
            (".users[7].name |= \"Ada\" | .users[7].name", serde_json::json!("Ada")),
            (".users[7].id += 1 | .users[7].id", serde_json::json!(8)),
            (".users[7].name //= \"Ada\" | .users[7].name", serde_json::json!("name-7")),
        ];

        for (filter, expected) in filters {
            let values = evaluate(&document, filter, &variables).expect("query succeeds");
            assert_eq!(values.len(), 1, "{filter}");
            assert_eq!(values[0].to_json_value(), expected, "{filter}");
        }
    }

    /// The named row changes, its neighbours and the untouched table do not.
    #[test]
    fn an_assignment_edits_the_row_it_names() {
        let document = table_document(&["users", "orders"]);
        let variables = Variables::new(&[]);
        let filter = ".users[7].name = \"Ada\" | [.users[6].name,.users[7].name,.orders[7].name]";
        let values = evaluate(&document, filter, &variables).expect("query succeeds");

        assert_eq!(
            values[0].to_json_value(),
            serde_json::json!(["name-6", "Ada", "name-7"])
        );
    }
}
