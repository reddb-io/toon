use reddb_io_toon::Value;

mod ast;
mod builtins;
mod eval;
mod indexing;
mod lexer;
mod parser;

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
