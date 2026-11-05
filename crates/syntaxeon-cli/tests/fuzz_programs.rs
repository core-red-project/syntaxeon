#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
#![allow(clippy::too_many_lines)] // the generator and the reference model are long match tables
#![allow(clippy::many_single_char_names)] // a/b/e/c name the operands of the model's nodes
//! Whole-program fuzzing. A generator writes random, well-typed integer programs (functions,
//! nested `If`/`Otherwise`, `Repeat`, bounded `As long as`, shadowing, early returns, overflow
//! and division by zero) together with a tiny reference interpreter written independently of
//! the real one. The program goes through lexer → parser → checker → evaluator and its output
//! and error code must equal the model's. Mutated programs must be rejected, never crash.
//!
//! Rhyme is not generated, so prosody is not part of this pipeline; it is fuzzed separately
//! with the lexicon. Deterministic: a fixed xorshift seed, no dependencies.
//! `SYNTAXEON_FUZZ_ITERS` and `SYNTAXEON_FUZZ_SEED` scale and vary a run.

use std::collections::HashMap;
use std::fmt::Write as _;

use syntaxeon_check::check;
use syntaxeon_eval::run;
use syntaxeon_lex::lexer::lex;
use syntaxeon_lex::literary::stanzas;
use syntaxeon_syntax::parse_program;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const VARS: &[&str] = &[
    "counter", "pile", "chimes", "slumber", "hero", "bird", "tally", "stack", "glimmer", "ember",
];
const FUNS: &[&str] = &["twice", "blend", "shift", "mixer"];
const PARAMS: &[&str] = &["left", "right"];

// ---- the model ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum E {
    Lit(i64),
    Var(String),
    Neg(Box<E>),
    Bin(Op, Box<E>, Box<E>),
    Call(usize, Vec<E>),
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Sum,
    Difference,
    Product,
    Quotient,
    Remainder,
}

#[derive(Clone, Copy, Debug)]
enum Rel {
    Less,
    Greater,
    AtMost,
    AtLeast,
    Is,
    IsNot,
}

#[derive(Clone, Debug)]
enum C {
    Cmp(Rel, E, E),
    Both(Box<C>, Box<C>),
    Either(Box<C>, Box<C>),
}

#[derive(Clone, Debug)]
enum S {
    Let(String, E),
    Set(String, E),
    Print(E),
    If(C, Vec<S>, Option<Vec<S>>),
    Repeat(i64, Vec<S>),
    /// `counter` starts at `start`, the loop runs while it is above zero and decrements first.
    While(String, i64, Vec<S>),
    Return(E),
}

struct Fun {
    name: usize,
    params: Vec<String>,
    body: Vec<S>,
}

struct Prog {
    entry: Vec<S>,
    funs: Vec<Fun>,
}

// ---- generator ---------------------------------------------------------------------------

/// Names visible at a point, innermost block last, and names that must not be assigned.
struct Scope {
    blocks: Vec<Vec<String>>,
    locked: Vec<String>,
}

impl Scope {
    fn visible(&self) -> Vec<String> {
        let mut v: Vec<String> = self.blocks.iter().flatten().cloned().collect();
        v.sort();
        v.dedup();
        v
    }
}

struct Gen {
    rng: Rng,
    /// Arity of each function; function `k` may only call functions below `k`.
    arity: Vec<usize>,
}

/// Whether the rendered text ends in a call with arguments, which would swallow a following
/// comma or comparison word (`spec/PROSE_GRAMMAR.md` §7.4).
fn ends_open(e: &E) -> bool {
    match e {
        E::Call(_, args) => !args.is_empty(),
        E::Neg(a) => ends_open(a),
        E::Bin(_, _, b) => ends_open(b),
        E::Lit(_) | E::Var(_) => false,
    }
}

impl Gen {
    fn literal(&mut self) -> i64 {
        let r = self.rng.below(100);
        let n = match r {
            0..=64 => self.rng.below(10),
            65..=84 => self.rng.below(1000),
            _ => self.rng.below(4_000_000_000),
        };
        i64::try_from(n).unwrap()
    }

    fn expr(&mut self, vars: &[String], limit: usize, depth: usize) -> E {
        let kind = self.rng.below(if depth == 0 { 3 } else { 10 });
        match kind {
            1 | 2 if !vars.is_empty() => E::Var(vars[self.rng.below(vars.len())].clone()),
            3 => E::Neg(Box::new(self.expr(vars, limit, depth - 1))),
            4..=8 => {
                let op = [
                    Op::Sum,
                    Op::Difference,
                    Op::Product,
                    Op::Quotient,
                    Op::Remainder,
                ][self.rng.below(5)];
                let a = self.expr(vars, limit, depth - 1);
                let b = if matches!(op, Op::Quotient | Op::Remainder) && self.rng.chance(85) {
                    self.divisor(vars, limit, depth - 1)
                } else {
                    self.expr(vars, limit, depth - 1)
                };
                E::Bin(op, Box::new(a), Box::new(b))
            }
            9 if limit > 0 => {
                let f = self.rng.below(limit);
                let n = self.arity[f];
                let mut args: Vec<E> = (0..n).map(|_| self.expr(vars, limit, depth - 1)).collect();
                // Only the last argument may end in a call; earlier ones become literals.
                for a in args.iter_mut().take(n.saturating_sub(1)) {
                    if ends_open(a) {
                        *a = E::Lit(self.literal());
                    }
                }
                E::Call(f, args)
            }
            _ => E::Lit(self.literal()),
        }
    }

    /// A divisor that is never zero (a small literal, or `x * x + 1`), so division by zero stays
    /// a deliberate minority and the programs keep running.
    fn divisor(&mut self, vars: &[String], limit: usize, depth: usize) -> E {
        if self.rng.chance(50) {
            return E::Lit(i64::try_from(1 + self.rng.below(50)).unwrap());
        }
        let x = self.expr(vars, limit, depth);
        let square = E::Bin(Op::Product, Box::new(x.clone()), Box::new(x));
        E::Bin(Op::Sum, Box::new(square), Box::new(E::Lit(1)))
    }

    /// A left operand of a comparison must not end in a call.
    fn closed(&mut self, vars: &[String], limit: usize, depth: usize) -> E {
        let e = self.expr(vars, limit, depth);
        if ends_open(&e) {
            E::Lit(self.literal())
        } else {
            e
        }
    }

    fn cond(&mut self, vars: &[String], limit: usize, depth: usize) -> C {
        match (depth > 0, self.rng.below(5)) {
            (true, 0) => C::Both(
                Box::new(self.cond(vars, limit, depth - 1)),
                Box::new(self.cond(vars, limit, depth - 1)),
            ),
            (true, 1) => C::Either(
                Box::new(self.cond(vars, limit, depth - 1)),
                Box::new(self.cond(vars, limit, depth - 1)),
            ),
            _ => {
                let rel = [
                    Rel::Less,
                    Rel::Greater,
                    Rel::AtMost,
                    Rel::AtLeast,
                    Rel::Is,
                    Rel::IsNot,
                ][self.rng.below(6)];
                C::Cmp(rel, self.closed(vars, limit, 2), self.expr(vars, limit, 2))
            }
        }
    }

    fn fresh(&mut self, sc: &Scope) -> Option<String> {
        let here = sc.blocks.last().cloned().unwrap_or_default();
        let free: Vec<&&str> = VARS
            .iter()
            .filter(|v| !here.iter().any(|h| h == **v) && !sc.locked.iter().any(|l| l == **v))
            .collect();
        (!free.is_empty()).then(|| (*free[self.rng.below(free.len())]).to_string())
    }

    fn block(&mut self, sc: &mut Scope, limit: usize, depth: usize, in_fun: bool) -> Vec<S> {
        sc.blocks.push(Vec::new());
        let n = 1 + self.rng.below(5);
        let mut out = Vec::new();
        for _ in 0..n {
            if let Some(s) = self.stmt(sc, limit, depth, in_fun) {
                out.push(s);
            }
        }
        sc.blocks.pop();
        out
    }

    fn stmt(&mut self, sc: &mut Scope, limit: usize, depth: usize, in_fun: bool) -> Option<S> {
        let vars = sc.visible();
        let settable: Vec<String> = vars
            .iter()
            .filter(|v| !sc.locked.contains(v))
            .cloned()
            .collect();
        let kind = self.rng.below(if depth == 0 { 4 } else { 9 });
        Some(match kind {
            0 | 1 => {
                let name = self.fresh(sc)?;
                let e = self.expr(&vars, limit, 3);
                sc.blocks.last_mut().unwrap().push(name.clone());
                S::Let(name, e)
            }
            2 if !settable.is_empty() => {
                let t = settable[self.rng.below(settable.len())].clone();
                S::Set(t, self.expr(&vars, limit, 3))
            }
            2 | 3 => S::Print(self.expr(&vars, limit, 3)),
            4 | 5 => {
                let c = self.cond(&vars, limit, 1);
                let then = self.block(sc, limit, depth - 1, in_fun);
                let other = self
                    .rng
                    .chance(60)
                    .then(|| self.block(sc, limit, depth - 1, in_fun));
                S::If(c, then, other)
            }
            6 => {
                let n = i64::try_from(self.rng.below(5)).unwrap();
                S::Repeat(n, self.block(sc, limit, depth - 1, in_fun))
            }
            7 => {
                let counter = self.fresh(sc)?;
                let start = i64::try_from(self.rng.below(5)).unwrap();
                sc.blocks.last_mut().unwrap().push(counter.clone());
                sc.locked.push(counter.clone());
                let body = self.block(sc, limit, depth - 1, in_fun);
                sc.locked.pop();
                S::While(counter, start, body)
            }
            _ if in_fun => S::Return(self.expr(&vars, limit, 3)),
            _ => S::Print(self.expr(&vars, limit, 3)),
        })
    }

    fn program(&mut self) -> Prog {
        let nfun = self.rng.below(4);
        self.arity = (0..nfun).map(|_| 1 + self.rng.below(2)).collect();
        let mut funs = Vec::new();
        for k in 0..nfun {
            let params: Vec<String> = PARAMS
                .iter()
                .take(self.arity[k])
                .map(|p| (*p).to_string())
                .collect();
            let mut sc = Scope {
                blocks: vec![params.clone()],
                // Parameters are immutable.
                locked: params.clone(),
            };
            let mut body = self.block(&mut sc, k, 2, true);
            // The final return sees only the parameters; the block above scoped its locals.
            body.push(S::Return(self.expr(&params, k, 3)));
            funs.push(Fun {
                name: k,
                params,
                body,
            });
        }
        let mut sc = Scope {
            blocks: vec![],
            locked: vec![],
        };
        let entry = self.block(&mut sc, nfun, 3, false);
        Prog { entry, funs }
    }
}

// ---- rendering ---------------------------------------------------------------------------

fn render_e(e: &E, out: &mut String) {
    match e {
        E::Lit(n) => out.push_str(&n.to_string()),
        E::Var(v) => {
            out.push_str("the ");
            out.push_str(v);
        }
        E::Neg(a) => {
            out.push_str("the negation of ");
            render_e(a, out);
        }
        E::Bin(op, a, b) => {
            let w = match op {
                Op::Sum => "sum",
                Op::Difference => "difference",
                Op::Product => "product",
                Op::Quotient => "quotient",
                Op::Remainder => "remainder",
            };
            write!(out, "the {w} of ").unwrap();
            render_e(a, out);
            out.push_str(" and ");
            render_e(b, out);
        }
        E::Call(f, args) => {
            out.push_str("calling ");
            out.push_str(FUNS[*f]);
            for (i, a) in args.iter().enumerate() {
                out.push_str(if i == 0 { " with " } else { ", " });
                render_e(a, out);
            }
        }
    }
}

fn render_c(c: &C, out: &mut String) {
    match c {
        C::Cmp(rel, a, b) => {
            render_e(a, out);
            out.push_str(match rel {
                Rel::Less => " is less than ",
                Rel::Greater => " is greater than ",
                Rel::AtMost => " is at most ",
                Rel::AtLeast => " is at least ",
                Rel::Is => " is ",
                Rel::IsNot => " is not ",
            });
            render_e(b, out);
        }
        C::Both(a, b) => {
            out.push_str("both ");
            render_c(a, out);
            out.push_str(" and ");
            render_c(b, out);
        }
        C::Either(a, b) => {
            out.push_str("either ");
            render_c(a, out);
            out.push_str(" or ");
            render_c(b, out);
        }
    }
}

fn render_block(b: &[S], out: &mut String) {
    for s in b {
        render_s(s, out);
    }
}

fn render_s(s: &S, out: &mut String) {
    match s {
        S::Let(n, e) => {
            write!(out, "Let the {n}, a whole number, become ").unwrap();
            render_e(e, out);
            out.push_str(".\n");
        }
        S::Set(n, e) => {
            write!(out, "Let the {n} now be ").unwrap();
            render_e(e, out);
            out.push_str(".\n");
        }
        S::Print(e) => {
            out.push_str("Proclaim the text of ");
            render_e(e, out);
            out.push_str(".\n");
        }
        S::Return(e) => {
            out.push_str("Give back ");
            render_e(e, out);
            out.push_str(".\n");
        }
        S::If(c, then, other) => {
            out.push_str("If ");
            render_c(c, out);
            out.push_str(", then:\n");
            render_block(then, out);
            if let Some(o) = other {
                out.push_str("Otherwise:\n");
                render_block(o, out);
            }
            out.push_str("So be it.\n");
        }
        S::Repeat(n, body) => {
            writeln!(out, "Do this {n} times:").unwrap();
            render_block(body, out);
            out.push_str("So be it.\n");
        }
        S::While(c, start, body) => {
            writeln!(out, "Let the {c}, a whole number, become {start}.").unwrap();
            writeln!(out, "As long as the {c} is greater than 0, do:").unwrap();
            writeln!(out, "Let the {c} now be the difference of the {c} and 1.").unwrap();
            render_block(body, out);
            out.push_str("So be it.\n");
        }
    }
}

fn render(p: &Prog) -> String {
    let mut out = String::from("Thus begins the tale:\n");
    render_block(&p.entry, &mut out);
    for f in &p.funs {
        write!(out, "\nTo {}, taking ", FUNS[f.name]).unwrap();
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{p} as whole number"))
            .collect();
        match params.as_slice() {
            [only] => out.push_str(only),
            [init @ .., last] => write!(out, "{}, and {last}", init.join(", ")).unwrap(),
            [] => {}
        }
        out.push_str(", giving whole number:\n");
        render_block(&f.body, &mut out);
    }
    out
}

// ---- the reference interpreter -----------------------------------------------------------

type Frame = Vec<HashMap<String, i64>>;

struct Model<'a> {
    funs: &'a [Fun],
    out: String,
}

enum Flow {
    Next,
    Ret(i64),
}

fn lookup(frame: &Frame, name: &str) -> i64 {
    frame
        .iter()
        .rev()
        .find_map(|b| b.get(name))
        .copied()
        .expect("generator keeps names in scope")
}

fn assign(frame: &mut Frame, name: &str, v: i64) {
    *frame
        .iter_mut()
        .rev()
        .find_map(|b| b.get_mut(name))
        .expect("name in scope") = v;
}

impl Model<'_> {
    fn eval(&mut self, e: &E, frame: &Frame) -> Result<i64, &'static str> {
        match e {
            E::Lit(n) => Ok(*n),
            E::Var(v) => Ok(lookup(frame, v)),
            E::Neg(a) => self.eval(a, frame)?.checked_neg().ok_or("SXP-501"),
            E::Bin(op, a, b) => {
                let (x, y) = (self.eval(a, frame)?, self.eval(b, frame)?);
                match op {
                    Op::Sum => x.checked_add(y).ok_or("SXP-501"),
                    Op::Difference => x.checked_sub(y).ok_or("SXP-501"),
                    Op::Product => x.checked_mul(y).ok_or("SXP-501"),
                    Op::Quotient | Op::Remainder if y == 0 => Err("SXP-502"),
                    Op::Quotient => x.checked_div(y).ok_or("SXP-501"),
                    Op::Remainder => Ok(x.wrapping_rem(y)),
                }
            }
            E::Call(f, args) => {
                let vals = args
                    .iter()
                    .map(|a| self.eval(a, frame))
                    .collect::<Result<Vec<_>, _>>()?;
                let fun = &self.funs[*f];
                let mut callee: Frame = vec![
                    fun.params
                        .iter()
                        .cloned()
                        .zip(vals)
                        .collect::<HashMap<_, _>>(),
                ];
                match self.block_in_place(&fun.body, &mut callee)? {
                    Flow::Ret(v) => Ok(v),
                    Flow::Next => unreachable!("every function ends in a return"),
                }
            }
        }
    }

    fn cond(&mut self, c: &C, frame: &Frame) -> Result<bool, &'static str> {
        match c {
            C::Cmp(rel, a, b) => {
                let (x, y) = (self.eval(a, frame)?, self.eval(b, frame)?);
                Ok(match rel {
                    Rel::Less => x < y,
                    Rel::Greater => x > y,
                    Rel::AtMost => x <= y,
                    Rel::AtLeast => x >= y,
                    Rel::Is => x == y,
                    Rel::IsNot => x != y,
                })
            }
            C::Both(a, b) => Ok(self.cond(a, frame)? && self.cond(b, frame)?),
            C::Either(a, b) => Ok(self.cond(a, frame)? || self.cond(b, frame)?),
        }
    }

    /// Runs statements in the frame's current innermost block.
    fn block_in_place(&mut self, b: &[S], frame: &mut Frame) -> Result<Flow, &'static str> {
        for s in b {
            if let Flow::Ret(v) = self.stmt(s, frame)? {
                return Ok(Flow::Ret(v));
            }
        }
        Ok(Flow::Next)
    }

    fn scoped(&mut self, b: &[S], frame: &mut Frame) -> Result<Flow, &'static str> {
        frame.push(HashMap::new());
        let r = self.block_in_place(b, frame);
        frame.pop();
        r
    }

    fn stmt(&mut self, s: &S, frame: &mut Frame) -> Result<Flow, &'static str> {
        match s {
            S::Let(n, e) => {
                let v = self.eval(e, frame)?;
                frame.last_mut().unwrap().insert(n.clone(), v);
            }
            S::Set(n, e) => {
                let v = self.eval(e, frame)?;
                assign(frame, n, v);
            }
            S::Print(e) => {
                let v = self.eval(e, frame)?;
                writeln!(self.out, "{v}").unwrap();
            }
            S::Return(e) => return Ok(Flow::Ret(self.eval(e, frame)?)),
            S::If(c, then, other) => {
                if self.cond(c, frame)? {
                    return self.scoped(then, frame);
                } else if let Some(o) = other {
                    return self.scoped(o, frame);
                }
            }
            S::Repeat(n, body) => {
                for _ in 0..*n {
                    if let Flow::Ret(v) = self.scoped(body, frame)? {
                        return Ok(Flow::Ret(v));
                    }
                }
            }
            S::While(c, start, body) => {
                frame.last_mut().unwrap().insert(c.clone(), *start);
                while lookup(frame, c) > 0 {
                    let left = lookup(frame, c) - 1;
                    assign(frame, c, left);
                    if let Flow::Ret(v) = self.scoped(body, frame)? {
                        return Ok(Flow::Ret(v));
                    }
                }
            }
        }
        Ok(Flow::Next)
    }
}

fn model_run(p: &Prog) -> Outcome {
    let mut m = Model {
        funs: &p.funs,
        out: String::new(),
    };
    let mut frame: Frame = vec![HashMap::new()];
    let r = m.block_in_place(&p.entry, &mut frame);
    (m.out, r.err())
}

// ---- the real pipeline -------------------------------------------------------------------

/// Front-end diagnostics, or the program's output and runtime error code.
/// What a program printed and the runtime error code it stopped with, if any.
type Outcome = (String, Option<&'static str>);

/// Diagnostic code and line.
type Diag = (&'static str, usize);

fn pipeline(src: &str) -> Result<Outcome, Vec<Diag>> {
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    let (program, parse) = parse_program(&lexed, &verses);
    let mut codes: Vec<(&'static str, usize)> = lexed
        .diagnostics
        .iter()
        .chain(&parse)
        .map(|d| (d.code, d.line))
        .collect();
    if codes.is_empty() {
        codes = check(&program).iter().map(|d| (d.code, d.line)).collect();
    }
    if !codes.is_empty() {
        return Err(codes);
    }
    let mut out = Vec::new();
    let r = std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn_scoped(s, || run(&program, &mut &b""[..], &mut out))
            .unwrap()
            .join()
            .unwrap()
    });
    Ok((String::from_utf8(out).unwrap(), r.err().map(|e| e.code)))
}

fn program_for(seed: u64) -> Prog {
    Gen {
        rng: Rng(seed | 1),
        arity: vec![],
    }
    .program()
}

fn mix(seed: u64, i: u64) -> u64 {
    seed.wrapping_add(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ 0xD1B5_4A32_D192_ED03
}

#[test]
fn generated_programs_behave_like_the_model() {
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 400);
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x5EED);
    let mut with_error = 0;
    let mut printed = 0;
    for i in 0..iters {
        let seed = mix(base, i);
        let prog = program_for(seed);
        let src = render(&prog);
        let want = model_run(&prog);
        let got = pipeline(&src).unwrap_or_else(|codes| {
            panic!("seed {seed:#x} rejected {codes:?}\n--- program\n{src}");
        });
        assert_eq!(got, want, "seed {seed:#x}\n--- program\n{src}");
        with_error += usize::from(want.1.is_some());
        printed += usize::from(!want.0.is_empty());
    }
    // The generator must actually exercise output and runtime errors.
    let n = usize::try_from(iters).unwrap();
    assert!(
        printed * 4 >= n,
        "too few programs print anything: {printed}/{n}"
    );
    assert!(with_error >= 1 || n < 50, "no program hit a runtime error");
}

#[test]
#[ignore = "exploratory: prints how much of the language the generator exercises"]
fn explore_coverage() {
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 5_000);
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x5EED);
    let (mut lines, mut chars) = (0, 0);
    let mut errors: HashMap<&str, usize> = HashMap::new();
    let (mut calls, mut whiles, mut repeats, mut elses, mut returns, mut shadows) =
        (0, 0, 0, 0, 0, 0);
    for i in 0..iters {
        let prog = program_for(mix(base, i));
        let src = render(&prog);
        lines += src.lines().count();
        chars += src.len();
        calls += src.matches("calling ").count();
        whiles += src.matches("As long as").count();
        repeats += src.matches("Do this").count();
        elses += src.matches("Otherwise:").count();
        returns += src.matches("Give back").count();
        // A name declared twice in the same text is shadowing or redeclaration across scopes.
        shadows += usize::from(src.matches("a whole number, become").count() > 12);
        if let (_, Some(code)) = model_run(&prog) {
            *errors.entry(code).or_default() += 1;
        }
    }
    println!(
        "programs {iters} | avg {} lines, {} chars\ncalls {calls} whiles {whiles} repeats {repeats} elses {elses} returns {returns} big-decl programs {shadows}\nruntime errors {errors:?}",
        lines / usize::try_from(iters).unwrap(),
        chars / usize::try_from(iters).unwrap()
    );
}

/// Lex, parse and check only; mutated programs may loop forever if run.
fn front_end(src: &str) -> Vec<(&'static str, usize)> {
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    let (program, parse) = parse_program(&lexed, &verses);
    let mut codes: Vec<(&'static str, usize)> = lexed
        .diagnostics
        .iter()
        .chain(&parse)
        .map(|d| (d.code, d.line))
        .collect();
    if codes.is_empty() {
        codes = check(&program).iter().map(|d| (d.code, d.line)).collect();
    }
    codes
}

/// Removing a closer leaves a block open and adding one closes a block that is not open: both
/// must be reported as block-structure errors, never absorbed.
#[test]
fn unbalanced_blocks_are_always_reported() {
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 400);
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x5EED);
    let mut rng = Rng(base | 1);
    let mut tested = 0;
    for i in 0..iters {
        let src = render(&program_for(mix(base, i)));
        let mut lines: Vec<&str> = src.lines().collect();
        let closers: Vec<usize> = (0..lines.len())
            .filter(|&k| lines[k] == "So be it.")
            .collect();
        if closers.is_empty() {
            continue;
        }
        tested += 1;
        let removed = closers[rng.below(closers.len())];
        let without = {
            let mut l = lines.clone();
            l.remove(removed);
            l.join("\n")
        };
        let codes = front_end(&without);
        assert!(
            codes.iter().any(|(c, _)| *c == "SXP-107"),
            "removing line {removed} was not reported: {codes:?}\n{without}"
        );
        let at = 1 + rng.below(lines.len() - 1);
        lines.insert(at, "So be it.");
        let with = lines.join("\n");
        let codes = front_end(&with);
        // A closer dropped in front of a stanza header is a malformed header (`SXP-109`).
        assert!(
            codes
                .iter()
                .any(|(c, _)| matches!(*c, "SXP-107" | "SXP-109")),
            "adding a closer at {at} was not reported: {codes:?}\n{with}"
        );
    }
    assert!(
        tested * 2 >= usize::try_from(iters).unwrap(),
        "too few programs had blocks"
    );
}

const WORDS: &[&str] = &[
    "the",
    "and",
    "of",
    "is",
    "now",
    "be",
    "become",
    "shall",
    "calling",
    "with",
    "item",
    "not",
    "Otherwise:",
    "So be it.",
    "then:",
    ",",
    ".",
    ":",
    "(",
    ")",
    "\"",
    "0",
    "99999999999999999999",
    "a list of",
    "an empty list of",
    "what is heard",
    "Thus begins",
    "To",
    "taking",
    "giving",
];

fn mutate(src: &str, rng: &mut Rng) -> String {
    if src.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = src.lines().map(String::from).collect();
    let k = rng.below(lines.len());
    match rng.below(8) {
        0 => {
            let cut = src
                .char_indices()
                .map(|(i, _)| i)
                .nth(rng.below(src.chars().count()));
            return src[..cut.unwrap_or(0)].to_string();
        }
        1 => {
            lines.remove(k);
        }
        2 => lines.insert(k, lines[k].clone()),
        3 => {
            let j = rng.below(lines.len());
            lines.swap(k, j);
        }
        4 => {
            let chars: Vec<char> = lines[k].chars().collect();
            let at = rng.below(chars.len().max(1));
            let mut c = chars.clone();
            if !c.is_empty() {
                c[at] = ['.', ',', ':', '(', ')', '"', ' ', '-', 'é', '\t', '\u{0}'][rng.below(11)];
            }
            lines[k] = c.into_iter().collect();
        }
        5 => {
            let w = WORDS[rng.below(WORDS.len())];
            let mut parts: Vec<&str> = lines[k].split(' ').collect();
            parts.insert(rng.below(parts.len() + 1), w);
            lines[k] = parts.join(" ");
        }
        6 => {
            let mut parts: Vec<&str> = lines[k].split(' ').collect();
            let j = rng.below(parts.len());
            parts.remove(j);
            lines[k] = parts.join(" ");
        }
        _ => {
            let mut parts: Vec<&str> = lines[k].split(' ').collect();
            let (a, b) = (rng.below(parts.len()), rng.below(parts.len()));
            parts.swap(a, b);
            lines[k] = parts.join(" ");
        }
    }
    lines.join("\n")
}

/// Whatever a program is turned into, every phase answers with diagnostics and returns.
#[test]
fn mutated_programs_never_crash() {
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 400) * 5;
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x5EED);
    let mut rng = Rng(base.wrapping_mul(3) | 1);
    let mut lex_ok = 0;
    for i in 0..iters {
        let mut src = render(&program_for(mix(base, i % 400)));
        for _ in 0..=rng.below(3) {
            src = mutate(&src, &mut rng);
        }
        if front_end(&src).is_empty() {
            lex_ok += 1;
        }
    }
    // Some mutations are harmless (a swap of equal lines); the rest are rejected.
    assert!(
        lex_ok < usize::try_from(iters).unwrap(),
        "nothing was ever rejected"
    );
}

/// The prosody driver reads the same mutated text: pragmas, stray asides and end words.
#[test]
fn mutated_programs_never_crash_prosody() {
    use syntaxeon_lex::prosody;
    use syntaxeon_lexicon::lexicon::Lexicon;
    let lexicon = Lexicon::pinned().unwrap();
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x5EED);
    let mut rng = Rng(base.wrapping_mul(5) | 1);
    for i in 0..150 {
        let mut src = render(&program_for(mix(base, i)));
        src.push_str("\n(pronounced \"blorn\" as \"B L AO1 R N\")\n");
        for _ in 0..=rng.below(3) {
            src = mutate(&src, &mut rng);
        }
        let lexed = lex(&src);
        let verses = stanzas(&lexed);
        let mut lex = lexicon.clone();
        let _ = prosody::check(&lexed, &verses, &mut lex);
    }
}
