//! Calculator MCP server — exposes a `calculator_arithmetic` tool.
//!
//! Speaks MCP revision 2026-07-28 over the stdio transport, so it can be
//! launched by any MCP client that supports that revision.

use reddb_io_toon_rpc_mcp::{CallToolResult, McpError, McpResult, McpService, ServerInfo, Tool};
use serde_json::{json, Value};

struct CalculatorMcp;

impl McpService for CalculatorMcp {
    fn server_info(&self) -> ServerInfo {
        ServerInfo {
            name: "calculator".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Calculator".into()),
        }
    }

    fn instructions(&self) -> Option<String> {
        Some("Evaluates arithmetic expressions with + - * / and parentheses.".into())
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![Tool {
            name: "calculator_arithmetic".into(),
            title: Some("Calculator".into()),
            description: Some(
                "Evaluate an arithmetic expression using + - * / and parentheses".into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Arithmetic expression to evaluate, e.g. '2 + 3 * 4'"
                    }
                },
                "required": ["expression"],
                "additionalProperties": false
            }),
            output_schema: None,
            annotations: None,
        }]
    }

    fn call_tool(&self, _name: &str, args: Value) -> CallToolResult {
        // An unknown tool never reaches here: the dispatcher rejects it with a
        // JSON-RPC error before calling the service.
        let expression = match args.get("expression").and_then(Value::as_str) {
            Some(s) => s,
            None => return CallToolResult::error("missing 'expression' argument"),
        };

        match evaluate(expression) {
            Ok(result) => CallToolResult::text(format!("= {result}")),
            Err(e) => CallToolResult::error(format!("evaluation error: {e}")),
        }
    }
}

fn evaluate(expr: &str) -> McpResult<f64> {
    // Toy evaluator — supports `+ - * /` and parentheses via simple Shunting-yard
    // For production use, swap for a real expression engine.
    let tokens = tokenize(expr)?;
    let rpn = shunting_yard(&tokens)?;
    let result = eval_rpn(&rpn)?;
    Ok(result)
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize(s: &str) -> McpResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_digit() || c == '.' {
            let mut n = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' {
                    n.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(
                n.parse()
                    .map_err(|_| McpError::InvalidParams("bad number".into()))?,
            ));
        } else if "+-*/".contains(c) {
            tokens.push(Token::Op(c));
            chars.next();
        } else if c == '(' {
            tokens.push(Token::LParen);
            chars.next();
        } else if c == ')' {
            tokens.push(Token::RParen);
            chars.next();
        } else {
            return Err(McpError::InvalidParams(format!("unexpected: {}", c)));
        }
    }
    Ok(tokens)
}

fn precedence(op: char) -> i32 {
    match op {
        '+' | '-' => 1,
        '*' | '/' => 2,
        _ => 0,
    }
}

fn shunting_yard(tokens: &[Token]) -> McpResult<Vec<Token>> {
    let mut output = Vec::new();
    let mut ops: Vec<Token> = Vec::new();
    for t in tokens {
        match t {
            Token::Number(_) => output.push(t.clone()),
            Token::Op(c) => {
                while let Some(top) = ops.last() {
                    if let Token::Op(tc) = top {
                        if precedence(*tc) >= precedence(*c) {
                            output.push(ops.pop().unwrap());
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                ops.push(t.clone());
            }
            Token::LParen => ops.push(t.clone()),
            Token::RParen => {
                let mut found = false;
                while let Some(top) = ops.pop() {
                    if matches!(top, Token::LParen) {
                        found = true;
                        break;
                    }
                    output.push(top);
                }
                if !found {
                    return Err(McpError::InvalidParams("mismatched parens".into()));
                }
            }
        }
    }
    while let Some(top) = ops.pop() {
        output.push(top);
    }
    Ok(output)
}

fn eval_rpn(rpn: &[Token]) -> McpResult<f64> {
    let mut stack: Vec<f64> = Vec::new();
    for t in rpn {
        match t {
            Token::Number(n) => stack.push(*n),
            Token::Op(op) => {
                let b = stack
                    .pop()
                    .ok_or_else(|| McpError::InvalidParams("missing operand".into()))?;
                let a = stack
                    .pop()
                    .ok_or_else(|| McpError::InvalidParams("missing operand".into()))?;
                let r = match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => {
                        if b == 0.0 {
                            return Err(McpError::InvalidParams("division by zero".into()));
                        }
                        a / b
                    }
                    _ => return Err(McpError::InvalidParams("unknown op".into())),
                };
                stack.push(r);
            }
            _ => return Err(McpError::InvalidParams("invalid token in rpn".into())),
        }
    }
    stack
        .pop()
        .ok_or_else(|| McpError::InvalidParams("empty expression".into()))
}

fn main() {
    reddb_io_toon_rpc_mcp::serve_stdio(CalculatorMcp).expect("stdio server failed");
}
