//! Sentence-level grammar (`spec/PROSE_GRAMMAR.md` §4–§7). Every function returns all parses.

use syntaxeon_core::{BinOp, Expr, Name, Param, StmtKind, Target, Type, UnOp};
use syntaxeon_lex::lexer::Tok;

/// All ways to parse a prefix: each result is a value and the index just past it.
type R<T> = Vec<(T, usize)>;

const DETS: [&str; 8] = ["the", "this", "that", "a", "an", "my", "thy", "our"];

/// Words that can never appear inside a `Name` (`spec/PROSE_GRAMMAR.md` §5).
const RESERVED: &[&str] = &[
    "the",
    "this",
    "that",
    "a",
    "an",
    "my",
    "thy",
    "our",
    "shall",
    "be",
    "become",
    "now",
    "let",
    "speak",
    "proclaim",
    "give",
    "return",
    "call",
    "calling",
    "if",
    "then",
    "otherwise",
    "as",
    "do",
    "times",
    "so",
    "it",
    "true",
    "false",
    "both",
    "and",
    "either",
    "or",
    "not",
    "is",
    "less",
    "greater",
    "than",
    "at",
    "least",
    "most",
    "plus",
    "minus",
    "over",
    "modulo",
    "item",
    "taking",
    "giving",
    "thus",
    "what",
    "heard",
    "appended",
    "with",
    "nothing",
];

/// After `the`, a name starting with one of these would collide with a prefix form.
const THE_HEADS: &[&str] = &[
    "sum",
    "difference",
    "product",
    "quotient",
    "remainder",
    "text",
    "whole",
    "fraction",
    "length",
    "negation",
    "letter",
];

/// Builtins produced by conversions; their results are not `with … appended` bases.
const CONVERSIONS: &[&str] = &["to_string", "to_integer", "to_float", "length", "hear"];

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

#[allow(clippy::cast_possible_wrap)] // values are at most 99
fn number_word(w: &str) -> Option<i64> {
    if let Some(n) = ONES.iter().position(|x| *x == w) {
        return Some(n as i64);
    }
    let tens = |s: &str| TENS.iter().position(|x| !x.is_empty() && *x == s);
    if let Some(t) = tens(w) {
        return Some(t as i64 * 10);
    }
    let (t, o) = w.split_once('-')?;
    let ones = ONES
        .iter()
        .position(|x| *x == o)
        .filter(|n| (1..=9).contains(n))?;
    Some((tens(t)? * 10 + ones) as i64)
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}

pub(crate) enum Open {
    If(Expr),
    While(Expr),
    Repeat(Expr),
}

pub(crate) struct P<'a> {
    pub t: &'a [Tok],
}

impl P<'_> {
    fn word(&self, i: usize, w: &str) -> bool {
        matches!(self.t.get(i), Some(Tok::Word(x)) if x == w)
    }

    fn punct(&self, i: usize, c: char) -> bool {
        matches!(self.t.get(i), Some(Tok::Punct(x)) if *x == c)
    }

    fn seq(&self, i: usize, ws: &[&str]) -> Option<usize> {
        ws.iter()
            .enumerate()
            .all(|(k, w)| self.word(i + k, w))
            .then_some(i + ws.len())
    }

    // ---- names and types -------------------------------------------------------------

    /// `[determiner] word { word }`; a quoted string is a name too where `quoted` is set.
    /// `as_var` forbids a bare number-word, which is a literal there.
    fn name(&self, i: usize, as_var: bool, quoted: bool) -> R<Name> {
        let mut out = Vec::new();
        if quoted && let Some(Tok::Str(s)) = self.t.get(i) {
            out.push((s.clone(), i + 1));
        }
        let det = match self.t.get(i) {
            Some(Tok::Word(w)) if DETS.contains(&w.as_str()) => Some(w.as_str()),
            _ => None,
        };
        let start = i + usize::from(det.is_some());
        let first_bad = match (det, self.t.get(start)) {
            (Some("the"), Some(Tok::Word(w))) => THE_HEADS.contains(&w.as_str()),
            (Some("a" | "an"), Some(Tok::Word(w))) => w == "list" || w == "empty",
            _ => false,
        };
        if first_bad {
            return out;
        }
        let mut words: Vec<&str> = Vec::new();
        while let Some(Tok::Word(w)) = self.t.get(start + words.len()) {
            if RESERVED.contains(&w.as_str()) {
                break;
            }
            words.push(w);
            if as_var && words.len() == 1 && number_word(w).is_some() {
                continue;
            }
            out.push((words.join(" "), start + words.len()));
        }
        out
    }

    fn typ(&self, i: usize) -> R<Type> {
        let mut out = Vec::new();
        let simple: [(&[&str], Type); 5] = [
            (&["whole", "number"], Type::Integer),
            (&["fraction"], Type::Float),
            (&["truth"], Type::Boolean),
            (&["text"], Type::String),
            (&["letter"], Type::Character),
        ];
        for (ws, ty) in simple {
            if let Some(j) = self.seq(i, ws) {
                out.push((ty, j));
            }
        }
        if let Some(j) = self.seq(i, &["list", "of"]) {
            out.extend(
                self.typ(j)
                    .into_iter()
                    .map(|(t, k)| (Type::List(Box::new(t)), k)),
            );
        }
        out
    }

    /// `X { "," X } [ "," "and" X ]`
    fn list_of<T: Clone>(&self, i: usize, f: &dyn Fn(usize) -> R<T>) -> R<Vec<T>> {
        let mut out = Vec::new();
        for (x, j) in f(i) {
            self.list_tail(j, &[x], f, &mut out);
        }
        out
    }

    fn list_tail<T: Clone>(
        &self,
        j: usize,
        acc: &[T],
        f: &dyn Fn(usize) -> R<T>,
        out: &mut R<Vec<T>>,
    ) {
        out.push((acc.to_vec(), j));
        if !self.punct(j, ',') {
            return;
        }
        for (x, k) in f(j + 1) {
            let mut a = acc.to_vec();
            a.push(x);
            self.list_tail(k, &a, f, out);
        }
        if self.word(j + 1, "and") {
            for (x, k) in f(j + 2) {
                let mut a = acc.to_vec();
                a.push(x);
                out.push((a, k));
            }
        }
    }

    // ---- expressions -------------------------------------------------------------------

    fn args(&self, i: usize) -> R<Vec<Expr>> {
        self.list_of(i, &|k| self.expr(k))
    }

    fn atom(&self, i: usize) -> R<Expr> {
        let mut out = Vec::new();
        match self.t.get(i) {
            Some(Tok::Int(n)) => out.push((Expr::Int(*n), i + 1)),
            Some(Tok::Float { value, .. }) => out.push((Expr::Float(*value), i + 1)),
            Some(Tok::Str(s)) => out.push((Expr::Str(s.clone()), i + 1)),
            Some(Tok::Word(w)) => {
                if let Some(n) = number_word(w) {
                    out.push((Expr::Int(n), i + 1));
                }
                if w == "true" || w == "false" {
                    out.push((Expr::Bool(w == "true"), i + 1));
                }
            }
            _ => {}
        }
        if let (Some(j), Some(Tok::Str(s))) = (self.seq(i, &["the", "letter"]), self.t.get(i + 2)) {
            let mut cs = s.chars();
            if let (Some(c), None) = (cs.next(), cs.next()) {
                out.push((Expr::Char(c), j + 1));
            }
        }
        if let Some(j) = self.seq(i, &["what", "is", "heard"]) {
            out.push((Expr::Call("hear".into(), vec![]), j));
        }
        if let Some(j) = self.seq(i, &["calling"]) {
            for (n, k) in self.name(j, false, false) {
                out.push((Expr::Call(n.clone(), vec![]), k));
                if self.word(k, "with") {
                    for (args, l) in self.args(k + 1) {
                        out.push((Expr::Call(n.clone(), args), l));
                    }
                }
            }
        }
        if let Some(j) = self.seq(i, &["item"]) {
            for (idx, k) in self.operand(j) {
                if self.word(k, "of") {
                    for (list, l) in self.operand(k + 1) {
                        out.push((Expr::Index(Box::new(list), Box::new(idx.clone())), l));
                    }
                }
            }
        }
        if let Some(j) = self.seq(i, &["a", "list", "of"]) {
            out.extend(self.args(j).into_iter().map(|(v, k)| (Expr::List(v), k)));
        }
        if let Some(j) = self.seq(i, &["an", "empty", "list", "of"]) {
            out.extend(
                self.typ(j)
                    .into_iter()
                    .map(|(t, k)| (Expr::EmptyList(t), k)),
            );
        }
        out.extend(
            self.name(i, true, false)
                .into_iter()
                .map(|(n, j)| (Expr::Var(n), j)),
        );
        out
    }

    fn prefix(&self, i: usize) -> R<Expr> {
        let mut out = Vec::new();
        let arith: [(&[&str], BinOp); 5] = [
            (&["the", "sum", "of"], BinOp::Add),
            (&["the", "difference", "of"], BinOp::Sub),
            (&["the", "product", "of"], BinOp::Mul),
            (&["the", "quotient", "of"], BinOp::Div),
            (&["the", "remainder", "of"], BinOp::Mod),
        ];
        for (ws, op) in arith {
            if let Some(j) = self.seq(i, ws) {
                self.pair(j, "and", op, &|k| self.arith(k), &mut out);
            }
        }
        let convert: [(&[&str], &str); 4] = [
            (&["the", "text", "of"], "to_string"),
            (&["the", "whole", "number", "of"], "to_integer"),
            (&["the", "fraction", "of"], "to_float"),
            (&["the", "length", "of"], "length"),
        ];
        for (ws, f) in convert {
            if let Some(j) = self.seq(i, ws) {
                out.extend(
                    self.operand(j)
                        .into_iter()
                        .map(|(e, k)| (Expr::Call(f.into(), vec![e]), k)),
                );
            }
        }
        if let Some(j) = self.seq(i, &["the", "negation", "of"]) {
            out.extend(
                self.operand(j)
                    .into_iter()
                    .map(|(e, k)| (Expr::Unary(UnOp::Neg, Box::new(e)), k)),
            );
        }
        if let Some(j) = self.seq(i, &["not"]) {
            out.extend(
                self.operand(j)
                    .into_iter()
                    .map(|(e, k)| (Expr::Unary(UnOp::Not, Box::new(e)), k)),
            );
        }
        out
    }

    /// `A sep A` where each side is parsed by `arg` (arity-driven; no precedence).
    fn pair(
        &self,
        j: usize,
        sep: &str,
        op: BinOp,
        arg: &dyn Fn(usize) -> R<Expr>,
        out: &mut R<Expr>,
    ) {
        for (a, k) in arg(j) {
            if self.word(k, sep) {
                for (b, l) in arg(k + 1) {
                    out.push((bin(op, a.clone(), b), l));
                }
            }
        }
    }

    /// Atom, prefix form, or `base with E appended` (`spec/PROSE_GRAMMAR.md` §7.2).
    fn operand(&self, i: usize) -> R<Expr> {
        let mut base = self.atom(i);
        base.extend(self.prefix(i));
        let mut out = base.clone();
        let appendable = |e: &Expr| match e {
            Expr::Var(_) => true,
            Expr::Call(n, _) => !CONVERSIONS.contains(&n.as_str()),
            _ => false,
        };
        let mut cur: R<Expr> = base.into_iter().filter(|(e, _)| appendable(e)).collect();
        while !cur.is_empty() {
            let mut next = Vec::new();
            for (list, j) in &cur {
                if !self.word(*j, "with") {
                    continue;
                }
                let mut elems = self.atom(j + 1);
                elems.extend(self.prefix(j + 1));
                for (x, k) in elems {
                    if self.word(k, "appended") {
                        let call = Expr::Call("append".into(), vec![list.clone(), x]);
                        next.push((call, k + 1));
                    }
                }
            }
            out.extend(next.iter().cloned());
            cur = next;
        }
        out
    }

    /// An operand, or `operand infix-word operand` (no chaining). The arguments of the
    /// arithmetic prefix forms use this, so a comparison can never be swallowed by them.
    fn arith(&self, i: usize) -> R<Expr> {
        let mut out = self.operand(i);
        let infix: [(&str, BinOp); 5] = [
            ("plus", BinOp::Add),
            ("minus", BinOp::Sub),
            ("times", BinOp::Mul),
            ("over", BinOp::Div),
            ("modulo", BinOp::Mod),
        ];
        for (a, j) in self.operand(i) {
            for (w, op) in infix {
                if self.word(j, w) {
                    for (b, k) in self.operand(j + 1) {
                        out.push((bin(op, a.clone(), b), k));
                    }
                }
            }
        }
        out
    }

    /// Full expression: arithmetic, a comparison, or `both`/`either` over full expressions.
    /// `both`/`either` live here, not at operand level, so their result is never the left
    /// side of a comparison (`both a and b is c` has exactly one reading).
    pub(crate) fn expr(&self, i: usize) -> R<Expr> {
        let mut out = self.arith(i);
        let cmp: [(&[&str], BinOp); 6] = [
            (&["is", "not"], BinOp::Ne),
            (&["is", "less", "than"], BinOp::Lt),
            (&["is", "greater", "than"], BinOp::Gt),
            (&["is", "at", "most"], BinOp::Le),
            (&["is", "at", "least"], BinOp::Ge),
            (&["is"], BinOp::Eq),
        ];
        for (a, j) in self.operand(i) {
            for (ws, op) in cmp {
                if let Some(k) = self.seq(j, ws) {
                    for (b, l) in self.operand(k) {
                        // `a is not b` is Ne only; never Eq(a, Not(b)).
                        if op == BinOp::Eq && matches!(b, Expr::Unary(UnOp::Not, _)) {
                            continue;
                        }
                        out.push((bin(op, a.clone(), b), l));
                    }
                }
            }
        }
        if let Some(j) = self.seq(i, &["both"]) {
            self.pair(j, "and", BinOp::And, &|k| self.expr(k), &mut out);
        }
        if let Some(j) = self.seq(i, &["either"]) {
            self.pair(j, "or", BinOp::Or, &|k| self.expr(k), &mut out);
        }
        out
    }

    // ---- statements --------------------------------------------------------------------

    fn target(&self, i: usize) -> R<Target> {
        let mut out: R<Target> = self
            .name(i, false, true)
            .into_iter()
            .map(|(n, j)| (Target::Name(n), j))
            .collect();
        if let Some(j) = self.seq(i, &["item"]) {
            for (idx, k) in self.operand(j) {
                if self.word(k, "of") {
                    for (n, l) in self.name(k + 1, false, true) {
                        out.push((Target::Index(n, idx.clone()), l));
                    }
                }
            }
        }
        out
    }

    /// Optional apposition `, a T,`.
    fn annot(&self, j: usize) -> R<Option<Type>> {
        let mut out = vec![(None, j)];
        if self.punct(j, ',') && (self.word(j + 1, "a") || self.word(j + 1, "an")) {
            for (t, k) in self.typ(j + 2) {
                if self.punct(k, ',') {
                    out.push((Some(t), k + 1));
                }
            }
        }
        out
    }

    fn verb(&self, j: usize) -> Option<(bool, usize)> {
        if self.word(j, "be") {
            Some((false, j + 1))
        } else if self.word(j, "become") {
            Some((true, j + 1))
        } else {
            None
        }
    }

    /// All parses of one whole simple sentence (terminator already removed).
    pub(crate) fn statement(&self) -> Vec<StmtKind> {
        let mut out = self.declarations();
        out.extend(self.assignments());
        out.extend(self.actions());
        out
    }

    /// Declarations D1–D3 (`spec/PROSE_GRAMMAR.md` §6.1).
    fn declarations(&self) -> Vec<StmtKind> {
        let n = self.t.len();
        let mut out = Vec::new();
        let decl = |name: &Name, ty: &Option<Type>, mutable, init: Expr| StmtKind::Decl {
            name: name.clone(),
            ty: ty.clone(),
            mutable,
            init,
        };
        // D1: N [", a T,"] shall V E
        for (name, j) in self.name(0, false, true) {
            for (ty, k) in self.annot(j) {
                if self.word(k, "shall")
                    && let Some((m, l)) = self.verb(k + 1)
                {
                    out.extend(
                        self.expr(l)
                            .into_iter()
                            .filter(|(_, r)| *r == n)
                            .map(|(e, _)| decl(&name, &ty, m, e)),
                    );
                }
            }
        }
        // D2: Let N [", a T,"] V E
        if self.word(0, "let") {
            for (name, j) in self.name(1, false, true) {
                for (ty, k) in self.annot(j) {
                    if let Some((m, l)) = self.verb(k) {
                        out.extend(
                            self.expr(l)
                                .into_iter()
                                .filter(|(_, r)| *r == n)
                                .map(|(e, _)| decl(&name, &ty, m, e)),
                        );
                    }
                }
            }
        }
        // D3: E shall N [", a T,"] V
        for (e, j) in self.expr(0) {
            if !self.word(j, "shall") {
                continue;
            }
            for (name, k) in self.name(j + 1, false, true) {
                for (ty, l) in self.annot(k) {
                    if let Some((m, r)) = self.verb(l)
                        && r == n
                    {
                        out.push(decl(&name, &ty, m, e.clone()));
                    }
                }
            }
        }
        out
    }

    /// Assignments A1–A3 (`spec/PROSE_GRAMMAR.md` §6.2); `now` is the marker.
    fn assignments(&self) -> Vec<StmtKind> {
        let mut out = Vec::new();
        for (x, j) in self.target(0) {
            if let Some(k) = self.seq(j, &["shall", "now", "be"]) {
                self.finish_assign(&x, k, &mut out);
            }
        }
        if self.word(0, "let") {
            for (x, j) in self.target(1) {
                if let Some(k) = self.seq(j, &["now", "be"]) {
                    self.finish_assign(&x, k, &mut out);
                }
            }
        }
        for (e, j) in self.expr(0) {
            if !self.word(j, "shall") {
                continue;
            }
            for (x, k) in self.target(j + 1) {
                if self.seq(k, &["now", "be"]) == Some(self.t.len()) {
                    out.push(StmtKind::Assign {
                        target: x,
                        value: e.clone(),
                    });
                }
            }
        }
        out
    }

    /// Output, call and return statements (`spec/PROSE_GRAMMAR.md` §6.3–§6.5).
    fn actions(&self) -> Vec<StmtKind> {
        let n = self.t.len();
        let mut out = Vec::new();
        for (w, f) in [("speak", "speak"), ("proclaim", "speakln")] {
            if self.word(0, w) {
                for (e, j) in self.expr(1) {
                    if j == n {
                        out.push(StmtKind::Expr(Expr::Call(f.into(), vec![e])));
                    }
                }
            }
        }
        if self.word(0, "call") {
            for (name, j) in self.name(1, false, false) {
                if j == n {
                    out.push(StmtKind::Expr(Expr::Call(name.clone(), vec![])));
                }
                if self.word(j, "with") {
                    for (args, k) in self.args(j + 1) {
                        if k == n {
                            out.push(StmtKind::Expr(Expr::Call(name.clone(), args)));
                        }
                    }
                }
            }
        }
        for lead in [&["give", "back"][..], &["return"][..]] {
            if let Some(j) = self.seq(0, lead) {
                if self.seq(j, &["nothing"]) == Some(n) {
                    out.push(StmtKind::Return(None));
                }
                for (e, k) in self.expr(j) {
                    if k == n {
                        out.push(StmtKind::Return(Some(e)));
                    }
                }
            }
        }
        out
    }

    fn finish_assign(&self, x: &Target, k: usize, out: &mut Vec<StmtKind>) {
        for (e, r) in self.expr(k) {
            if r == self.t.len() {
                out.push(StmtKind::Assign {
                    target: x.clone(),
                    value: e,
                });
            }
        }
    }

    /// `If E, then` / `As long as E, do` / `Do this E times` (terminator `:` removed).
    pub(crate) fn block_header(&self) -> Vec<Open> {
        let mut out = Vec::new();
        let n = self.t.len();
        if self.word(0, "if") {
            for (e, j) in self.expr(1) {
                if self.punct(j, ',') && self.seq(j + 1, &["then"]) == Some(n) {
                    out.push(Open::If(e));
                }
            }
        }
        if let Some(i) = self.seq(0, &["as", "long", "as"]) {
            for (e, j) in self.expr(i) {
                if self.punct(j, ',') && self.seq(j + 1, &["do"]) == Some(n) {
                    out.push(Open::While(e));
                }
            }
        }
        if let Some(i) = self.seq(0, &["do", "this"]) {
            for (e, j) in self.expr(i) {
                if self.seq(j, &["times"]) == Some(n) {
                    out.push(Open::Repeat(e));
                }
            }
        }
        out
    }

    // ---- stanza headers ----------------------------------------------------------------

    pub(crate) fn entry_header(&self) -> Vec<Name> {
        let Some(i) = self.seq(0, &["thus", "begins"]) else {
            return vec![];
        };
        self.name(i, false, true)
            .into_iter()
            .filter(|(_, j)| *j == self.t.len())
            .map(|(n, _)| n)
            .collect()
    }

    fn param(&self, i: usize) -> R<Param> {
        let mut out = Vec::new();
        for (name, j) in self.name(i, false, true) {
            if self.word(j, "as") {
                out.extend(self.typ(j + 1).into_iter().map(|(ty, k)| {
                    (
                        Param {
                            name: name.clone(),
                            ty,
                        },
                        k,
                    )
                }));
            }
        }
        out
    }

    pub(crate) fn function_header(&self) -> Vec<(Name, Vec<Param>, Type)> {
        let mut out = Vec::new();
        if !self.word(0, "to") {
            return out;
        }
        for (name, j) in self.name(1, false, true) {
            let mut with_params: R<Vec<Param>> = vec![(vec![], j)];
            if self.punct(j, ',') && self.word(j + 1, "taking") {
                with_params.extend(self.list_of(j + 2, &|k| self.param(k)));
            }
            for (params, k) in with_params {
                let mut rets: R<Type> = vec![(Type::Unit, k)];
                if self.punct(k, ',') && self.word(k + 1, "giving") {
                    rets.extend(self.typ(k + 2));
                }
                for (ret, l) in rets {
                    if l == self.t.len() {
                        out.push((name.clone(), params.clone(), ret));
                    }
                }
            }
        }
        out
    }
}
