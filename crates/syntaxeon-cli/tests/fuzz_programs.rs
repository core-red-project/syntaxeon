#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
#![allow(clippy::too_many_lines)] // the generator and the reference model are long match tables
#![allow(clippy::many_single_char_names)] // a/b/e/c name the operands of the model's nodes
//! Whole-program fuzzing. A generator writes random, well-typed integer programs (functions,
//! nested `If`/`Otherwise`, `Repeat`, bounded `As long as`, shadowing, early returns, overflow
//! and division by zero) together with a tiny reference interpreter written independently of
//! the real one. The program goes through lexer → parser → checker → evaluator and its output
//! and error code must equal the model's. Mutated programs must be rejected, never crash.
//!
//! Prosody is fuzzed too, but apart from this pipeline: end words are steered by pragmas and
//! every stanza is compared with a rhyme oracle. Deterministic: a fixed xorshift seed.
//! `SYNTAXEON_FUZZ_ITERS` and `SYNTAXEON_FUZZ_SEED` scale and vary a run.

use std::collections::HashMap;
use std::fmt::Write as _;

use syntaxeon_check::check;
use syntaxeon_eval::run_vm as run;
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
    pipeline_with(src, b"")
}

/// Like [`pipeline`], with `input` on standard input (what `hear` reads).
fn pipeline_with(src: &str, input: &[u8]) -> Result<Outcome, Vec<Diag>> {
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
            .spawn_scoped(s, || run(&program, &mut &input[..], &mut out))
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

// ---- rhyme in generated programs ---------------------------------------------------------

/// A rhyme oracle over the raw dictionary, written from `spec/PROSODY.md` §4 and §7 and
/// sharing no code with the crate (a copy of the one in the lexicon's own fuzz test).
/// shortcut: duplicated across test crates, share it if a third user appears.
mod oracle {
    use std::collections::HashMap;

    pub type Dict = HashMap<String, Vec<Vec<String>>>;

    pub fn load() -> Dict {
        let mut m = Dict::new();
        let text = include_str!("../../syntaxeon-lexicon/lexicon/cmudict.dict");
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let mut it = line.split_whitespace();
            let Some(head) = it.next() else { continue };
            let word = head.split('(').next().unwrap().to_lowercase();
            m.entry(word)
                .or_default()
                .push(it.map(str::to_string).collect());
        }
        m
    }

    fn bare(t: &str) -> String {
        t.trim_end_matches(char::is_numeric).to_string()
    }

    /// (tail without stress digits, onset, vowels of the tail).
    pub fn split(p: &[String]) -> Option<(Vec<String>, Option<String>, Vec<String>)> {
        let ends = |d: char| p.iter().rposition(|t| t.ends_with(d));
        let idx = ends('1')
            .or_else(|| ends('2'))
            .or_else(|| p.iter().rposition(|t| t.ends_with(char::is_numeric)))?;
        let tail = p[idx..].iter().map(|t| bare(t)).collect();
        let vowels = p[idx..]
            .iter()
            .filter(|t| t.ends_with(char::is_numeric))
            .map(|t| bare(t))
            .collect();
        Some((tail, idx.checked_sub(1).map(|i| bare(&p[i])), vowels))
    }

    /// Hyphenated words rhyme on their last component (§5).
    pub fn key(w: &str) -> &str {
        w.rsplit('-').next().unwrap_or(w)
    }

    pub fn rhyme(d: &Dict, a: &str, b: &str, assonance: bool) -> Option<bool> {
        let (pa, pb) = (d.get(key(a))?, d.get(key(b))?);
        Some(
            a != b
                && pa.iter().any(|x| {
                    pb.iter().any(|y| match (split(x), split(y)) {
                        (Some((tx, ox, vx)), Some((ty, oy, vy))) => {
                            let identical = tx == ty && ox == oy;
                            (tx == ty && !identical) || (assonance && !identical && vx == vy)
                        }
                        _ => false,
                    })
                }),
        )
    }

    /// Couplet chain, with the last three verses forming one group when `n` is odd.
    pub fn schemes(n: usize) -> Vec<Vec<usize>> {
        if n < 2 {
            return vec![];
        }
        let mut c: Vec<usize> = (0..n / 2 * 2).map(|i| i / 2).collect();
        if n % 2 == 1 {
            c.push(c.last().copied().unwrap_or(0));
        }
        let mut out = vec![c];
        if n == 4 {
            out.extend([vec![0, 1, 0, 1], vec![0, 1, 1, 0]]);
        }
        out
    }

    /// `None` when a word is unknown (the crate must report `SXP-301`).
    pub fn stanza_ok(d: &Dict, words: &[&str], assonance: bool) -> bool {
        schemes(words.len()).iter().any(|s| {
            // Every pair inside a group rhymes.
            (0..s.len()).all(|i| {
                (0..i)
                    .all(|j| s[j] != s[i] || rhyme(d, words[j], words[i], assonance) == Some(true))
            })
        })
    }
}

const ONSETS: &[&str] = &[
    "B", "D", "F", "G", "K", "L", "M", "N", "P", "S", "T", "V", "Z",
];
const TAILS: &[&str] = &["AY1 T", "EH1 N", "OW1 L", "ER1 S", "AA1 K T", "IY1 M"];

/// Renames every program name to a word the lexicon cannot know, so that a pragma can give it
/// whatever pronunciation a stanza needs.
fn unknown_names(src: &str) -> String {
    let mut out = src.to_string();
    for n in VARS.iter().chain(FUNS).chain(PARAMS) {
        let mut next = String::new();
        let mut word = String::new();
        let flush = |w: &mut String, next: &mut String| {
            if w == n {
                next.push_str("xq");
            }
            next.push_str(w);
            w.clear();
        };
        for c in out.chars() {
            if c.is_ascii_alphabetic() {
                word.push(c);
            } else {
                flush(&mut word, &mut next);
                next.push(c);
            }
        }
        flush(&mut word, &mut next);
        out = next;
    }
    out
}

/// Steers the end words of each rhyme group by pragma, except for a random third of the groups.
fn steer(src: &str, dict: &oracle::Dict, rng: &mut Rng) -> String {
    let lexed = lex(src);
    let mut pragmas: HashMap<String, String> = HashMap::new();
    for stanza in stanzas(&lexed) {
        let words: Vec<String> = stanza
            .verses
            .iter()
            .filter(|v| !v.structural)
            .filter_map(|v| v.end_word.clone().ok())
            .collect();
        let n = words.len();
        let Some(scheme) = oracle::schemes(n).into_iter().next() else {
            continue;
        };
        for (i, g) in scheme.iter().enumerate() {
            let lead = scheme.iter().position(|x| x == g).unwrap();
            if lead == i || rng.below(3) == 0 {
                continue;
            }
            let free = |w: &str| w.starts_with("xq") && !pragmas.contains_key(w);
            let tail_of =
                |w: &str, pragmas: &HashMap<String, String>| -> Option<(String, String)> {
                    let toks: Vec<String> = match pragmas.get(w) {
                        Some(p) => p.split(' ').map(str::to_string).collect(),
                        None => dict.get(oracle::key(w))?.first()?.clone(),
                    };
                    let (tail, onset, _) = oracle::split(&toks)?;
                    let idx = toks.len() - tail.len();
                    Some((toks[idx..].join(" "), onset.unwrap_or_default()))
                };
            let (a, b) = (&words[lead], &words[i]);
            let (anchor, target) = if free(b) {
                (a, b)
            } else if free(a) {
                (b, a)
            } else {
                continue;
            };
            let (tail, onset) = if free(anchor) {
                // Both ends are free: pick the tail and let `anchor` take its own onset first.
                let tail = TAILS[rng.below(TAILS.len())].to_string();
                let o = ONSETS[rng.below(ONSETS.len())].to_string();
                pragmas.insert(anchor.clone(), format!("{o} {tail}"));
                (tail, o)
            } else if let Some(t) = tail_of(anchor, &pragmas) {
                t
            } else {
                continue;
            };
            let choices: Vec<&&str> = ONSETS.iter().filter(|o| **o != onset).collect();
            let o = choices[rng.below(choices.len())];
            pragmas.insert(target.clone(), format!("{o} {tail}"));
        }
    }
    let mut out = src.to_string();
    out.push('\n');
    let mut sorted: Vec<_> = pragmas.into_iter().collect();
    sorted.sort();
    for (w, p) in sorted {
        writeln!(out, "(pronounced \"{w}\" as \"{p}\")").unwrap();
    }
    out
}

/// Compares the crate's per-stanza prosody verdict with the oracle's on the same end words.
/// Returns the (valid, invalid) stanza counts.
fn prosody_agrees(
    src: &str,
    dict: &mut oracle::Dict,
    assonance: bool,
    what: &str,
) -> (usize, usize) {
    use syntaxeon_lex::prosody;
    use syntaxeon_lexicon::lexicon::{Lexicon, RhymeRules};
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    assert!(
        lexed.diagnostics.is_empty(),
        "{:?}\n{src}",
        lexed.diagnostics
    );
    let mut lexicon = Lexicon::pinned().unwrap();
    lexicon.set_rhyme_rules(RhymeRules { assonance });
    let real = prosody::check(&lexed, &verses, &mut lexicon);
    // Pragma words join the oracle as ordinary pronunciations.
    for p in &lexed.pragmas {
        dict.insert(
            p.word.clone(),
            vec![p.phones.split(' ').map(str::to_string).collect()],
        );
    }
    // A diagnostic points at one verse of its stanza; compare which stanzas, not which line.
    let stanza_of = |line: usize| {
        verses
            .iter()
            .position(|s| s.verses.iter().any(|v| v.line == line))
    };
    let failing_real: Vec<Option<usize>> = real.iter().map(|d| stanza_of(d.line)).collect();
    let (mut valid, mut invalid) = (0, 0);
    let mut failing_oracle = Vec::new();
    for (index, s) in verses.iter().enumerate() {
        let words: Vec<&str> = s
            .verses
            .iter()
            .filter(|v| !v.structural)
            .map(|v| v.end_word.as_deref().unwrap())
            .collect();
        if oracle::stanza_ok(dict, &words, assonance) {
            valid += 1;
        } else {
            invalid += 1;
            failing_oracle.push(Some(index));
        }
    }
    for p in &lexed.pragmas {
        dict.remove(&p.word);
    }
    assert_eq!(
        failing_real, failing_oracle,
        "{what} (assonance {assonance})\n{src}"
    );
    (valid, invalid)
}

/// Prosody on generated programs: whatever the random end words are (steered or not), the
/// crate and the oracle agree on every stanza. Valid stanzas are rare here (headers and
/// numerals end in fixed words), so the poems below carry the positive cases.
#[test]
fn rhyme_in_generated_programs_agrees_with_the_oracle() {
    let mut dict = oracle::load();
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x000A_11CE);
    let mut rng = Rng(base.wrapping_mul(7) | 1);
    for i in 0..env_or("SYNTAXEON_FUZZ_ITERS", 40) {
        let src = steer(
            &unknown_names(&render(&program_for(mix(base, i)))),
            &dict,
            &mut rng,
        );
        for assonance in [false, true] {
            prosody_agrees(
                &src,
                &mut dict,
                assonance,
                &format!("seed {base} program {i}"),
            );
        }
    }
}

/// Stanzas written only to rhyme (they need not type-check): mostly free-name endings with a
/// few fixed ones (numerals, quoted text, headers) and exempt `Otherwise:` / `So be it.`.
fn poem(rng: &mut Rng) -> String {
    let name = |rng: &mut Rng| {
        format!(
            "the {}",
            ["pile", "ember", "hero", "bird", "stack", "tally"][rng.below(6)]
        )
    };
    let mut out = String::from("Thus begins the tale of men:\n");
    for st in 0..=rng.below(3) {
        if st > 0 {
            out.push('\n');
        }
        for _ in 0..=rng.below(8) {
            let line = match rng.below(14) {
                0 => "Proclaim 7.".to_string(),
                1 => "Speak \"Hello again\".".to_string(),
                2 => "Proclaim the text of 21.".to_string(),
                3 => "If the pile is the ember, then:".to_string(),
                4 => "Do this 3 times:".to_string(),
                5 => "Otherwise:".to_string(),
                6 => "So be it.".to_string(),
                7 => format!("Give back {}.", name(rng)),
                8 => format!("Let the pile now be {}.", name(rng)),
                9 => format!(
                    "Let the hero, a whole number, become the sum of {} and {}.",
                    name(rng),
                    name(rng)
                ),
                _ => format!("Proclaim the text of {}.", name(rng)),
            };
            out.push_str(&unknown_names(&line));
            out.push('\n');
        }
    }
    out
}

#[test]
fn rhymed_poems_agree_with_the_oracle() {
    let mut dict = oracle::load();
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0xB0E7);
    let iters = usize::try_from(env_or("SYNTAXEON_FUZZ_ITERS", 40)).unwrap();
    let mut rng = Rng(base.wrapping_mul(11) | 1);
    let (mut valid, mut invalid) = (0, 0);
    for i in 0..iters {
        let src = steer(&poem(&mut rng), &dict, &mut rng);
        for assonance in [false, true] {
            let (v, n) =
                prosody_agrees(&src, &mut dict, assonance, &format!("seed {base} poem {i}"));
            valid += v;
            invalid += n;
        }
    }
    assert!(
        valid * 8 >= iters && invalid * 8 >= iters,
        "unbalanced: {valid} valid / {invalid} invalid"
    );
}

/// `fmt` on generated programs whose whitespace is mangled: the result equals the formatting of
/// the clean text, is stable, and still passes `check`.
#[test]
fn fmt_ignores_mangled_whitespace() {
    use syntaxeon_cli::{Env, run};
    let env = Env {
        stdout_tty: false,
        stderr_tty: false,
        no_color_env: true,
    };
    let fmt = |src: &str| {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(
            &["fmt".into(), "-".into()],
            &env,
            &mut src.as_bytes(),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    };
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0xF0_F0);
    let mut rng = Rng(base | 1);
    for i in 0..env_or("SYNTAXEON_FUZZ_ITERS", 40).min(60) {
        let clean = render(&program_for(mix(base, i)));
        let mut mangled = String::from("\n\n");
        for line in clean.lines() {
            mangled.push_str(line);
            if line.is_empty() {
                mangled.push_str(["", "\n", "\n\n"][rng.below(3)]);
            } else {
                mangled.push_str(["", "  ", "   "][rng.below(3)]);
            }
            mangled.push('\n');
        }
        mangled.push_str("\n\n");
        let (code, once, err) = fmt(&clean);
        assert_eq!(code, 0, "{err}\n{clean}");
        let (code, again, _) = fmt(&once);
        assert_eq!(code, 0);
        assert_eq!(again, once, "not stable\n{clean}");
        assert_eq!(fmt(&mangled).1, once, "mangled\n{mangled}");
    }
}

/// One stanza of declarations and assignments in random spellings (D1–D3, A1–A3), with
/// pragma-steered rhymes. Whenever such a program is valid, `fmt` must keep it valid and
/// equal in meaning, and be stable; some of them must actually be rewritten.
#[test]
fn fmt_keeps_rhyming_programs_valid() {
    use syntaxeon_cli::{Env, run};
    let env = Env {
        stdout_tty: false,
        stderr_tty: false,
        no_color_env: true,
    };
    let exec = |args: &[&str], src: &str| {
        let argv: Vec<String> = args.iter().map(ToString::to_string).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&argv, &env, &mut src.as_bytes(), &mut out, &mut err);
        (code, String::from_utf8(out).unwrap())
    };
    let meaning = |src: &str| {
        let (code, ast) = exec(&["ast", "-"], src);
        assert_eq!(code, 0);
        ast.lines()
            .filter(|l| !l.contains("line:"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let dict = oracle::load();
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0xF0_0D);
    let mut rng = Rng(base | 1);
    let (mut valid, mut rewritten) = (0_u64, 0_u64);
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 40) * 3;
    for _ in 0..iters {
        let mut declared: Vec<&str> = Vec::new();
        let mut text = format!(
            "Thus begins the tale of {}:\n",
            ["men", "thumb"][rng.below(2)]
        );
        for _ in 0..=(2 * rng.below(2)) {
            let expr = if declared.is_empty() || rng.below(10) < 2 {
                ["one", "two", "three", "nine"][rng.below(4)].to_string()
            } else {
                format!("the {}", declared[rng.below(declared.len())])
            };
            let cap = |s: String| {
                let mut c = s.chars();
                c.next()
                    .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
            };
            let line = if declared.is_empty() || (declared.len() < VARS.len() && rng.below(2) == 0)
            {
                let v = VARS[declared.len()];
                declared.push(v);
                match rng.below(3) {
                    0 => format!("The {v}, a whole number, shall become {expr}."),
                    1 => format!("Let the {v}, a whole number, become {expr}."),
                    _ => cap(format!("{expr} shall the {v}, a whole number, become.")),
                }
            } else {
                let v = declared[rng.below(declared.len())];
                match rng.below(3) {
                    0 => format!("The {v} shall now be {expr}."),
                    1 => format!("Let the {v} now be {expr}."),
                    _ => cap(format!("{expr} shall the {v} now be.")),
                }
            };
            text.push_str(&line);
            text.push('\n');
        }
        let src = steer(&unknown_names(&text), &dict, &mut rng);
        if exec(&["check", "-q", "-"], &src).0 != 0 {
            continue;
        }
        valid += 1;
        let (code, once) = exec(&["fmt", "-"], &src);
        assert_eq!(code, 0, "{src}");
        assert_eq!(
            exec(&["check", "-q", "-"], &once).0,
            0,
            "fmt broke the rhyme:\n{src}\n---\n{once}"
        );
        assert_eq!(meaning(&once), meaning(&src), "{src}\n---\n{once}");
        assert_eq!(
            exec(&["fmt", "-"], &once).1,
            once,
            "not stable:\n{src}\n---\n{once}"
        );
        rewritten += u64::from(once != src);
    }
    assert!(
        valid * 16 >= iters,
        "too few valid programs: {valid}/{iters}"
    );
    assert!(
        rewritten * 16 >= valid,
        "too few rewrites: {rewritten}/{valid}"
    );
}

// ---- strings and floats ------------------------------------------------------------------
//
// A second, straight-line generator over `Integer`, `Float`, `String`, `Character` and `Boolean`
// values with the conversions between them, `hear` and the input tests, checked against a model of
// its own. Control flow is covered above.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    Int,
    Float,
    Text,
    Char,
    Bool,
}

/// What a declaration says of each type, indexed by `Ty as usize`.
const TY_NOTES: [&str; 5] = [
    "a whole number",
    "a fraction",
    "a text",
    "a letter",
    "a truth",
];
const CHARS: &[char] = &['a', 'Z', ' ', 'é', '7', '~'];

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i64),
    Float(f64),
    Text(String),
    Char(char),
    Bool(bool),
}

#[derive(Clone, Debug)]
enum X {
    Lit(Val, String),
    Var(usize),
    Neg(Box<X>),
    Arith(Op, Box<X>, Box<X>),
    /// `the text of`
    ToText(Box<X>),
    /// `the length of`
    Len(Box<X>),
    /// `the whole number of`
    Trunc(Box<X>),
    /// `the fraction of`
    Frac(Box<X>),
    /// `what is heard`
    Hear,
    /// `the whole number of` a text
    ParseInt(Box<X>),
    /// `the code of` a letter
    Code(Box<X>),
    /// `the letter with code`
    ToLetter(Box<X>),
    /// `item N of` a text
    Index(Box<X>, Box<X>),
    /// `there is more to hear`
    More,
    /// `… reads as a whole number`
    Reads(Box<X>),
}

/// The lines of standard input still to be heard.
type Input = std::iter::Peekable<std::vec::IntoIter<String>>;

/// A declared variable: its type and its current value in the model.
type Slot = (Ty, Option<Val>);

struct Scalars {
    rng: Rng,
    vars: Vec<Ty>,
}

const FLOATS: &[(f64, &str)] = &[
    (0.5, "0.5"),
    (1.5, "1.5"),
    (2.25, "2.25"),
    (10.0, "10.0"),
    (0.1, "0.1"),
    (3.75, "3.75"),
    (100.5, "100.5"),
];
const TEXTS: &[&str] = &["a", "bc", "hello", "Zed", "x y", "", "tale of two"];

impl Scalars {
    /// A truth that can stand where an operand is required.
    fn bool_operand(&mut self) -> X {
        let vars: Vec<usize> = (0..self.vars.len())
            .filter(|i| self.vars[*i] == Ty::Bool)
            .collect();
        if !vars.is_empty() && self.rng.chance(50) {
            return X::Var(vars[self.rng.below(vars.len())]);
        }
        X::More
    }

    fn expr(&mut self, want: Ty, depth: usize) -> X {
        let vars: Vec<usize> = (0..self.vars.len())
            .filter(|i| self.vars[*i] == want)
            .collect();
        if depth == 0 || self.rng.chance(25) {
            if !vars.is_empty() && self.rng.chance(50) {
                return X::Var(vars[self.rng.below(vars.len())]);
            }
            if want == Ty::Text && self.rng.chance(25) {
                return X::Hear;
            }
            if want == Ty::Bool && self.rng.chance(60) {
                return X::More;
            }
            return match want {
                Ty::Int => {
                    let n = i64::try_from(self.rng.below(3000)).unwrap();
                    let n = if self.rng.chance(10) { i64::MAX - n } else { n };
                    // Around the edges of what is a letter: surrogates and the end of Unicode.
                    let edges = [0xD7FF, 0xD800, 0xDFFF, 0xE000, 0x10_FFFF, 0x11_0000];
                    let n = if self.rng.chance(8) {
                        edges[self.rng.below(edges.len())]
                    } else {
                        n
                    };
                    X::Lit(Val::Int(n), n.to_string())
                }
                Ty::Float => {
                    let (f, t) = FLOATS[self.rng.below(FLOATS.len())];
                    X::Lit(Val::Float(f), t.to_string())
                }
                Ty::Text => {
                    let w = TEXTS[self.rng.below(TEXTS.len())];
                    X::Lit(Val::Text(w.to_string()), format!("\"{w}\""))
                }
                Ty::Char => {
                    let c = CHARS[self.rng.below(CHARS.len())];
                    X::Lit(Val::Char(c), format!("the letter \"{c}\""))
                }
                Ty::Bool => {
                    let inner = self.expr(Ty::Text, 0);
                    X::Reads(Box::new(inner))
                }
            };
        }
        let d = depth - 1;
        let b = |x: X| Box::new(x);
        match (want, self.rng.below(8)) {
            (Ty::Text, 0 | 1) => X::Arith(
                Op::Sum,
                b(self.expr(Ty::Text, d)),
                b(self.expr(Ty::Text, d)),
            ),
            (Ty::Text, 2) => X::Hear,
            (Ty::Text, _) => {
                let from = [Ty::Int, Ty::Float, Ty::Char, Ty::Bool][self.rng.below(4)];
                // `reads as` is not an operand, so a truth is only ever shown through a name
                // or the question about input.
                let arg = if from == Ty::Bool {
                    self.bool_operand()
                } else {
                    self.expr(from, d)
                };
                X::ToText(b(arg))
            }
            (Ty::Int, 0) => X::Len(b(self.expr(Ty::Text, d))),
            (Ty::Int, 1) => X::Trunc(b(self.expr(Ty::Float, d))),
            (Ty::Int, 2) => X::Neg(b(self.expr(Ty::Int, d))),
            (Ty::Int, 3) => X::ParseInt(b(self.expr(Ty::Text, d))),
            (Ty::Int, 4) => X::Code(b(self.expr(Ty::Char, d))),
            (Ty::Int, _) => {
                let op = [
                    Op::Sum,
                    Op::Difference,
                    Op::Product,
                    Op::Quotient,
                    Op::Remainder,
                ][self.rng.below(5)];
                X::Arith(op, b(self.expr(Ty::Int, d)), b(self.expr(Ty::Int, d)))
            }
            (Ty::Char, 0..=3) => X::ToLetter(b(self.expr(Ty::Int, d))),
            (Ty::Char, _) => {
                // A small index, or a variable: mostly out of range, sometimes not.
                let n = i64::try_from(self.rng.below(4)).unwrap();
                let idx = X::Lit(Val::Int(n), n.to_string());
                X::Index(b(self.expr(Ty::Text, d)), b(idx))
            }
            (Ty::Bool, 0..=3) => self.bool_operand(),
            (Ty::Bool, _) => X::Reads(b(self.expr(Ty::Text, d))),
            (Ty::Float, 0) => X::Frac(b(self.expr(Ty::Int, d))),
            (Ty::Float, 1) => X::Neg(b(self.expr(Ty::Float, d))),
            (Ty::Float, _) => {
                let op = [Op::Sum, Op::Difference, Op::Product, Op::Quotient][self.rng.below(4)];
                X::Arith(op, b(self.expr(Ty::Float, d)), b(self.expr(Ty::Float, d)))
            }
        }
    }
}

fn render_x(x: &X, out: &mut String) {
    let word = |op: &Op| match op {
        Op::Sum => "sum",
        Op::Difference => "difference",
        Op::Product => "product",
        Op::Quotient => "quotient",
        Op::Remainder => "remainder",
    };
    match x {
        X::Lit(_, text) => out.push_str(text),
        X::Var(i) => write!(out, "the {}", VARS[*i]).unwrap(),
        X::Neg(a) => {
            out.push_str("the negation of ");
            render_x(a, out);
        }
        X::Arith(op, a, b) => {
            write!(out, "the {} of ", word(op)).unwrap();
            render_x(a, out);
            out.push_str(" and ");
            render_x(b, out);
        }
        X::ToText(a) => {
            out.push_str("the text of ");
            render_x(a, out);
        }
        X::Len(a) => {
            out.push_str("the length of ");
            render_x(a, out);
        }
        X::Frac(a) => {
            out.push_str("the fraction of ");
            render_x(a, out);
        }
        X::Hear => out.push_str("what is heard"),
        X::More => out.push_str("there is more to hear"),
        X::Code(a) => {
            out.push_str("the code of ");
            render_x(a, out);
        }
        X::ToLetter(a) => {
            out.push_str("the letter with code ");
            render_x(a, out);
        }
        X::Index(text, idx) => {
            out.push_str("item ");
            render_x(idx, out);
            out.push_str(" of ");
            render_x(text, out);
        }
        X::Reads(a) => {
            render_x(a, out);
            out.push_str(" reads as a whole number");
        }
        X::Trunc(a) | X::ParseInt(a) => {
            out.push_str("the whole number of ");
            render_x(a, out);
        }
    }
}

/// Model evaluation; `Err` is the runtime error code.
fn eval_x(x: &X, env: &[Slot], input: &mut Input) -> Result<Val, &'static str> {
    Ok(match x {
        X::Lit(v, _) => v.clone(),
        X::Var(i) => env[*i].1.clone().expect("declared before use"),
        X::Neg(a) => match eval_x(a, env, input)? {
            Val::Int(n) => Val::Int(n.checked_neg().ok_or("SXP-501")?),
            Val::Float(f) => Val::Float(-f),
            _ => unreachable!("typed"),
        },
        X::Arith(op, a, b) => match (eval_x(a, env, input)?, eval_x(b, env, input)?) {
            (Val::Int(p), Val::Int(q)) => Val::Int(match op {
                Op::Sum => p.checked_add(q).ok_or("SXP-501")?,
                Op::Difference => p.checked_sub(q).ok_or("SXP-501")?,
                Op::Product => p.checked_mul(q).ok_or("SXP-501")?,
                Op::Quotient | Op::Remainder if q == 0 => return Err("SXP-502"),
                Op::Quotient => p.checked_div(q).ok_or("SXP-501")?,
                // `MIN mod -1` is 0 (SEMANTIC_CORE §10), which Rust's checked_rem refuses.
                Op::Remainder if q == -1 => 0,
                Op::Remainder => p % q,
            }),
            (Val::Float(p), Val::Float(q)) => Val::Float(match op {
                Op::Sum => p + q,
                Op::Difference => p - q,
                Op::Product => p * q,
                _ => p / q,
            }),
            (Val::Text(p), Val::Text(q)) => Val::Text(p + &q),
            _ => unreachable!("typed"),
        },
        X::ToText(a) => Val::Text(match eval_x(a, env, input)? {
            Val::Int(n) => n.to_string(),
            Val::Float(f) => format!("{f:?}"),
            Val::Char(c) => c.to_string(),
            Val::Bool(b) => b.to_string(),
            Val::Text(_) => unreachable!("typed"),
        }),
        X::Len(a) => match eval_x(a, env, input)? {
            Val::Text(s) => Val::Int(i64::try_from(s.chars().count()).unwrap()),
            _ => unreachable!("typed"),
        },
        X::Trunc(a) => match eval_x(a, env, input)? {
            Val::Float(f)
                if f.is_nan()
                    || f >= 9_223_372_036_854_775_808.0
                    || f < -9_223_372_036_854_775_808.0 =>
            {
                return Err("SXP-506");
            }
            #[allow(clippy::cast_possible_truncation)] // range checked on the line above
            Val::Float(f) => Val::Int(f.trunc() as i64),
            _ => unreachable!("typed"),
        },
        X::Hear => Val::Text(input.next().ok_or("SXP-505")?),
        X::ParseInt(a) => match eval_x(a, env, input)? {
            Val::Text(s) => Val::Int(parse_int(&s).ok_or("SXP-506")?),
            _ => unreachable!("typed"),
        },
        X::More => Val::Bool(input.peek().is_some()),
        X::Code(a) => match eval_x(a, env, input)? {
            Val::Char(c) => Val::Int(i64::from(u32::from(c))),
            _ => unreachable!("typed"),
        },
        X::ToLetter(a) => match eval_x(a, env, input)? {
            // A letter is a Unicode scalar value: no surrogates, nothing past U+10FFFF.
            Val::Int(n) => Val::Char(
                u32::try_from(n)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or("SXP-506")?,
            ),
            _ => unreachable!("typed"),
        },
        X::Index(text, idx) => match (eval_x(text, env, input)?, eval_x(idx, env, input)?) {
            (Val::Text(s), Val::Int(n)) => Val::Char(
                usize::try_from(n)
                    .ok()
                    .and_then(|k| s.chars().nth(k))
                    .ok_or("SXP-503")?,
            ),
            _ => unreachable!("typed"),
        },
        X::Reads(a) => match eval_x(a, env, input)? {
            Val::Text(s) => Val::Bool(parse_int(&s).is_some()),
            _ => unreachable!("typed"),
        },
        X::Frac(a) => match eval_x(a, env, input)? {
            #[allow(clippy::cast_precision_loss)] // the language rounds the same way
            Val::Int(n) => Val::Float(n as f64),
            _ => unreachable!("typed"),
        },
    })
}

/// A whole number as `to_integer` reads it from text: an optional sign, then ASCII digits and
/// nothing else (no spaces, no separators), within `Integer`'s range (`SEMANTIC_CORE.md` §8).
fn parse_int(s: &str) -> Option<i64> {
    let (neg, digits) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut n: i128 = 0;
    for b in digits.bytes() {
        n = n * 10 + i128::from(b - b'0');
        if n > i128::from(i64::MAX) + 1 {
            return None;
        }
    }
    i64::try_from(if neg { -n } else { n }).ok()
}

const INPUT_LINES: &[&str] = &[
    "42",
    "-7",
    "0",
    "+3",
    "007",
    "abc",
    "",
    "1.5",
    " 5",
    "5 ",
    "9223372036854775807",
    "9223372036854775808",
    "-9223372036854775808",
    "-9223372036854775809",
    "x y",
    "tale",
    "+",
    "-",
];

/// Lines of standard input and its bytes: random terminators (`\n` or `\r\n`), the last line
/// sometimes left unterminated (never empty then, which would just be the end of input).
fn input_for(rng: &mut Rng) -> (Vec<String>, Vec<u8>) {
    let n = rng.below(6);
    let lines: Vec<String> = (0..n)
        .map(|_| INPUT_LINES[rng.below(INPUT_LINES.len())].to_string())
        .collect();
    let mut bytes = String::new();
    for (k, l) in lines.iter().enumerate() {
        bytes.push_str(l);
        let last = k + 1 == lines.len();
        if last && !l.is_empty() && rng.chance(30) {
            continue;
        }
        bytes.push_str(["\n", "\r\n"][rng.below(2)]);
    }
    (lines, bytes.into_bytes())
}

/// Program text, its standard input, and the model's `(output, error)`.
fn scalar_program(seed: u64) -> (String, Vec<u8>, Outcome) {
    let mut g = Scalars {
        rng: Rng(seed | 1),
        vars: vec![],
    };
    let mut env: Vec<Slot> = Vec::new();
    let (lines, stdin) = input_for(&mut g.rng);
    let mut input: Input = lines.into_iter().peekable();
    let mut src = String::from("Thus begins the tale:\n");
    let mut out = String::new();
    let mut failed: Option<&'static str> = None;
    for _ in 0..4 + g.rng.below(14) {
        let declare = g.vars.len() < VARS.len() && (g.vars.is_empty() || g.rng.chance(55));
        if declare {
            let ty = [Ty::Int, Ty::Float, Ty::Text, Ty::Char, Ty::Bool][g.rng.below(5)];
            let x = g.expr(ty, 3);
            let note = TY_NOTES[ty as usize];
            let mut e = String::new();
            render_x(&x, &mut e);
            writeln!(src, "Let the {}, {note}, become {e}.", VARS[g.vars.len()]).unwrap();
            g.vars.push(ty);
            env.push((ty, None));
            if failed.is_none() {
                match eval_x(&x, &env, &mut input) {
                    Ok(v) => env[g.vars.len() - 1].1 = Some(v),
                    Err(c) => failed = Some(c),
                }
            }
        } else {
            let ty = [Ty::Int, Ty::Float, Ty::Text, Ty::Char, Ty::Bool][g.rng.below(5)];
            let x = if ty == Ty::Bool {
                g.bool_operand()
            } else {
                g.expr(ty, 3)
            };
            let mut e = String::new();
            render_x(&x, &mut e);
            if ty == Ty::Text {
                writeln!(src, "Proclaim {e}.").unwrap();
            } else {
                writeln!(src, "Proclaim the text of {e}.").unwrap();
            }
            if failed.is_none() {
                match eval_x(&x, &env, &mut input) {
                    Ok(Val::Text(s)) => writeln!(out, "{s}").unwrap(),
                    Ok(Val::Int(n)) => writeln!(out, "{n}").unwrap(),
                    Ok(Val::Float(f)) => writeln!(out, "{f:?}").unwrap(),
                    Ok(Val::Char(c)) => writeln!(out, "{c}").unwrap(),
                    Ok(Val::Bool(b)) => writeln!(out, "{b}").unwrap(),
                    Err(c) => failed = Some(c),
                }
            }
        }
        if failed.is_some() {
            break;
        }
    }
    (src, stdin, (out, failed))
}

#[test]
fn scalar_programs_behave_like_the_model() {
    let base = env_or("SYNTAXEON_FUZZ_SEED", 0x5CA1);
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 400);
    let (mut errors, mut floats, mut texts, mut hears) = (0, 0, 0, 0);
    let (mut codes, mut letters, mut mores, mut reads) = (0, 0, 0, 0);
    for i in 0..iters {
        let seed = mix(base, i);
        let (src, stdin, want) = scalar_program(seed);
        let got = pipeline_with(&src, &stdin).unwrap_or_else(|codes| {
            panic!("seed {seed:#x} rejected {codes:?}\n--- program\n{src}\n--- stdin\n{stdin:?}");
        });
        assert_eq!(
            got, want,
            "seed {seed:#x}\n--- program\n{src}\n--- stdin\n{stdin:?}"
        );
        hears += usize::from(src.contains("heard"));
        errors += usize::from(want.1.is_some());
        floats += usize::from(src.contains("fraction"));
        texts += usize::from(src.contains("a text"));
        codes += usize::from(src.contains("the code of"));
        letters += usize::from(src.contains("letter with code"));
        mores += usize::from(src.contains("more to hear"));
        reads += usize::from(src.contains("reads as a whole number"));
    }
    let n = usize::try_from(iters).unwrap();
    assert!(
        errors * 20 >= n && floats * 4 >= n && texts * 4 >= n && hears * 4 >= n,
        "weak coverage: {errors} errors, {floats} floats, {texts} texts of {n}"
    );
    assert!(
        codes * 10 >= n && letters * 10 >= n && mores * 10 >= n && reads * 10 >= n,
        "weak coverage of the primitives: {codes} codes, {letters} letters, {mores} mores, {reads} reads of {n}"
    );
}
