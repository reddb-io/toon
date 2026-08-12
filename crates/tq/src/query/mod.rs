use reddb_io_toon::Value;

mod assign;
mod ast;
mod builtins;
pub(crate) mod compat;
mod eval;
mod halt;
mod indexing;
mod inputs;
mod lexer;
mod ordering;
mod parser;
mod paths;

pub(crate) use halt::Halt;
pub(crate) use inputs::Inputs;

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
    evaluate_reading(document, query, variables, None)
}

/// The same evaluation, with the reader the remaining documents come from.
/// `input` and `inputs` draw from it, so a filter that reads ahead moves the
/// same cursor the caller's loop is walking.
pub(crate) fn evaluate_reading(
    document: &Value,
    query: &str,
    variables: &Variables,
    inputs: Option<&Inputs>,
) -> Result<Vec<Value>, String> {
    let expression = parser::Parser::new(query)?.parse()?;
    let env = eval::Env::with_variables(&variables.bindings);
    let env = match inputs {
        Some(inputs) => env.reading(inputs),
        None => env,
    };
    expression.eval(document, &env)
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

    fn numbers(values: &[i64]) -> Inputs {
        let rows = values
            .iter()
            .map(|value| Ok(Value::Number(value.to_string())))
            .collect::<Vec<_>>();
        Inputs::new(rows.into_iter())
    }

    /// `input` and `inputs` share the caller's reader rather than a copy of it:
    /// each read moves the one cursor, and what one filter took is gone.
    #[test]
    fn input_and_inputs_draw_from_one_shared_reader() {
        let inputs = numbers(&[1, 2, 3]);
        let variables = Variables::new(&[]);

        let first = evaluate_reading(&Value::Null, "input", &variables, Some(&inputs))
            .expect("query succeeds");
        assert_eq!(first[0].to_json_value(), serde_json::json!(1));

        let rest = evaluate_reading(&Value::Null, "[inputs]", &variables, Some(&inputs))
            .expect("query succeeds");
        assert_eq!(rest[0].to_json_value(), serde_json::json!([2, 3]));

        assert!(inputs.next_input().is_none(), "the reader is exhausted");
        assert_eq!(format!("{inputs:?}"), "Inputs");
    }

    /// An exhausted reader is an error for `input` and simply the end for
    /// `inputs`, and a reader that fails reports where the filter read it.
    #[test]
    fn an_exhausted_or_failing_reader_reaches_the_filter() {
        let variables = Variables::new(&[]);

        let empty = numbers(&[]);
        let error = evaluate_reading(&Value::Null, "input", &variables, Some(&empty))
            .expect_err("there is nothing to read");
        assert_eq!(error, "No more inputs");

        let drained = evaluate_reading(&Value::Null, "[inputs]", &variables, Some(&empty))
            .expect("query succeeds");
        assert_eq!(drained[0].to_json_value(), serde_json::json!([]));

        let broken = Inputs::new(std::iter::once(Err("row 2: unreadable".to_owned())));
        let error = evaluate_reading(&Value::Null, "input", &variables, Some(&broken))
            .expect_err("the read fails");
        assert_eq!(error, "row 2: unreadable");
    }

    /// A halt leaves the query carrying the status and message the CLI turns
    /// back into an exit code and stderr text.
    #[test]
    fn a_halt_carries_its_status_and_message_out_of_the_query() {
        let variables = Variables::new(&[]);
        let document = Value::String("stop".to_owned());

        let error = evaluate(&document, "halt_error(3)", &variables).expect_err("the query halts");
        let halt = Halt::decode(&error).expect("the error carries a halt");

        assert_eq!(halt.code, 3);
        assert_eq!(halt.message, "stop");
    }
}
