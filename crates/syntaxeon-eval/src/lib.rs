//! Evaluator. Normative reference: `spec/SEMANTIC_CORE.md` §6–§8, ADR-013.
//!
//! The program must already have passed `syntaxeon-check`; an ill-typed program that slips
//! through reports `SXP-599` instead of panicking.
//!
//! The interpreter recurses on the Rust stack, so callers must run it on a thread whose stack
//! can hold [`MAX_FRAMES`] Syntaxeon calls (the CLI uses 256 MiB).

use std::collections::HashMap;
use std::io::{BufRead, Write};

use syntaxeon_core::{BinOp, Expr, ExprKind, Program, Span, Stanza, Stmt, StmtKind, Target, UnOp};

/// 2^63 as a float: the exclusive upper bound of `Integer`.
const I64_BOUND: f64 = 9_223_372_036_854_775_808.0;

/// Deterministic call-depth limit (`SXP-504`).
pub const MAX_FRAMES: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Char(char),
    List(Vec<Value>),
    Unit,
}

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
    stdin: &'a mut dyn BufRead,
    out: &'a mut dyn Write,
    depth: usize,
    line: usize,
    span: Option<Span>,
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
        stdin,
        out,
        depth: 0,
        line: entry.line,
        span: None,
    };
    it.call(entry, Vec::new())?;
    it.out
        .flush()
        .map_err(|e| it.err("SXP-507", format!("cannot write output: {e}")))
}

fn truthy(v: &Value) -> bool {
    matches!(v, Value::Bool(true))
}

fn to_text(v: &Value) -> Option<String> {
    Some(match v {
        Value::Int(n) => n.to_string(),
        // Shortest round-trip form, always showing the fraction: 1.0, 0.1, 1e21, NaN, inf.
        Value::Float(f) => format!("{f:?}"),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => c.to_string(),
        _ => return None,
    })
}

impl Interp<'_> {
    fn err(&self, code: &'static str, message: impl Into<String>) -> RuntimeError {
        RuntimeError {
            code,
            message: message.into(),
            line: self.line,
            span: self.span,
        }
    }

    fn internal(&self, what: &str) -> RuntimeError {
        self.err(
            "SXP-599",
            format!("internal error: {what} (the program was not type-checked)"),
        )
    }

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
        self.span = None;
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
            StmtKind::While { cond, body } => {
                while truthy(&self.eval(env, cond)?) {
                    self.line = s.line;
                    self.span = None;
                    if let Flow::Return(v) = self.block(env, body)? {
                        return Ok(Flow::Return(v));
                    }
                }
            }
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

    fn unary(&self, op: UnOp, v: Value) -> Res<Value> {
        match (op, v) {
            (UnOp::Neg, Value::Int(n)) => n
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| self.err("SXP-501", "integer overflow in negation")),
            (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
            (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
            _ => Err(self.internal("unary operand")),
        }
    }

    fn index(&self, l: Value, i: i64) -> Res<Value> {
        let oob = |len: usize| {
            self.err(
                "SXP-503",
                format!("index {i} is outside a sequence of {len}"),
            )
        };
        match l {
            Value::List(items) => usize::try_from(i)
                .ok()
                .and_then(|i| items.get(i).cloned())
                .ok_or_else(|| oob(items.len())),
            Value::Str(s) => usize::try_from(i)
                .ok()
                .and_then(|i| s.chars().nth(i))
                .map(Value::Char)
                .ok_or_else(|| oob(s.chars().count())),
            _ => Err(self.internal("indexed value")),
        }
    }

    fn binary(&self, op: BinOp, x: &Value, y: &Value) -> Res<Value> {
        use Value::{Bool, Char, Float, Int, Str};
        let overflow = || self.err("SXP-501", format!("integer overflow in {op:?}"));
        let zero = || self.err("SXP-502", "division by zero");
        Ok(match (op, x, y) {
            (BinOp::Eq, ..) => Bool(x == y),
            (BinOp::Ne, ..) => Bool(x != y),
            (BinOp::Add, Int(a), Int(b)) => Int(a.checked_add(*b).ok_or_else(overflow)?),
            (BinOp::Sub, Int(a), Int(b)) => Int(a.checked_sub(*b).ok_or_else(overflow)?),
            (BinOp::Mul, Int(a), Int(b)) => Int(a.checked_mul(*b).ok_or_else(overflow)?),
            (BinOp::Div | BinOp::Mod, Int(_), Int(0)) => return Err(zero()),
            (BinOp::Div, Int(a), Int(b)) => Int(a.checked_div(*b).ok_or_else(overflow)?),
            (BinOp::Mod, Int(a), Int(b)) => Int(a.wrapping_rem(*b)),
            (BinOp::Add, Float(a), Float(b)) => Float(a + b),
            (BinOp::Sub, Float(a), Float(b)) => Float(a - b),
            (BinOp::Mul, Float(a), Float(b)) => Float(a * b),
            (BinOp::Div, Float(a), Float(b)) => Float(a / b),
            (BinOp::Add, Str(a), Str(b)) => Str(format!("{a}{b}")),
            (BinOp::Lt, Int(a), Int(b)) => Bool(a < b),
            (BinOp::Le, Int(a), Int(b)) => Bool(a <= b),
            (BinOp::Gt, Int(a), Int(b)) => Bool(a > b),
            (BinOp::Ge, Int(a), Int(b)) => Bool(a >= b),
            (BinOp::Lt, Float(a), Float(b)) => Bool(a < b),
            (BinOp::Le, Float(a), Float(b)) => Bool(a <= b),
            (BinOp::Gt, Float(a), Float(b)) => Bool(a > b),
            (BinOp::Ge, Float(a), Float(b)) => Bool(a >= b),
            (BinOp::Lt, Char(a), Char(b)) => Bool(a < b),
            (BinOp::Le, Char(a), Char(b)) => Bool(a <= b),
            (BinOp::Gt, Char(a), Char(b)) => Bool(a > b),
            (BinOp::Ge, Char(a), Char(b)) => Bool(a >= b),
            (BinOp::Lt, Str(a), Str(b)) => Bool(a < b),
            (BinOp::Le, Str(a), Str(b)) => Bool(a <= b),
            (BinOp::Gt, Str(a), Str(b)) => Bool(a > b),
            (BinOp::Ge, Str(a), Str(b)) => Bool(a >= b),
            _ => return Err(self.internal("binary operands")),
        })
    }

    fn write(&mut self, s: &str) -> Res<()> {
        self.out
            .write_all(s.as_bytes())
            .map_err(|e| self.err("SXP-507", format!("cannot write output: {e}")))
    }

    fn builtin(&mut self, name: &str, args: &[Value]) -> Res<Value> {
        match (name, args) {
            ("speak", [Value::Str(s)]) => self.write(s).map(|()| Value::Unit),
            ("speakln", [Value::Str(s)]) => {
                self.write(s)?;
                self.write("\n").map(|()| Value::Unit)
            }
            ("hear", []) => {
                let mut line = String::new();
                match self.stdin.read_line(&mut line) {
                    Ok(0) => Err(self.err("SXP-505", "no more input to hear")),
                    Ok(_) => {
                        let trimmed = line.strip_suffix('\n').unwrap_or(&line);
                        Ok(Value::Str(
                            trimmed.strip_suffix('\r').unwrap_or(trimmed).to_string(),
                        ))
                    }
                    Err(e) => Err(self.err("SXP-505", format!("cannot read input: {e}"))),
                }
            }
            ("length", [Value::List(v)]) => {
                Ok(Value::Int(i64::try_from(v.len()).unwrap_or(i64::MAX)))
            }
            ("length", [Value::Str(s)]) => Ok(Value::Int(
                i64::try_from(s.chars().count()).unwrap_or(i64::MAX),
            )),
            ("append", [Value::List(v), x]) => {
                let mut v = v.clone();
                v.push(x.clone());
                Ok(Value::List(v))
            }
            ("to_string", [v]) => to_text(v)
                .map(Value::Str)
                .ok_or_else(|| self.internal("to_string")),
            ("to_integer", [Value::Float(f)]) => {
                let t = f.trunc();
                if t.is_finite() && (-I64_BOUND..I64_BOUND).contains(&t) {
                    #[allow(clippy::cast_possible_truncation)] // range checked above
                    Ok(Value::Int(t as i64))
                } else {
                    Err(self.err(
                        "SXP-506",
                        format!("{f:?} cannot be converted to a whole number"),
                    ))
                }
            }
            ("to_integer", [Value::Str(s)]) => s
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| self.err("SXP-506", format!("{s:?} is not a whole number"))),
            #[allow(clippy::cast_precision_loss)] // the language's Integer->Float conversion
            ("to_float", [Value::Int(n)]) => Ok(Value::Float(*n as f64)),
            _ => Err(self.internal(&format!("builtin {name}"))),
        }
    }
}
