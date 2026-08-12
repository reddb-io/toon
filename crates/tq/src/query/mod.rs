use reddb_io_toon::Value;

mod assign;
mod ast;
mod builtins;
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

    /// The same contract seen through the assignment family, which is where a
    /// query most often writes. Two tables sit side by side and only one is
    /// assigned into: locating the target reads its one row, materialising it
    /// reads that table in full, and the untouched table is never decoded —
    /// so the total stays a single table's worth rather than both.
    #[test]
    fn an_assignment_materializes_only_the_table_it_touches() {
        let _counting = counting();
        let document = table_document(&["users", "orders"]);
        let variables = Variables::new(&[]);
        let filters = [
            // `=` never reads what it overwrites, so beyond materialising the
            // table it decodes only the row the path walk touched.
            (".users[7].name = \"Ada\"", ROWS + 1),
            // The update operators combine with the current value, which reads
            // that one row a second time.
            (".users[7].name |= \"Ada\"", ROWS + 2),
            (".users[7].id += 1", ROWS + 2),
            (".users[7].name //= \"Ada\"", ROWS + 2),
        ];

        for (filter, decodes) in filters {
            reddb_io_toon::reset_tabular_row_decode_count_for_tests();
            let values = evaluate(&document, filter, &variables).expect("query succeeds");
            assert_eq!(values.len(), 1, "{filter}");
            assert_eq!(
                reddb_io_toon::tabular_row_decode_count_for_tests(),
                decodes,
                "{filter}"
            );
        }
    }

    /// The edit itself, checked separately from how much it decoded: the named
    /// row changes, its neighbours and the untouched table do not.
    #[test]
    fn an_assignment_edits_the_row_it_names() {
        let _counting = counting();
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
