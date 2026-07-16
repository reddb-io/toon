#[derive(Debug, Clone, PartialEq)]
enum LexToken {
    Colon,
    Comma,
    Dot,
    EqualEqual,
    Greater,
    GreaterEqual,
    Ident(String),
    LBrace,
    LBracket,
    LParen,
    Less,
    LessEqual,
    Minus,
    NotEqual,
    Number(String),
    Pipe,
    Plus,
    RBrace,
    RBracket,
    RParen,
    Slash,
    Star,
    String(String),
}

struct Parser {
    tokens: Vec<LexToken>,
    index: usize,
}

impl Parser {
    fn new(query: &str) -> Result<Self, String> {
        Ok(Self {
            tokens: lex(query)?,
            index: 0,
        })
    }

    fn parse(mut self) -> Result<Expr, String> {
        let expression = self.parse_pipe()?;
        if self.peek().is_some() {
            return Err("unexpected trailing filter input".to_owned());
        }
        Ok(expression)
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_comma()?;
        while self.consume(&LexToken::Pipe) {
            let right = self.parse_comma()?;
            expression = Expr::Pipe(Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn parse_comma(&mut self) -> Result<Expr, String> {
        let mut expressions = vec![self.parse_comparison()?];
        while self.consume(&LexToken::Comma) {
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
            let operator = if self.consume(&LexToken::Plus) {
                BinaryOp::Add
            } else if self.consume(&LexToken::Minus) {
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
            let operator = if self.consume(&LexToken::Star) {
                BinaryOp::Multiply
            } else if self.consume(&LexToken::Slash) {
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
        if self.consume(&LexToken::Minus) {
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
            if self.consume(&LexToken::Dot) {
                let key = self.expect_ident()?;
                expression = Expr::Field(Box::new(expression), key);
                continue;
            }

            if self.consume(&LexToken::LBracket) {
                if self.consume(&LexToken::RBracket) {
                    expression = Expr::Iter(Box::new(expression));
                    continue;
                }

                let start = if self.peek() == Some(&LexToken::Colon) {
                    None
                } else {
                    Some(self.expect_usize()?)
                };
                if self.consume(&LexToken::Colon) {
                    let end = if self.peek() == Some(&LexToken::RBracket) {
                        None
                    } else {
                        Some(self.expect_usize()?)
                    };
                    self.expect(LexToken::RBracket)?;
                    expression = Expr::Slice(Box::new(expression), start, end);
                } else {
                    let index = start.ok_or_else(|| "expected array index".to_owned())?;
                    self.expect(LexToken::RBracket)?;
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
            Some(LexToken::Dot) => {
                let mut expression = Expr::Identity;
                if matches!(self.peek(), Some(LexToken::Ident(_))) {
                    let key = self.expect_ident()?;
                    expression = Expr::Field(Box::new(expression), key);
                }
                Ok(expression)
            }
            Some(LexToken::Ident(value)) => self.parse_identifier(value),
            Some(LexToken::LBracket) => self.parse_array_constructor(),
            Some(LexToken::LBrace) => self.parse_object_constructor(),
            Some(LexToken::LParen) => {
                let expression = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(expression)
            }
            Some(LexToken::Number(value)) => Ok(Expr::Literal(Value::Number(value))),
            Some(LexToken::String(value)) => Ok(Expr::Literal(Value::String(value))),
            token => Err(format!("unexpected token `{token:?}`")),
        }
    }

    fn parse_identifier(&mut self, value: String) -> Result<Expr, String> {
        match value.as_str() {
            "add" => Ok(Expr::Builtin(Builtin::Add)),
            "false" => Ok(Expr::Literal(Value::Bool(false))),
            "from_entries" => Ok(Expr::Builtin(Builtin::FromEntries)),
            "group_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::GroupBy(Box::new(filter))))
            }
            "has" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Has(Box::new(filter))))
            }
            "join" => {
                self.expect(LexToken::LParen)?;
                let separator = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Join(Box::new(separator))))
            }
            "keys" => Ok(Expr::Builtin(Builtin::Keys)),
            "length" => Ok(Expr::Builtin(Builtin::Length)),
            "map" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Map(Box::new(filter))))
            }
            "max_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::MaxBy(Box::new(filter))))
            }
            "min_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::MinBy(Box::new(filter))))
            }
            "null" => Ok(Expr::Literal(Value::Null)),
            "select" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Select(Box::new(filter))))
            }
            "sort_by" => {
                self.expect(LexToken::LParen)?;
                let filter = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::SortBy(Box::new(filter))))
            }
            "split" => {
                self.expect(LexToken::LParen)?;
                let separator = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Split(Box::new(separator))))
            }
            "test" => {
                self.expect(LexToken::LParen)?;
                let pattern = self.parse_pipe()?;
                self.expect(LexToken::RParen)?;
                Ok(Expr::Builtin(Builtin::Test(Box::new(pattern))))
            }
            "to_entries" => Ok(Expr::Builtin(Builtin::ToEntries)),
            "true" => Ok(Expr::Literal(Value::Bool(true))),
            "unique" => Ok(Expr::Builtin(Builtin::Unique)),
            _ => Err(format!("unsupported identifier `{value}`")),
        }
    }

    fn parse_array_constructor(&mut self) -> Result<Expr, String> {
        if self.consume(&LexToken::RBracket) {
            return Ok(Expr::Array(Vec::new()));
        }

        let mut items = Vec::new();
        loop {
            items.push(self.parse_pipe_item()?);
            if self.consume(&LexToken::Comma) {
                continue;
            }
            self.expect(LexToken::RBracket)?;
            break;
        }
        Ok(Expr::Array(items))
    }

    fn parse_object_constructor(&mut self) -> Result<Expr, String> {
        if self.consume(&LexToken::RBrace) {
            return Ok(Expr::Object(Vec::new()));
        }

        let mut fields = Vec::new();
        loop {
            let key = match self.next() {
                Some(LexToken::Ident(value)) | Some(LexToken::String(value)) => value,
                token => return Err(format!("expected object key, got `{token:?}`")),
            };
            self.expect(LexToken::Colon)?;
            fields.push((key, self.parse_pipe_item()?));
            if self.consume(&LexToken::Comma) {
                continue;
            }
            self.expect(LexToken::RBrace)?;
            break;
        }
        Ok(Expr::Object(fields))
    }

    fn parse_pipe_item(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_comparison()?;
        while self.consume(&LexToken::Pipe) {
            let right = self.parse_comparison()?;
            expression = Expr::Pipe(Box::new(expression), Box::new(right));
        }
        Ok(expression)
    }

    fn match_comparison_operator(&mut self) -> Option<BinaryOp> {
        let operator = match self.peek()? {
            LexToken::EqualEqual => BinaryOp::Equal,
            LexToken::Greater => BinaryOp::Greater,
            LexToken::GreaterEqual => BinaryOp::GreaterEqual,
            LexToken::Less => BinaryOp::Less,
            LexToken::LessEqual => BinaryOp::LessEqual,
            LexToken::NotEqual => BinaryOp::NotEqual,
            _ => return None,
        };
        self.index += 1;
        Some(operator)
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.next() {
            Some(LexToken::Ident(value)) => Ok(value),
            token => Err(format!("expected identifier, got `{token:?}`")),
        }
    }

    fn expect_usize(&mut self) -> Result<usize, String> {
        match self.next() {
            Some(LexToken::Number(value)) => parse_usize(&value),
            token => Err(format!("expected array index, got `{token:?}`")),
        }
    }

    fn expect(&mut self, expected: LexToken) -> Result<(), String> {
        let actual = self.next();
        if actual == Some(expected.clone()) {
            Ok(())
        } else {
            Err(format!("expected `{expected:?}`, got `{actual:?}`"))
        }
    }

    fn consume(&mut self, expected: &LexToken) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn next(&mut self) -> Option<LexToken> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    fn peek(&self) -> Option<&LexToken> {
        self.tokens.get(self.index)
    }
}

fn lex(query: &str) -> Result<Vec<LexToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = query.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        match character {
            character if character.is_whitespace() => {}
            ':' => tokens.push(LexToken::Colon),
            ',' => tokens.push(LexToken::Comma),
            '.' => tokens.push(LexToken::Dot),
            '|' => tokens.push(LexToken::Pipe),
            '+' => tokens.push(LexToken::Plus),
            '-' => tokens.push(LexToken::Minus),
            '*' => tokens.push(LexToken::Star),
            '/' => tokens.push(LexToken::Slash),
            '(' => tokens.push(LexToken::LParen),
            ')' => tokens.push(LexToken::RParen),
            '[' => tokens.push(LexToken::LBracket),
            ']' => tokens.push(LexToken::RBracket),
            '{' => tokens.push(LexToken::LBrace),
            '}' => tokens.push(LexToken::RBrace),
            '=' => {
                expect_char(&mut chars, '=')?;
                tokens.push(LexToken::EqualEqual);
            }
            '!' => {
                expect_char(&mut chars, '=')?;
                tokens.push(LexToken::NotEqual);
            }
            '<' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(LexToken::LessEqual);
                } else {
                    tokens.push(LexToken::Less);
                }
            }
            '>' => {
                if consume_char(&mut chars, '=') {
                    tokens.push(LexToken::GreaterEqual);
                } else {
                    tokens.push(LexToken::Greater);
                }
            }
            '"' => {
                let (value, end) = read_string(query, index)?;
                tokens.push(LexToken::String(value));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            character if character.is_ascii_digit() => {
                let end = read_number_end(query, index);
                tokens.push(LexToken::Number(query[index..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            character if is_ident_start(character) => {
                let end = read_ident_end(query, index);
                tokens.push(LexToken::Ident(query[index..end].to_owned()));
                while matches!(chars.peek(), Some((next_index, _)) if *next_index < end) {
                    chars.next();
                }
            }
            _ => return Err(format!("unsupported character `{character}`")),
        }
    }

    Ok(tokens)
}

fn expect_char(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    expected: char,
) -> Result<(), String> {
    if consume_char(chars, expected) {
        Ok(())
    } else {
        Err(format!("expected `{expected}`"))
    }
}

fn consume_char(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    expected: char,
) -> bool {
    if matches!(chars.peek(), Some((_, character)) if *character == expected) {
        chars.next();
        true
    } else {
        false
    }
}

fn read_string(query: &str, start: usize) -> Result<(String, usize), String> {
    let mut escaped = false;
    for (index, character) in query[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => {
                let end = start + 1 + index + 1;
                return serde_json::from_str(&query[start..end])
                    .map(|value| (value, end))
                    .map_err(|error| format!("invalid string literal: {error}"));
            }
            _ => {}
        }
    }
    Err("unterminated string literal".to_owned())
}

fn read_number_end(query: &str, start: usize) -> usize {
    let mut end = start;
    for (index, character) in query[start..].char_indices() {
        if index == 0 || character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E') {
            end = start + index + character.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn read_ident_end(query: &str, start: usize) -> usize {
    let mut end = start;
    for (index, character) in query[start..].char_indices() {
        if index == 0 || is_ident_continue(character) {
            end = start + index + character.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn is_ident_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_ident_continue(character: char) -> bool {
    is_ident_start(character) || character.is_ascii_digit() || character == '-'
}

fn format_values(values: &[Value], options: &Options) -> Result<String, String> {
    if options.output_format == Format::Toonl {
        return encode_toonl_values(values).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for value in values {
        if options.raw_output {
            if let Value::String(value) = value {
                output.push_str(value);
                output.push('\n');
                continue;
            }
        }

        match options.output_format {
            Format::Json => {
                output.push_str(
                    &value
                        .to_json_string(options.compact)
                        .map_err(|error| error.to_string())?,
                );
                output.push('\n');
            }
            Format::Toon => {
                output.push_str(
                    &value
                        .try_to_toon_with_options(EncodeOptions {
                            nested_tabular_headers: options.nested_tabular_headers,
                            keyed_map_collapse: options.keyed_map_collapse,
                            primitive_array_columns: options.primitive_array_columns,
                            object_array_columns: options.object_array_columns,
                            cyclic_discriminated_arrays: options.cyclic_discriminated_arrays,
                            delimiter: options.delimiter,
                            ..EncodeOptions::default()
                        })
                        .map_err(|error| error.to_string())?,
                );
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            Format::Toonl => unreachable!("TOONL output is handled before the loop"),
            Format::Yaml => unreachable!("YAML output is not supported"),
        }
    }
    Ok(output)
}
