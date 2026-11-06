//! Sentence-level grammar (`spec/PROSE_GRAMMAR.md` §4–§7). Every function returns all parses.

use crate::rules::{
    COMPARE, CONVERSIONS, DETS, FORMS, HEADERS, HeaderKind, INFIX, JOIN, Make, PREFIX_ARITH,
    PREFIX_CALL, PREFIX_UNARY, RESERVED, SIMPLE_TYPES, SPEAK, Step, THE_HEADS, number_word,
};
use std::cell::RefCell;
use std::collections::HashMap;

use syntaxeon_core::{BinOp, Expr, ExprKind, Name, Param, Span, StmtKind, Target, Type, UnOp};
use syntaxeon_lex::lexer::Tok;

/// Parts collected while matching a [`Form`](crate::rules::Form).
#[derive(Clone, Default)]
struct Slots {
    name: Option<Name>,
    target: Option<Target>,
    ty: Option<Type>,
    mutable: bool,
    expr: Option<Expr>,
    /// Token range `(start, end)` each step consumed, in step order.
    spans: Vec<(usize, usize)>,
}

/// All ways to parse a prefix: each result is a value and the index just past it.
type R<T> = Vec<(T, usize)>;

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}

/// Where a name appears, which decides what it may contain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NameKind {
    /// A stanza title or a call target: may contain `of` (`the tale of men`).
    Title,
    /// A declared name, parameter or assignment target: no `of`, which would make
    /// `item i of the pile of xs` unreadable (`spec/PROSE_GRAMMAR.md` §9).
    Binding,
    /// A variable used in an expression: no `of`, and not a bare number-word.
    Var,
}

/// Whether `e` ends inside an unbounded construct that would swallow whatever comes next: a
/// list literal, or a call with arguments (`spec/PROSE_GRAMMAR.md` §7.4). A comma or an
/// infix/comparison word after such an expression belongs to the construct, never to what
/// contains it, so every sentence keeps one reading.
fn open_tail(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::List(_) => true,
        ExprKind::Call(n, args) if CONVERSIONS.contains(&n.as_str()) => {
            args.last().is_some_and(open_tail)
        }
        ExprKind::Call(n, args) => n != "append" && !args.is_empty(),
        ExprKind::Unary(_, a) | ExprKind::Index(a, _) => open_tail(a),
        ExprKind::Binary(_, _, b) => open_tail(b),
        _ => false,
    }
}

/// How to parse one list element and whether an element is open (see [`open_tail`]).
struct ListOps<'a, T> {
    item: &'a dyn Fn(usize) -> R<T>,
    open: &'a dyn Fn(&T) -> bool,
}

pub(crate) enum Open {
    If(Expr),
    While(Expr),
    Repeat(Expr),
}

/// Memo tables for the three recursive expression levels, keyed by start token. Without them
/// every level re-parses its operands several times and nested calls take exponential time.
#[derive(Default)]
struct Memo {
    operand: HashMap<usize, R<Expr>>,
    arith: HashMap<usize, R<Expr>>,
    expr: HashMap<usize, R<Expr>>,
}

pub(crate) struct P<'a> {
    t: &'a [Tok],
    /// Source span of each token in `t`.
    sp: &'a [Span],
    memo: RefCell<Memo>,
}

impl<'a> P<'a> {
    pub(crate) fn new(t: &'a [Tok], sp: &'a [Span]) -> Self {
        P {
            t,
            sp,
            memo: RefCell::new(Memo::default()),
        }
    }

    /// The source text covered by tokens `i..j` (empty when there are none).
    fn span(&self, i: usize, j: usize) -> Span {
        match (
            self.sp.get(i),
            j.checked_sub(1).and_then(|k| self.sp.get(k)),
        ) {
            (Some(a), Some(b)) if j > i => Span {
                start: a.start,
                end: b.end,
            },
            _ => Span::default(),
        }
    }
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
    /// `kind` decides which words may appear inside (see [`NameKind`]).
    fn name(&self, i: usize, kind: NameKind, quoted: bool) -> R<Name> {
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
            if RESERVED.contains(&w.as_str()) || (kind != NameKind::Title && w == "of") {
                break;
            }
            words.push(w);
            if kind == NameKind::Var && words.len() == 1 && number_word(w).is_some() {
                continue;
            }
            out.push((words.join(" "), start + words.len()));
        }
        out
    }

    fn typ(&self, i: usize) -> R<Type> {
        let mut out = Vec::new();
        for (ws, ty) in SIMPLE_TYPES {
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
    fn list_of<T: Clone>(
        &self,
        i: usize,
        f: &dyn Fn(usize) -> R<T>,
        open: &dyn Fn(&T) -> bool,
    ) -> R<Vec<T>> {
        let ops = ListOps { item: f, open };
        let mut out = Vec::new();
        for (x, j) in f(i) {
            self.list_tail(j, &[x], &ops, &mut out);
        }
        out
    }

    fn list_tail<T: Clone>(&self, j: usize, acc: &[T], ops: &ListOps<T>, out: &mut R<Vec<T>>) {
        out.push((acc.to_vec(), j));
        // A comma after an open element belongs to that element (innermost list wins).
        if !self.punct(j, ',') || acc.last().is_some_and(ops.open) {
            return;
        }
        for (x, k) in (ops.item)(j + 1) {
            let mut a = acc.to_vec();
            a.push(x);
            self.list_tail(k, &a, ops, out);
        }
        if self.word(j + 1, "and") {
            for (x, k) in (ops.item)(j + 2) {
                let mut a = acc.to_vec();
                a.push(x);
                out.push((a, k));
            }
        }
    }

    // ---- expressions -------------------------------------------------------------------

    fn args(&self, i: usize) -> R<Vec<Expr>> {
        self.list_of(i, &|k| self.expr(k), &open_tail)
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
            for (n, k) in self.name(j, NameKind::Title, false) {
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
            self.name(i, NameKind::Var, false)
                .into_iter()
                .map(|(n, j)| (Expr::Var(n), j)),
        );
        out
    }

    fn prefix(&self, i: usize) -> R<Expr> {
        let mut out = Vec::new();
        for (ws, op) in PREFIX_ARITH {
            if let Some(j) = self.seq(i, ws) {
                self.pair(j, "and", op, &|k| self.operand(k), &mut out);
            }
        }
        for (ws, f) in PREFIX_CALL {
            if let Some(j) = self.seq(i, ws) {
                out.extend(
                    self.operand(j)
                        .into_iter()
                        .map(|(e, k)| (Expr::Call(f.into(), vec![e]), k)),
                );
            }
        }
        for (ws, op) in PREFIX_UNARY {
            if let Some(j) = self.seq(i, ws) {
                out.extend(
                    self.operand(j)
                        .into_iter()
                        .map(|(e, k)| (Expr::Unary(op, Box::new(e)), k)),
                );
            }
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
        if let Some(hit) = self.memo.borrow().operand.get(&i) {
            return hit.clone();
        }
        let mut out = self.operand_uncached(i);
        for (e, j) in &mut out {
            e.span = self.span(i, *j);
        }
        self.memo.borrow_mut().operand.insert(i, out.clone());
        out
    }

    fn operand_uncached(&self, i: usize) -> R<Expr> {
        let mut base = self.atom(i);
        base.extend(self.prefix(i));
        let mut out = base.clone();
        let appendable = |e: &Expr| match &e.kind {
            ExprKind::Var(_) => true,
            ExprKind::Call(n, _) => !CONVERSIONS.contains(&n.as_str()),
            _ => false,
        };
        let mut cur: R<Expr> = base
            .into_iter()
            .filter(|(e, _)| appendable(e) && !open_tail(e))
            .collect();
        while !cur.is_empty() {
            let mut next = Vec::new();
            for (list, j) in &cur {
                if !self.word(*j, "with") {
                    continue;
                }
                let mut elems = self.atom(j + 1);
                elems.extend(self.prefix(j + 1));
                for (mut x, k) in elems {
                    x.span = self.span(j + 1, k);
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

    /// An operand, or `operand infix-word operand` (no chaining).
    fn arith(&self, i: usize) -> R<Expr> {
        if let Some(hit) = self.memo.borrow().arith.get(&i) {
            return hit.clone();
        }
        let mut out = self.arith_uncached(i);
        for (e, j) in &mut out {
            e.span = self.span(i, *j);
        }
        self.memo.borrow_mut().arith.insert(i, out.clone());
        out
    }

    fn arith_uncached(&self, i: usize) -> R<Expr> {
        let mut out = self.operand(i);
        for (a, j) in self.operand(i).into_iter().filter(|(a, _)| !open_tail(a)) {
            for (w, op) in INFIX {
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
        if let Some(hit) = self.memo.borrow().expr.get(&i) {
            return hit.clone();
        }
        let mut out = self.expr_uncached(i);
        for (e, j) in &mut out {
            e.span = self.span(i, *j);
        }
        self.memo.borrow_mut().expr.insert(i, out.clone());
        out
    }

    fn expr_uncached(&self, i: usize) -> R<Expr> {
        let mut out = self.arith(i);
        for (a, j) in self.operand(i).into_iter().filter(|(a, _)| !open_tail(a)) {
            for (ws, op) in COMPARE {
                if let Some(k) = self.seq(j, ws) {
                    for (b, l) in self.operand(k) {
                        // `a is not b` is Ne only; never Eq(a, Not(b)).
                        if op == BinOp::Eq && matches!(b.kind, ExprKind::Unary(UnOp::Not, _)) {
                            continue;
                        }
                        out.push((bin(op, a.clone(), b), l));
                    }
                }
            }
        }
        for (opener, sep, op) in JOIN {
            if let Some(j) = self.seq(i, &[opener]) {
                self.pair(j, sep, op, &|k| self.expr(k), &mut out);
            }
        }
        out
    }

    // ---- statements --------------------------------------------------------------------

    fn target(&self, i: usize) -> R<Target> {
        let mut out: R<Target> = self
            .name(i, NameKind::Binding, true)
            .into_iter()
            .map(|(n, j)| (Target::Name(n), j))
            .collect();
        if let Some(j) = self.seq(i, &["item"]) {
            for (idx, k) in self.operand(j) {
                if self.word(k, "of") {
                    for (n, l) in self.name(k + 1, NameKind::Binding, true) {
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
        let mut out = self.forms();
        out.extend(self.actions());
        out
    }

    /// Declarations and assignments, read from the [`FORMS`] table.
    fn forms(&self) -> Vec<StmtKind> {
        let mut out = Vec::new();
        for form in FORMS {
            let mut done = Vec::new();
            self.run_steps(form.steps, 0, Slots::default(), &mut done);
            out.extend(done.into_iter().filter_map(|s| match form.make {
                Make::Decl => Some(StmtKind::Decl {
                    name: s.name?,
                    ty: s.ty,
                    mutable: s.mutable,
                    init: s.expr?,
                }),
                Make::Assign => Some(StmtKind::Assign {
                    target: s.target?,
                    value: s.expr?,
                }),
                Make::Return => Some(StmtKind::Return(Some(s.expr?))),
                Make::Nothing => Some(StmtKind::Return(None)),
            }));
        }
        out
    }

    /// Matches `steps` from token `i`; every complete match must end at the last token.
    fn run_steps(&self, steps: &[Step], i: usize, slots: Slots, out: &mut Vec<Slots>) {
        let Some((step, rest)) = steps.split_first() else {
            if i == self.t.len() {
                out.push(slots);
            }
            return;
        };
        let mut go = |j: usize, mut s: Slots| {
            s.spans.push((i, j));
            self.run_steps(rest, j, s, out);
        };
        match step {
            Step::Words(ws) => {
                if let Some(j) = self.seq(i, ws) {
                    go(j, slots);
                }
            }
            Step::Name => {
                for (name, j) in self.name(i, NameKind::Binding, true) {
                    go(
                        j,
                        Slots {
                            name: Some(name),
                            ..slots.clone()
                        },
                    );
                }
            }
            Step::Target => {
                for (target, j) in self.target(i) {
                    go(
                        j,
                        Slots {
                            target: Some(target),
                            ..slots.clone()
                        },
                    );
                }
            }
            Step::Annot => {
                for (ty, j) in self.annot(i) {
                    go(
                        j,
                        Slots {
                            ty,
                            ..slots.clone()
                        },
                    );
                }
            }
            Step::Verb => {
                if let Some((mutable, j)) = self.verb(i) {
                    go(j, Slots { mutable, ..slots });
                }
            }
            Step::Expr => {
                for (e, j) in self.expr(i) {
                    go(
                        j,
                        Slots {
                            expr: Some(e),
                            ..slots.clone()
                        },
                    );
                }
            }
        }
    }

    /// Every form that reads the whole sentence, as `(form index, token ranges per step)`.
    pub(crate) fn form_matches(&self) -> Vec<(usize, Vec<(usize, usize)>)> {
        let mut out = Vec::new();
        for (k, form) in FORMS.iter().enumerate() {
            let mut done = Vec::new();
            self.run_steps(form.steps, 0, Slots::default(), &mut done);
            out.extend(done.into_iter().map(|s| (k, s.spans)));
        }
        out
    }

    /// Output, call and return statements (`spec/PROSE_GRAMMAR.md` §6.3–§6.5).
    fn actions(&self) -> Vec<StmtKind> {
        let n = self.t.len();
        let mut out = Vec::new();
        for &(w, f) in SPEAK {
            if self.word(0, w) {
                for (e, j) in self.expr(1) {
                    if j == n {
                        out.push(StmtKind::Expr(Expr::Call(f.into(), vec![e])));
                    }
                }
            }
        }
        if self.word(0, "call") {
            for (name, j) in self.name(1, NameKind::Title, false) {
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
        out
    }

    /// `If E, then` / `As long as E, do` / `Do this E times` (terminator `:` removed).
    pub(crate) fn block_header(&self) -> Vec<Open> {
        let mut out = Vec::new();
        for h in HEADERS {
            let Some(i) = self.seq(0, h.lead) else {
                continue;
            };
            for (e, j) in self.expr(i) {
                if self.tail(j, h.close) {
                    out.push(match h.kind {
                        HeaderKind::If => Open::If(e),
                        HeaderKind::While => Open::While(e),
                        HeaderKind::Repeat => Open::Repeat(e),
                    });
                }
            }
        }
        out
    }

    /// Whether `close` (words, with `","` for a comma) fills the rest of the sentence from `j`.
    fn tail(&self, j: usize, close: &[&str]) -> bool {
        j + close.len() == self.t.len()
            && close.iter().enumerate().all(|(k, w)| {
                if *w == "," {
                    self.punct(j + k, ',')
                } else {
                    self.word(j + k, w)
                }
            })
    }

    // ---- stanza headers ----------------------------------------------------------------

    pub(crate) fn entry_header(&self) -> Vec<Name> {
        let Some(i) = self.seq(0, &["thus", "begins"]) else {
            return vec![];
        };
        self.name(i, NameKind::Title, true)
            .into_iter()
            .filter(|(_, j)| *j == self.t.len())
            .map(|(n, _)| n)
            .collect()
    }

    fn param(&self, i: usize) -> R<Param> {
        let mut out = Vec::new();
        for (name, j) in self.name(i, NameKind::Binding, true) {
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
        for (name, j) in self.name(1, NameKind::Title, true) {
            let mut with_params: R<Vec<Param>> = vec![(vec![], j)];
            if self.punct(j, ',') && self.word(j + 1, "taking") {
                with_params.extend(self.list_of(j + 2, &|k| self.param(k), &|_| false));
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
