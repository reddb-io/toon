use reddb_io_toon::Value;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    Alternative(Box<Expr>, Box<Expr>),
    Array(Vec<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Comma(Vec<Expr>),
    Field(Box<Expr>, String),
    Identity,
    Index(Box<Expr>, usize),
    Iter(Box<Expr>),
    Literal(Value),
    Object(Vec<(String, Expr)>),
    Pipe(Box<Expr>, Box<Expr>),
    Slice(Box<Expr>, Option<usize>, Option<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BinaryOp {
    Add,
    And,
    Subtract,
    Multiply,
    Or,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Modulo,
    Greater,
    GreaterEqual,
}
