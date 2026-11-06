//! The VM: runs the IR on an explicit stack of frames, so it needs no deep native stack
//! (ADR-021). It is observably the same as the tree evaluator: same output, same errors, same
//! lines and spans.

use std::io::{BufRead, Write};

use syntaxeon_core::{BinOp, Program};

use crate::ir::{Function, Ir, Op, lower};
use crate::rt::{Rt, Value, truthy};
use crate::{MAX_FRAMES, Res, RuntimeError};

struct Frame {
    func: usize,
    pc: usize,
    /// Where this frame's slots start in `slots`.
    base: usize,
}

/// Lowers and runs the entry stanza. Program output goes to `out`.
///
/// # Errors
/// The runtime failure that stopped the program.
pub fn run(
    program: &Program,
    stdin: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<(), RuntimeError> {
    if program.stanzas.is_empty() {
        return Err(RuntimeError {
            code: "SXP-599",
            message: "no entry stanza".into(),
            line: 0,
            span: None,
        });
    }
    let ir = lower(program)?;
    execute(&ir, stdin, out)
}

#[allow(clippy::too_many_lines)] // one match over the operations; splitting would hide the loop
fn execute(ir: &Ir, stdin: &mut dyn BufRead, out: &mut dyn Write) -> Result<(), RuntimeError> {
    let mut rt = Rt {
        stdin,
        out,
        line: ir.functions[0].locs.first().map_or(0, |l| l.line),
        span: None,
    };
    let mut stack: Vec<Value> = Vec::new();
    let mut slots: Vec<Value> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    enter(&mut slots, &mut frames, &ir.functions, 0, &mut Vec::new());
    while let Some(frame) = frames.last_mut() {
        let func = &ir.functions[frame.func];
        let at = frame.pc;
        frame.pc += 1;
        let base = frame.base;
        let loc = func.locs[at];
        rt.line = loc.line;
        rt.span = loc.span;
        match &func.code[at] {
            Op::Int(n) => stack.push(Value::Int(*n)),
            Op::Float(x) => stack.push(Value::Float(*x)),
            Op::Bool(b) => stack.push(Value::Bool(*b)),
            Op::Str(s) => stack.push(Value::Str(s.clone())),
            Op::Char(c) => stack.push(Value::Char(*c)),
            Op::Nil => stack.push(Value::List(Vec::new())),
            Op::Load(s) => stack.push(slots[base + s].clone()),
            Op::Store(s) => slots[base + s] = pop(&rt, &mut stack)?,
            Op::StoreAt(s) => {
                let Value::Int(i) = pop(&rt, &mut stack)? else {
                    return Err(rt.internal("index"));
                };
                let v = pop(&rt, &mut stack)?;
                let Value::List(items) = &mut slots[base + s] else {
                    return Err(rt.internal("indexed assignment"));
                };
                let len = items.len();
                match usize::try_from(i).ok().and_then(|i| items.get_mut(i)) {
                    Some(cell) => *cell = v,
                    None => {
                        return Err(
                            rt.err("SXP-503", format!("index {i} is outside a list of {len}"))
                        );
                    }
                }
            }
            Op::Pop => {
                pop(&rt, &mut stack)?;
            }
            Op::Neg | Op::Not => {
                let v = pop(&rt, &mut stack)?;
                let op = if matches!(func.code[at], Op::Neg) {
                    syntaxeon_core::UnOp::Neg
                } else {
                    syntaxeon_core::UnOp::Not
                };
                stack.push(rt.unary(op, v)?);
            }
            Op::Bin(op) => {
                let y = pop(&rt, &mut stack)?;
                let x = pop(&rt, &mut stack)?;
                // `and` / `or` are jumps; only the strict operators reach here.
                debug_assert!(!matches!(op, BinOp::And | BinOp::Or));
                stack.push(rt.binary(*op, &x, &y)?);
            }
            Op::List(n) => {
                let items = stack.split_off(stack.len().saturating_sub(*n));
                stack.push(Value::List(items));
            }
            Op::Index => {
                let Value::Int(i) = pop(&rt, &mut stack)? else {
                    return Err(rt.internal("index"));
                };
                let l = pop(&rt, &mut stack)?;
                stack.push(rt.index(l, i)?);
            }
            Op::Jump(t) => frame.pc = *t,
            Op::JumpIfNot(t) => {
                if !truthy(&pop(&rt, &mut stack)?) {
                    frame.pc = *t;
                }
            }
            Op::Call(f, n) => {
                if frames.len() + 1 > MAX_FRAMES {
                    return Err(
                        rt.err("SXP-504", format!("call depth exceeds {MAX_FRAMES} frames"))
                    );
                }
                let mut args = stack.split_off(stack.len().saturating_sub(*n));
                enter(&mut slots, &mut frames, &ir.functions, *f, &mut args);
            }
            Op::Builtin(name, n) => {
                let args = stack.split_off(stack.len().saturating_sub(*n));
                stack.push(rt.builtin(name, &args)?);
            }
            Op::Return | Op::ReturnUnit => {
                let v = if matches!(func.code[at], Op::Return) {
                    pop(&rt, &mut stack)?
                } else {
                    Value::Unit
                };
                slots.truncate(base);
                frames.pop();
                if frames.is_empty() {
                    break;
                }
                stack.push(v);
            }
        }
    }
    rt.out
        .flush()
        .map_err(|e| rt.err("SXP-507", format!("cannot write output: {e}")))
}

fn pop(rt: &Rt, stack: &mut Vec<Value>) -> Res<Value> {
    stack.pop().ok_or_else(|| rt.internal("empty stack"))
}

/// Pushes a frame for `func`: its arguments, then room for the rest of its slots.
fn enter(
    slots: &mut Vec<Value>,
    frames: &mut Vec<Frame>,
    functions: &[Function],
    func: usize,
    args: &mut Vec<Value>,
) {
    let base = slots.len();
    slots.append(args);
    slots.resize(base + functions[func].slots, Value::Unit);
    frames.push(Frame { func, pc: 0, base });
}
