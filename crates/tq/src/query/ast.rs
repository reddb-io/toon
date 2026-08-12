use std::rc::Rc;

use reddb_io_toon::Value;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    Alternative(Box<Expr>, Box<Expr>),
    Array(Vec<Expr>),
    /// `lhs op= rhs`: an edit of the paths `lhs` selects.
    Assign(AssignOp, Box<Expr>, Box<Expr>),
    Bind(Box<Expr>, Pattern, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Comma(Vec<Expr>),
    Conditional(Vec<(Expr, Expr)>, Box<Expr>),
    /// A `def name(params): body; rest` scope. The body is shared so a call can
    /// clone the closure without copying the filter it defines.
    Def {
        name: String,
        parameters: Vec<String>,
        body: Rc<Expr>,
        rest: Box<Expr>,
    },
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

/// jq's assignment family. Every member edits the paths its left-hand side
/// selects; they differ only in what is written at each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AssignOp {
    /// `=`: the right-hand value, verbatim.
    Set,
    /// `|=`: the first value the right-hand filter produces from the current
    /// one, or nothing, which deletes the path.
    Update,
    /// `+=`, `-=`, `*=`, `/=`, `%=`: the operator applied to the current value
    /// and the right-hand value.
    Arithmetic(BinaryOp),
    /// `//=`: the right-hand value, but only where the current one is falsy.
    Alternative,
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
