//! Evaluator. Normative reference: `spec/SEMANTIC_CORE.md` §6–§8, ADR-013.
//!
//! The program must already have passed `syntaxeon-check`; an ill-typed program that slips
//! through reports `SXP-599` instead of panicking.
//!
//! The interpreter recurses on the Rust stack, so callers must run it on a thread whose stack
//! can hold [`MAX_FRAMES`] Syntaxeon calls (the CLI uses 256 MiB).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::ops::{Deref, DerefMut};

use syntaxeon_core::{BinOp, Expr, ExprKind, Program, Span, Stanza, Stmt, StmtKind, Target};

mod ir;
mod rt;
mod vm;

pub use ir::{Ir, lower};
pub use vm::run as run_vm;

use rt::{Rt, Value, truthy};

/// Deterministic call-depth limit (`SXP-504`).
pub const MAX_FRAMES: usize = 10_000;

/// A runtime failure; the program aborts with it (`spec/SEMANTIC_CORE.md` §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: &'static str,
    pub message: String,
    pub line: usize,
    /// The expression being evaluated when it failed, if the failure is about one.
    pub span: Option<Span>,
}

type Res<T> = Result<T, RuntimeError>;
type Scopes = Vec<HashMap<String, Value>>;

enum Flow {
    Next,
    Return(Option<Value>),
}

struct Interp<'a> {
    fns: HashMap<&'a str, &'a Stanza>,
    rt: Rt<'a>,
    depth: usize,
}

impl<'a> Deref for Interp<'a> {
    type Target = Rt<'a>;
    fn deref(&self) -> &Rt<'a> {
        &self.rt
    }
}

impl DerefMut for Interp<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.rt
    }
}

/// Runs the entry stanza (the first one). Program output goes to `out`.
pub fn run(
    program: &Program,
    stdin: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<(), RuntimeError> {
    let Some(entry) = program.stanzas.first() else {
        return Err(RuntimeError {
            code: "SXP-599",
            message: "no entry stanza".into(),
            line: 0,
            span: None,
        });
    };
    let mut it = Interp {
        fns: program
            .stanzas
            .iter()
            .map(|s| (s.title.as_str(), s))
            .collect(),
        rt: Rt {
            stdin,
            out,
            line: entry.line,
            span: None,
        },
        depth: 0,
    };
    it.call(entry, Vec::new())?;
    it.rt
        .out
        .flush()
        .map_err(|e| it.err("SXP-507", format!("cannot write output: {e}")))
}

impl Interp<'_> {
    fn call(&mut self, st: &Stanza, args: Vec<Value>) -> Res<Value> {
        self.depth += 1;
        if self.depth > MAX_FRAMES {
            return Err(self.err("SXP-504", format!("call depth exceeds {MAX_FRAMES} frames")));
        }
        let params = st.params.iter().map(|p| p.name.clone()).zip(args).collect();
        let mut env: Scopes = vec![params];
        let flow = self.stmts(&mut env, &st.body)?;
        self.depth -= 1;
        Ok(match flow {
            Flow::Return(Some(v)) => v,
            _ => Value::Unit,
        })
    }

    fn block(&mut self, env: &mut Scopes, body: &[Stmt]) -> Res<Flow> {
        env.push(HashMap::new());
        let flow = self.stmts(env, body);
        env.pop();
        flow
    }

    fn stmts(&mut self, env: &mut Scopes, body: &[Stmt]) -> Res<Flow> {
        for s in body {
            if let Flow::Return(v) = self.stmt(env, s)? {
                return Ok(Flow::Return(v));
            }
        }
        Ok(Flow::Next)
    }

    fn stmt(&mut self, env: &mut Scopes, s: &Stmt) -> Res<Flow> {
        self.line = s.line;
        self.span = (!s.span.is_empty()).then_some(s.span);
        match &s.kind {
            StmtKind::Decl { name, init, .. } => {
                let v = self.eval(env, init)?;
                env.last_mut()
                    .ok_or_else(|| self.internal("no scope"))?
                    .insert(name.clone(), v);
            }
            StmtKind::Assign { target, value } => self.assign(env, target, value)?,
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                let c = self.eval(env, cond)?;
                if truthy(&c) {
                    return self.block(env, then);
                } else if let Some(o) = otherwise {
                    return self.block(env, o);
                }
            }
            StmtKind::While { cond, body } => loop {
                self.line = s.line;
                self.span = (!s.span.is_empty()).then_some(s.span);
                if !truthy(&self.eval(env, cond)?) {
                    break;
                }
                if let Flow::Return(v) = self.block(env, body)? {
                    return Ok(Flow::Return(v));
                }
            },
            StmtKind::Repeat { count, body } => {
                let Value::Int(n) = self.eval(env, count)? else {
                    return Err(self.internal("repeat count"));
                };
                for _ in 0..n.max(0) {
                    if let Flow::Return(v) = self.block(env, body)? {
                        return Ok(Flow::Return(v));
                    }
                }
            }
            StmtKind::Return(e) => {
                let v = e.as_ref().map(|e| self.eval(env, e)).transpose()?;
                return Ok(Flow::Return(v));
            }
            StmtKind::Expr(e) => {
                self.eval(env, e)?;
            }
        }
        Ok(Flow::Next)
    }

    fn assign(&mut self, env: &mut Scopes, target: &Target, value: &Expr) -> Res<()> {
        let v = self.eval(env, value)?;
        let (name, idx) = match target {
            Target::Name(n) => (n, None),
            Target::Index(n, i) => {
                let Value::Int(i) = self.eval(env, i)? else {
                    return Err(self.internal("index"));
                };
                (n, Some(i))
            }
        };
        let slot = env
            .iter_mut()
            .rev()
            .find_map(|s| s.get_mut(name))
            .ok_or_else(|| self.internal("assignment to unbound name"))?;
        match (idx, slot) {
            (None, slot) => *slot = v,
            (Some(i), Value::List(items)) => {
                let len = items.len();
                match usize::try_from(i).ok().and_then(|i| items.get_mut(i)) {
                    Some(cell) => *cell = v,
                    None => {
                        return Err(
                            self.err("SXP-503", format!("index {i} is outside a list of {len}"))
                        );
                    }
                }
            }
            _ => return Err(self.internal("indexed assignment")),
        }
        Ok(())
    }

    fn eval(&mut self, env: &mut Scopes, e: &Expr) -> Res<Value> {
        let saved = self.span;
        self.span = (!e.span.is_empty()).then_some(e.span);
        let out = self.eval_kind(env, e);
        self.span = saved;
        out
    }

    #[allow(clippy::many_single_char_names)] // l/i/a/b name the operands of the AST node
    fn eval_kind(&mut self, env: &mut Scopes, e: &Expr) -> Res<Value> {
        Ok(match &e.kind {
            ExprKind::Int(n) => Value::Int(*n),
            ExprKind::Float(f) => Value::Float(*f),
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Str(s) => Value::Str(s.clone()),
            ExprKind::Char(c) => Value::Char(*c),
            ExprKind::EmptyList(_) => Value::List(vec![]),
            ExprKind::Var(n) => env
                .iter()
                .rev()
                .find_map(|s| s.get(n))
                .cloned()
                .ok_or_else(|| self.internal("unbound name"))?,
            ExprKind::Unary(op, a) => {
                let v = self.eval(env, a)?;
                self.unary(*op, v)?
            }
            ExprKind::Binary(BinOp::And, a, b) => {
                if truthy(&self.eval(env, a)?) {
                    self.eval(env, b)?
                } else {
                    Value::Bool(false)
                }
            }
            ExprKind::Binary(BinOp::Or, a, b) => {
                if truthy(&self.eval(env, a)?) {
                    Value::Bool(true)
                } else {
                    self.eval(env, b)?
                }
            }
            ExprKind::Binary(op, a, b) => {
                let x = self.eval(env, a)?;
                let y = self.eval(env, b)?;
                self.binary(*op, &x, &y)?
            }
            ExprKind::List(xs) => Value::List(
                xs.iter()
                    .map(|x| self.eval(env, x))
                    .collect::<Res<Vec<_>>>()?,
            ),
            ExprKind::Index(l, i) => {
                let l = self.eval(env, l)?;
                let Value::Int(i) = self.eval(env, i)? else {
                    return Err(self.internal("index"));
                };
                self.index(l, i)?
            }
            ExprKind::Call(name, args) => {
                let vals = args
                    .iter()
                    .map(|a| self.eval(env, a))
                    .collect::<Res<Vec<_>>>()?;
                if let Some(st) = self.fns.get(name.as_str()).copied() {
                    let line = self.line;
                    let v = self.call(st, vals)?;
                    self.line = line;
                    v
                } else {
                    self.builtin(name, &vals)?
                }
            }
        })
    }
}
