use reddb_io_toon::Value;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    Alternative(Box<Expr>, Box<Expr>),
    Array(Vec<Expr>),
    Bind(Box<Expr>, Pattern, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Comma(Vec<Expr>),
    Conditional(Vec<(Expr, Expr)>, Box<Expr>),
    Empty,
    Environment,
    Field(Box<Expr>, String),
    Foreach {
        generator: Box<Expr>,
        pattern: Pattern,
        initial: Box<Expr>,
        update: Box<Expr>,
        extract: Box<Expr>,
    },
    Identity,
    Index(Box<Expr>, Box<Expr>),
    Iter(Box<Expr>),
    Literal(Value),
    Object(Vec<(String, Expr)>),
    Optional(Box<Expr>),
    Pipe(Box<Expr>, Box<Expr>),
    Reduce {
        generator: Box<Expr>,
        pattern: Pattern,
        initial: Box<Expr>,
        update: Box<Expr>,
    },
    Slice(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),
    Try(Box<Expr>, Option<Box<Expr>>),
    Variable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Pattern {
    Array(Vec<Pattern>),
    Object(Vec<(String, Pattern)>),
    Variable(String),
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
