//! Canonical AST and types. Normative reference: `spec/SEMANTIC_CORE.md` §3–§4.
//!
//! Statements and stanzas carry a source line; every expression carries the byte span of the
//! words it was read from. Spans never take part in equality, so two readings of one text, or
//! a program before and after `fmt`, compare equal when their structure does.

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

#[derive(Clone, PartialEq)]
pub struct Stanza {
    pub id: usize,
    pub title: Name,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
    pub line: usize,
    /// The stanza header.
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub ty: Type,
}

#[derive(Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
    /// The whole sentence (or block header) the statement was read from.
    pub span: Span,
    /// What an error about the statement itself is about: the declared name of a `Decl`, the
    /// target of an `Assign`; otherwise the same as `span`. Empty when built without a source.
    pub focus: Span,
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

/// Byte range into the source text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Whether the span covers text; spans of expressions built without a source are empty.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.end <= self.start
    }
}

/// An expression: its shape and where it was written. Equality and `Debug` look at the shape
/// only (see the crate docs).
#[derive(Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl std::fmt::Debug for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

impl From<ExprKind> for Expr {
    fn from(kind: ExprKind) -> Self {
        Expr {
            kind,
            span: Span::default(),
        }
    }
}

// Constructors named like the variants, so building `Expr::Int(1)` reads as before spans existed.
#[allow(non_snake_case)] // mirrors the `ExprKind` variant names on purpose
impl Expr {
    #[must_use]
    pub fn Int(n: i64) -> Expr {
        ExprKind::Int(n).into()
    }
    #[must_use]
    pub fn Float(x: f64) -> Expr {
        ExprKind::Float(x).into()
    }
    #[must_use]
    pub fn Bool(b: bool) -> Expr {
        ExprKind::Bool(b).into()
    }
    #[must_use]
    pub fn Str(s: String) -> Expr {
        ExprKind::Str(s).into()
    }
    #[must_use]
    pub fn Char(c: char) -> Expr {
        ExprKind::Char(c).into()
    }
    #[must_use]
    pub fn Var(n: Name) -> Expr {
        ExprKind::Var(n).into()
    }
    #[must_use]
    pub fn Unary(op: UnOp, a: Box<Expr>) -> Expr {
        ExprKind::Unary(op, a).into()
    }
    #[must_use]
    pub fn Binary(op: BinOp, a: Box<Expr>, b: Box<Expr>) -> Expr {
        ExprKind::Binary(op, a, b).into()
    }
    #[must_use]
    pub fn Call(n: Name, args: Vec<Expr>) -> Expr {
        ExprKind::Call(n, args).into()
    }
    #[must_use]
    pub fn List(xs: Vec<Expr>) -> Expr {
        ExprKind::List(xs).into()
    }
    #[must_use]
    pub fn Index(l: Box<Expr>, i: Box<Expr>) -> Expr {
        ExprKind::Index(l, i).into()
    }
    #[must_use]
    pub fn EmptyList(t: Type) -> Expr {
        ExprKind::EmptyList(t).into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
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

// `Debug` shows the structure and the line, not the byte spans, so the `ast` dump is the same
// whatever the whitespace of the source (spans are in the fields for tools that want them).
#[allow(clippy::missing_fields_in_debug)] // the spans are left out on purpose (see above)
impl std::fmt::Debug for Stmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stmt")
            .field("kind", &self.kind)
            .field("line", &self.line)
            .finish()
    }
}

#[allow(clippy::missing_fields_in_debug)] // the span is left out on purpose (see above)
impl std::fmt::Debug for Stanza {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stanza")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("params", &self.params)
            .field("ret", &self.ret)
            .field("body", &self.body)
            .field("line", &self.line)
            .finish()
    }
}
