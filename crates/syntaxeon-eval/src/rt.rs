//! What the tree evaluator and the VM share: values, and the operators and builtins on them.

use std::io::{BufRead, Write};

use syntaxeon_core::{BinOp, Span, UnOp};

use crate::{Res, RuntimeError};

/// 2^63 as a float: the exclusive upper bound of `Integer`.
const I64_BOUND: f64 = 9_223_372_036_854_775_808.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Char(char),
    List(Vec<Value>),
    Unit,
}

pub(crate) struct Rt<'a> {
    pub stdin: &'a mut dyn BufRead,
    pub out: &'a mut dyn Write,
    /// The statement being executed and the expression being evaluated, for error reports.
    pub line: usize,
    pub span: Option<Span>,
}

pub(crate) fn truthy(v: &Value) -> bool {
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

impl Rt<'_> {
    pub(crate) fn err(&self, code: &'static str, message: impl Into<String>) -> RuntimeError {
        RuntimeError {
            code,
            message: message.into(),
            line: self.line,
            span: self.span,
        }
    }

    pub(crate) fn internal(&self, what: &str) -> RuntimeError {
        self.err(
            "SXP-599",
            format!("internal error: {what} (the program was not type-checked)"),
        )
    }

    pub(crate) fn unary(&self, op: UnOp, v: Value) -> Res<Value> {
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

    pub(crate) fn index(&self, l: Value, i: i64) -> Res<Value> {
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

    pub(crate) fn binary(&self, op: BinOp, x: &Value, y: &Value) -> Res<Value> {
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

    pub(crate) fn write(&mut self, s: &str) -> Res<()> {
        self.out
            .write_all(s.as_bytes())
            .map_err(|e| self.err("SXP-507", format!("cannot write output: {e}")))
    }

    pub(crate) fn builtin(&mut self, name: &str, args: &[Value]) -> Res<Value> {
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
            ("has_input", []) => Ok(Value::Bool(
                self.stdin.fill_buf().is_ok_and(|buf| !buf.is_empty()),
            )),
            ("reads_as_integer", [Value::Str(s)]) => Ok(Value::Bool(s.parse::<i64>().is_ok())),
            ("to_code", [Value::Char(c)]) => Ok(Value::Int(i64::from(u32::from(*c)))),
            ("to_letter", [Value::Int(n)]) => u32::try_from(*n)
                .ok()
                .and_then(char::from_u32)
                .map(Value::Char)
                .ok_or_else(|| self.err("SXP-506", format!("{n} is not the code of a letter"))),
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
