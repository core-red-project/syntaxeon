//! The intermediate representation: each stanza lowered to a flat list of stack operations
//! with names resolved to slots and control flow to jumps (ADR-021). `lower` accepts only
//! programs that passed `syntaxeon-check`; anything else is `SXP-599`.

use std::collections::HashMap;
use std::fmt::{self, Write as _};

use syntaxeon_core::{BinOp, Expr, ExprKind, Program, Span, Stmt, StmtKind, Target, UnOp};

use crate::RuntimeError;

/// Every builtin the VM can call (`spec/SEMANTIC_CORE.md` §8).
const BUILTINS: [&str; 12] = [
    "speak",
    "speakln",
    "hear",
    "has_input",
    "reads_as_integer",
    "to_code",
    "to_letter",
    "length",
    "append",
    "to_string",
    "to_integer",
    "to_float",
];

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Char(char),
    /// The empty list.
    Nil,
    Load(usize),
    /// Pops into a slot.
    Store(usize),
    /// Pops the index, then the value, and stores into the list in a slot.
    StoreAt(usize),
    Pop,
    Neg,
    Not,
    Bin(BinOp),
    /// Pops `n` values into a new list, first pushed first.
    List(usize),
    Index,
    Jump(usize),
    /// Pops a truth value; jumps when it is false.
    JumpIfNot(usize),
    /// Calls function number `.0` with the `.1` values on the stack.
    Call(usize, usize),
    Builtin(&'static str, usize),
    Return,
    ReturnUnit,
}

/// Where an operation came from, for error reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    pub line: usize,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: usize,
    /// Parameters, declarations and the hidden counters of `repeat` loops.
    pub slots: usize,
    pub code: Vec<Op>,
    /// One per operation.
    pub locs: Vec<Loc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ir {
    /// The first function is the entry stanza.
    pub functions: Vec<Function>,
}

struct Lower<'a> {
    functions: &'a HashMap<&'a str, usize>,
    code: Vec<Op>,
    locs: Vec<Loc>,
    slots: usize,
    scopes: Vec<HashMap<String, usize>>,
    loc: Loc,
}

fn bad(what: impl Into<String>, line: usize) -> RuntimeError {
    RuntimeError {
        code: "SXP-599",
        message: format!(
            "internal error: {} (the program was not type-checked)",
            what.into()
        ),
        line,
        span: None,
    }
}

impl Lower<'_> {
    fn emit(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.locs.push(self.loc);
        self.code.len() - 1
    }

    fn here(&self) -> usize {
        self.code.len()
    }

    fn patch(&mut self, at: usize) {
        let to = self.here();
        if let Op::Jump(t) | Op::JumpIfNot(t) = &mut self.code[at] {
            *t = to;
        }
    }

    fn slot(&mut self) -> usize {
        self.slots += 1;
        self.slots - 1
    }

    fn lookup(&self, name: &str) -> Result<usize, RuntimeError> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name).copied())
            .ok_or_else(|| bad(format!("unbound name {name:?}"), self.loc.line))
    }

    fn block(&mut self, body: &[Stmt]) -> Result<(), RuntimeError> {
        self.scopes.push(HashMap::new());
        let r = body.iter().try_for_each(|s| self.stmt(s));
        self.scopes.pop();
        r
    }

    fn at_stmt(&mut self, s: &Stmt) {
        self.loc = Loc {
            line: s.line,
            span: (!s.span.is_empty()).then_some(s.span),
        };
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), RuntimeError> {
        self.at_stmt(s);
        match &s.kind {
            StmtKind::Decl { name, init, .. } => {
                self.expr(init)?;
                self.at_stmt(s);
                let slot = self.slot();
                self.scopes
                    .last_mut()
                    .ok_or_else(|| bad("no scope", s.line))?
                    .insert(name.clone(), slot);
                self.emit(Op::Store(slot));
            }
            StmtKind::Assign { target, value } => {
                self.expr(value)?;
                match target {
                    Target::Name(n) => {
                        let slot = self.lookup(n)?;
                        self.at_stmt(s);
                        self.emit(Op::Store(slot));
                    }
                    Target::Index(n, i) => {
                        self.expr(i)?;
                        let slot = self.lookup(n)?;
                        self.at_stmt(s);
                        self.emit(Op::StoreAt(slot));
                    }
                }
            }
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond)?;
                self.at_stmt(s);
                let to_else = self.emit(Op::JumpIfNot(0));
                self.block(then)?;
                if let Some(o) = otherwise {
                    let to_end = self.emit(Op::Jump(0));
                    self.patch(to_else);
                    self.block(o)?;
                    self.patch(to_end);
                } else {
                    self.patch(to_else);
                }
            }
            StmtKind::While { cond, body } => {
                let top = self.here();
                self.expr(cond)?;
                self.at_stmt(s);
                let to_end = self.emit(Op::JumpIfNot(0));
                self.block(body)?;
                self.at_stmt(s);
                self.emit(Op::Jump(top));
                self.patch(to_end);
            }
            StmtKind::Repeat { count, body } => {
                // A hidden counter counts down; a negative count runs the body no times.
                self.expr(count)?;
                self.at_stmt(s);
                let counter = self.slot();
                self.emit(Op::Store(counter));
                let top = self.here();
                self.emit(Op::Load(counter));
                self.emit(Op::Int(0));
                self.emit(Op::Bin(BinOp::Gt));
                let to_end = self.emit(Op::JumpIfNot(0));
                self.block(body)?;
                self.at_stmt(s);
                self.emit(Op::Load(counter));
                self.emit(Op::Int(1));
                self.emit(Op::Bin(BinOp::Sub));
                self.emit(Op::Store(counter));
                self.emit(Op::Jump(top));
                self.patch(to_end);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.expr(e)?;
                    self.at_stmt(s);
                    self.emit(Op::Return);
                } else {
                    self.emit(Op::ReturnUnit);
                }
            }
            StmtKind::Expr(e) => {
                self.expr(e)?;
                self.at_stmt(s);
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> Result<(), RuntimeError> {
        let saved = self.loc;
        self.loc.span = (!e.span.is_empty()).then_some(e.span);
        let r = self.expr_kind(e);
        self.loc = saved;
        r
    }

    fn expr_kind(&mut self, e: &Expr) -> Result<(), RuntimeError> {
        match &e.kind {
            ExprKind::Int(n) => {
                self.emit(Op::Int(*n));
            }
            ExprKind::Float(f) => {
                self.emit(Op::Float(*f));
            }
            ExprKind::Bool(b) => {
                self.emit(Op::Bool(*b));
            }
            ExprKind::Str(s) => {
                self.emit(Op::Str(s.clone()));
            }
            ExprKind::Char(c) => {
                self.emit(Op::Char(*c));
            }
            ExprKind::EmptyList(_) => {
                self.emit(Op::Nil);
            }
            ExprKind::Var(n) => {
                let slot = self.lookup(n)?;
                self.emit(Op::Load(slot));
            }
            ExprKind::Unary(op, a) => {
                self.expr(a)?;
                self.emit(match op {
                    UnOp::Neg => Op::Neg,
                    UnOp::Not => Op::Not,
                });
            }
            // `and` / `or` stop at the first operand that decides (SEMANTIC_CORE §6).
            ExprKind::Binary(BinOp::And, a, b) => {
                self.expr(a)?;
                let to_false = self.emit(Op::JumpIfNot(0));
                self.expr(b)?;
                let to_end = self.emit(Op::Jump(0));
                self.patch(to_false);
                self.emit(Op::Bool(false));
                self.patch(to_end);
            }
            ExprKind::Binary(BinOp::Or, a, b) => {
                self.expr(a)?;
                let to_right = self.emit(Op::JumpIfNot(0));
                self.emit(Op::Bool(true));
                let to_end = self.emit(Op::Jump(0));
                self.patch(to_right);
                self.expr(b)?;
                self.patch(to_end);
            }
            ExprKind::Binary(op, a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                self.emit(Op::Bin(*op));
            }
            ExprKind::List(items) => {
                for i in items {
                    self.expr(i)?;
                }
                self.emit(Op::List(items.len()));
            }
            ExprKind::Index(l, i) => {
                self.expr(l)?;
                self.expr(i)?;
                self.emit(Op::Index);
            }
            ExprKind::Call(name, args) => {
                for a in args {
                    self.expr(a)?;
                }
                if let Some(&f) = self.functions.get(name.as_str()) {
                    self.emit(Op::Call(f, args.len()));
                } else if let Some(b) = BUILTINS.iter().find(|b| **b == name) {
                    self.emit(Op::Builtin(b, args.len()));
                } else {
                    return Err(bad(format!("unknown function {name:?}"), self.loc.line));
                }
            }
        }
        Ok(())
    }
}

/// Lowers a checked program.
///
/// # Errors
/// `SXP-599` when the program refers to something the checker would have refused.
pub fn lower(program: &Program) -> Result<Ir, RuntimeError> {
    let functions: HashMap<&str, usize> = program
        .stanzas
        .iter()
        .enumerate()
        .map(|(i, s)| (s.title.as_str(), i))
        .collect();
    let mut out = Vec::new();
    for st in &program.stanzas {
        let mut l = Lower {
            functions: &functions,
            code: Vec::new(),
            locs: Vec::new(),
            slots: st.params.len(),
            scopes: vec![
                st.params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p.name.clone(), i))
                    .collect(),
            ],
            loc: Loc {
                line: st.line,
                span: None,
            },
        };
        for s in &st.body {
            l.stmt(s)?;
        }
        l.loc = Loc {
            line: st.line,
            span: None,
        };
        l.emit(Op::ReturnUnit);
        out.push(Function {
            name: st.title.clone(),
            params: st.params.len(),
            slots: l.slots,
            code: l.code,
            locs: l.locs,
        });
    }
    Ok(Ir { functions: out })
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Int(n) => write!(f, "int {n}"),
            Op::Float(x) => write!(f, "float {x:?}"),
            Op::Bool(b) => write!(f, "bool {b}"),
            Op::Str(s) => write!(f, "str {s:?}"),
            Op::Char(c) => write!(f, "char {c:?}"),
            Op::Nil => f.write_str("nil"),
            Op::Load(s) => write!(f, "load ${s}"),
            Op::Store(s) => write!(f, "store ${s}"),
            Op::StoreAt(s) => write!(f, "store_at ${s}"),
            Op::Pop => f.write_str("pop"),
            Op::Neg => f.write_str("neg"),
            Op::Not => f.write_str("not"),
            Op::Bin(op) => write!(f, "{}", format!("{op:?}").to_lowercase()),
            Op::List(n) => write!(f, "list {n}"),
            Op::Index => f.write_str("index"),
            Op::Jump(t) => write!(f, "jump @{t}"),
            Op::JumpIfNot(t) => write!(f, "jump_if_not @{t}"),
            Op::Call(func, n) => write!(f, "call fn{func} argc={n}"),
            Op::Builtin(name, n) => write!(f, "builtin {name} argc={n}"),
            Op::Return => f.write_str("return"),
            Op::ReturnUnit => f.write_str("return_unit"),
        }
    }
}

impl fmt::Display for Ir {
    /// The listing `syntaxeon ir` prints: one block per function, one operation per line with
    /// the source line it came from when that changes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, func) in self.functions.iter().enumerate() {
            let mut head = format!(
                "fn{n} {:?} params={} slots={}",
                func.name, func.params, func.slots
            );
            if n == 0 {
                head.push_str(" entry");
            }
            writeln!(f, "{head}")?;
            let mut last = 0;
            for (i, (op, loc)) in func.code.iter().zip(&func.locs).enumerate() {
                let mut line = String::new();
                if loc.line == last {
                    line.push_str("      ");
                } else {
                    let _ = write!(line, "L{:<4} ", loc.line);
                    last = loc.line;
                }
                writeln!(f, "  {i:>4}  {line}{op}")?;
            }
        }
        Ok(())
    }
}
