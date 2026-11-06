//! Where names are defined and where they are used, found by walking the program with the
//! scopes the checker uses (`spec/SEMANTIC_CORE.md` §5). The AST records the span of whole
//! expressions, so the span of a name is found inside them by its words.

use std::collections::HashMap;

use syntaxeon_core::{Expr, ExprKind, Program, Span, Stmt, StmtKind, Target};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefKind {
    Function,
    Param,
    Variable,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub span: Span,
    pub kind: DefKind,
    /// Index into [`Index::stanzas`].
    pub stanza: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Use {
    pub span: Span,
    /// Index into [`Index::defs`].
    pub def: usize,
}

#[derive(Default)]
pub struct Index {
    pub defs: Vec<Def>,
    pub uses: Vec<Use>,
    /// The extent of each stanza: from its header to the next header.
    pub stanzas: Vec<Span>,
}

impl Index {
    /// The name written at `offset`: its definition (an index into `defs`) and where this
    /// mention is.
    #[must_use]
    pub fn at(&self, offset: usize) -> Option<(usize, Span)> {
        let within = |s: Span| s.start <= offset && offset <= s.end;
        self.uses
            .iter()
            .find(|u| within(u.span))
            .map(|u| (u.def, u.span))
            .or_else(|| {
                let i = self.defs.iter().position(|d| within(d.span))?;
                Some((i, self.defs[i].span))
            })
    }

    /// The definition the name at `offset` refers to (or is).
    #[must_use]
    pub fn definition_at(&self, offset: usize) -> Option<&Def> {
        self.at(offset).map(|(d, _)| &self.defs[d])
    }

    /// Every place `def` is written: its definition first, then its uses.
    #[must_use]
    pub fn mentions(&self, def: usize) -> Vec<Span> {
        std::iter::once(self.defs[def].span)
            .chain(self.uses.iter().filter(|u| u.def == def).map(|u| u.span))
            .collect()
    }

    /// The stanza that contains `offset`.
    #[must_use]
    pub fn stanza_at(&self, offset: usize) -> Option<usize> {
        self.stanzas
            .iter()
            .position(|s| s.start <= offset && offset <= s.end)
    }
}

/// Words of `text[within]` with their byte ranges, lowercased.
fn words(text: &str, within: Span) -> Vec<(Span, String)> {
    let Some(slice) = text.get(within.start..within.end) else {
        return Vec::new();
    };
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '-';
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in slice.char_indices().chain([(slice.len(), ' ')]) {
        match (is_word(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((
                    Span {
                        start: within.start + s,
                        end: within.start + i,
                    },
                    slice[s..i].to_lowercase(),
                ));
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// The first (or last) place in `within` where the words of `name` are written in a row.
fn find(text: &str, within: Span, name: &str, last: bool) -> Option<Span> {
    let want: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
    let have = words(text, within);
    if want.is_empty() || have.len() < want.len() {
        return None;
    }
    let mut hits = (0..=have.len() - want.len()).filter(|&i| {
        have[i..i + want.len()]
            .iter()
            .map(|(_, w)| w)
            .eq(want.iter())
    });
    let i = if last { hits.next_back() } else { hits.next() }?;
    Some(Span {
        start: have[i].0.start,
        end: have[i + want.len() - 1].0.end,
    })
}

struct Builder<'a> {
    text: &'a str,
    index: Index,
    functions: HashMap<String, usize>,
    scopes: Vec<HashMap<String, usize>>,
    stanza: usize,
}

impl Builder<'_> {
    fn define(&mut self, name: &str, span: Span, kind: DefKind) -> usize {
        self.index.defs.push(Def {
            name: name.to_string(),
            span,
            kind,
            stanza: self.stanza,
        });
        self.index.defs.len() - 1
    }

    fn mention(&mut self, name: &str, within: Span, last: bool, function: bool) {
        if within.is_empty() {
            return;
        }
        let def = if function {
            self.functions.get(name).copied()
        } else {
            self.scopes.iter().rev().find_map(|s| s.get(name).copied())
        };
        if let (Some(def), Some(span)) = (def, find(self.text, within, name, last)) {
            self.index.uses.push(Use { span, def });
        }
    }

    fn scoped(&mut self, body: &[Stmt]) {
        self.scopes.push(HashMap::new());
        for s in body {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Decl { name, init, .. } => {
                self.expr(init);
                if !s.focus.is_empty() {
                    let at = find(self.text, s.focus, name, false).unwrap_or(s.focus);
                    let id = self.define(name, at, DefKind::Variable);
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert(name.clone(), id);
                    }
                }
            }
            StmtKind::Assign { target, value } => {
                self.expr(value);
                match target {
                    Target::Name(n) => self.mention(n, s.focus, false, false),
                    Target::Index(n, i) => {
                        self.mention(n, s.focus, false, false);
                        self.expr(i);
                    }
                }
            }
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond);
                self.scoped(then);
                if let Some(o) = otherwise {
                    self.scoped(o);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.scoped(body);
            }
            StmtKind::Repeat { count, body } => {
                self.expr(count);
                self.scoped(body);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            StmtKind::Expr(e) => self.expr(e),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Var(n) => self.mention(n, e.span, true, false),
            ExprKind::Call(n, args) => {
                self.mention(n, e.span, false, true);
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::Unary(_, x) => self.expr(x),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::List(items) => {
                for i in items {
                    self.expr(i);
                }
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Char(_)
            | ExprKind::EmptyList(_) => {}
        }
    }
}

#[must_use]
pub fn build(text: &str, program: &Program) -> Index {
    let mut b = Builder {
        text,
        index: Index::default(),
        functions: HashMap::new(),
        scopes: Vec::new(),
        stanza: 0,
    };
    for (i, st) in program.stanzas.iter().enumerate() {
        let end = program
            .stanzas
            .get(i + 1)
            .map_or(text.len(), |n| n.span.start);
        b.index.stanzas.push(Span {
            start: st.span.start,
            end,
        });
    }
    // Functions first: a call may come before the stanza that defines it.
    for (i, st) in program.stanzas.iter().enumerate() {
        b.stanza = i;
        if let Some(at) = find(text, st.span, &st.title, false) {
            let id = b.define(&st.title, at, DefKind::Function);
            b.functions.insert(st.title.clone(), id);
        }
    }
    for (i, st) in program.stanzas.iter().enumerate() {
        b.stanza = i;
        // Parameters follow the title in the header.
        let after_title = Span {
            start: b
                .functions
                .get(&st.title)
                .map_or(st.span.start, |&d| b.index.defs[d].span.end),
            end: st.span.end,
        };
        let mut scope = HashMap::new();
        for p in &st.params {
            if let Some(at) = find(text, after_title, &p.name, false) {
                let id = b.define(&p.name, at, DefKind::Param);
                scope.insert(p.name.clone(), id);
            }
        }
        // Parameters and the top level of the body share one scope.
        b.scopes = vec![scope];
        for s in &st.body {
            b.stmt(s);
        }
    }
    b.index
}
