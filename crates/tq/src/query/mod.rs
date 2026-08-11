use reddb_io_toon::Value;

mod ast;
mod builtins;
mod eval;
mod lexer;
mod parser;
mod values;

pub(crate) fn evaluate(document: &Value, query: &str) -> Result<Vec<Value>, String> {
    let expression = parser::Parser::new(query)?.parse()?;
    expression.eval(document, &eval::Env::default())
}
