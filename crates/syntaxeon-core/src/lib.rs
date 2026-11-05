//! Canonical AST and types. Normative reference: `spec/SEMANTIC_CORE.md` §3–§4.
//!
//! shortcut: only statements and stanzas carry a source line; expressions carry no span yet.
//! Add spans before the type checker needs expression-level diagnostics.

/// Normalized identifier (`spec/SEMANTIC_CORE.md` §2).
pub type Name = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Integer,
    Float,
    Boolean,
    String,
    Character,
    List(Box<Type>),
    Unit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub stanzas: Vec<Stanza>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stanza {
    pub id: usize,
    pub title: Name,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Decl {
        name: Name,
        ty: Option<Type>,
        mutable: bool,
        init: Expr,
    },
    Assign {
        target: Target,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Repeat {
        count: Expr,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Name(Name),
    Index(Name, Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Char(char),
    Var(Name),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(Name, Vec<Expr>),
    List(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    /// Parser-internal (`an empty list of T`). Never present in a returned `Program`:
    /// it is resolved into `List([])` by an annotated `Decl` or rejected as `SXP-108`.
    EmptyList(Type),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
