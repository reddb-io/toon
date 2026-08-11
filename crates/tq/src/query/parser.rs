use reddb_io_toon::Value;

use super::ast::{BinaryOp, Expr};
use super::builtins;
use super::lexer::{lex, Token};
use super::values::parse_usize;

pub(super) struct Parser {
    tokens: Vec<Token>,
    index: usize,
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
        let mut expression = self.parse_comma()?;
        while self.consume(&Token::Pipe) {
            let right = self.parse_comma()?;
            expression = Expr::Pipe(Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_comma(&mut self) -> Result<Expr, String> {
        let mut expressions = vec![self.parse_comparison()?];
        while self.consume(&Token::Comma) {
            expressions.push(self.parse_comparison()?);
        }
        if expressions.len() == 1 {
            Ok(expressions.pop().expect("one expression exists"))
        } else {
            Ok(Expr::Comma(expressions))
        }
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
            if self.consume(&Token::Dot) {
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
                    Some(self.expect_usize()?)
                };
                if self.consume(&Token::Colon) {
                    let end = if self.peek() == Some(&Token::RBracket) {
                        None
                    } else {
                        Some(self.expect_usize()?)
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

            break;
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Dot) => {
                let mut expression = Expr::Identity;
                if matches!(self.peek(), Some(Token::Ident(_))) {
                    let key = self.expect_ident()?;
                    expression = Expr::Field(Box::new(expression), key);
                }
                Ok(expression)
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
            token => Err(format!("unexpected token `{token:?}`")),
        }
    }

    fn parse_identifier(&mut self, name: String) -> Result<Expr, String> {
        match name.as_str() {
            "false" => Ok(Expr::Literal(Value::Bool(false))),
            "null" => Ok(Expr::Literal(Value::Null)),
            "true" => Ok(Expr::Literal(Value::Bool(true))),
            _ => self.parse_call(name),
        }
    }

    fn parse_call(&mut self, name: String) -> Result<Expr, String> {
        // Keep the historic bare spelling for zero-arity builtins. Leaving a
        // following parenthesis untouched preserves its existing diagnostic.
        if builtins::supports(&name, 0) {
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
        let mut expression = self.parse_comparison()?;
        while self.consume(&Token::Pipe) {
            let right = self.parse_comparison()?;
            expression = Expr::Pipe(Box::new(expression), Box::new(right));
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

    fn expect_usize(&mut self) -> Result<usize, String> {
        match self.next() {
            Some(Token::Number(value)) => parse_usize(&value),
            token => Err(format!("expected array index, got `{token:?}`")),
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

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }
}
