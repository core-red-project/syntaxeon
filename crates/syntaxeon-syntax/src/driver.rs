//! Stanza and block driver (`spec/PROSE_GRAMMAR.md` §3, §6.6, §8, §10).

use syntaxeon_core::{Expr, Program, Stanza, Stmt, StmtKind, Type};
use syntaxeon_lex::Diagnostic;
use syntaxeon_lex::lexer::{Lexed, Tok};
use syntaxeon_lex::literary::Stanza as Verses;

use crate::grammar::{Open, P};

struct Sentence {
    toks: Vec<Tok>,
    term: char,
    line: usize,
}

fn diag(code: &'static str, message: &str, line: usize) -> Diagnostic {
    Diagnostic {
        code,
        message: message.to_string(),
        line,
    }
}

fn dedup<T: PartialEq>(v: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for x in v {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

fn split_sentences(lexed: &Lexed, st: &Verses, diags: &mut Vec<Diagnostic>) -> Vec<Sentence> {
    let (Some(first), Some(last)) = (st.verses.first(), st.verses.last()) else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut line = 0;
    for t in &lexed.tokens[first.tokens.start..last.tokens.end] {
        if cur.is_empty() {
            line = t.line;
        }
        match t.tok {
            Tok::Punct(c @ ('.' | '!' | '?' | ':')) => {
                out.push(Sentence {
                    toks: std::mem::take(&mut cur),
                    term: c,
                    line,
                });
            }
            ref other => cur.push(other.clone()),
        }
    }
    if !cur.is_empty() {
        diags.push(diag("SXP-103", "sentence is not terminated", line));
    }
    out
}

/// Parses every stanza. Errors are collected; a stanza with a bad header is skipped.
#[must_use]
pub fn parse_program(lexed: &Lexed, stanzas: &[Verses]) -> (Program, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut out = Vec::new();
    for (idx, st) in stanzas.iter().enumerate() {
        let sentences = split_sentences(lexed, st, &mut diags);
        if let Some(s) = parse_stanza(idx, &sentences, &mut diags) {
            out.push(s);
        }
    }
    (Program { stanzas: out }, diags)
}

fn parse_stanza(idx: usize, sentences: &[Sentence], diags: &mut Vec<Diagnostic>) -> Option<Stanza> {
    let first = sentences.first()?;
    if first.term != ':' {
        diags.push(diag(
            "SXP-109",
            "a stanza must begin with a header ending in ':'",
            first.line,
        ));
        return None;
    }
    let p = P { t: &first.toks };
    let entry = dedup(p.entry_header());
    let funcs = dedup(p.function_header());
    let (title, params, ret) = match (idx, entry.len(), funcs.len()) {
        (0, 1, _) => (entry[0].clone(), vec![], Type::Unit),
        (_, 0, 1) if idx > 0 => funcs[0].clone(),
        (_, e, f) if e + f > 1 => {
            diags.push(diag("SXP-104", "ambiguous stanza header", first.line));
            return None;
        }
        (0, 0, _) => {
            diags.push(diag(
                "SXP-109",
                "the first stanza must begin 'Thus begins …:'",
                first.line,
            ));
            return None;
        }
        (_, _, 0) => {
            let msg = if entry.is_empty() {
                "malformed stanza header"
            } else {
                "'Thus begins' is only legal on the first stanza"
            };
            diags.push(diag("SXP-109", msg, first.line));
            return None;
        }
        _ => {
            diags.push(diag("SXP-109", "malformed stanza header", first.line));
            return None;
        }
    };
    let body = parse_body(&sentences[1..], diags);
    Some(Stanza {
        id: idx,
        title,
        params,
        ret,
        body,
        line: first.line,
    })
}

struct Frame {
    open: Open,
    line: usize,
    then: Vec<Stmt>,
    otherwise: Option<Vec<Stmt>>,
}

fn current<'a>(frames: &'a mut [Frame], root: &'a mut Vec<Stmt>) -> &'a mut Vec<Stmt> {
    match frames.last_mut() {
        Some(f) => f.otherwise.as_mut().unwrap_or(&mut f.then),
        None => root,
    }
}

fn close(f: Frame) -> Stmt {
    let kind = match f.open {
        Open::If(cond) => StmtKind::If {
            cond,
            then: f.then,
            otherwise: f.otherwise,
        },
        Open::While(cond) => StmtKind::While { cond, body: f.then },
        Open::Repeat(count) => StmtKind::Repeat {
            count,
            body: f.then,
        },
    };
    Stmt { kind, line: f.line }
}

fn parse_body(sentences: &[Sentence], diags: &mut Vec<Diagnostic>) -> Vec<Stmt> {
    let mut root: Vec<Stmt> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    for s in sentences {
        let is = |ws: &[&str]| {
            s.toks.len() == ws.len()
                && s.toks
                    .iter()
                    .zip(ws)
                    .all(|(t, w)| matches!(t, Tok::Word(x) if x == w))
        };
        if s.term == '.' && is(&["so", "be", "it"]) {
            match frames.pop() {
                Some(f) => current(&mut frames, &mut root).push(close(f)),
                None => diags.push(diag("SXP-107", "'So be it.' closes no open block", s.line)),
            }
        } else if s.term == ':' && is(&["otherwise"]) {
            match frames.last_mut() {
                Some(f) if matches!(f.open, Open::If(_)) && f.otherwise.is_none() => {
                    f.otherwise = Some(Vec::new());
                }
                _ => diags.push(diag(
                    "SXP-107",
                    "'Otherwise:' outside an open If, or repeated",
                    s.line,
                )),
            }
        } else if s.term == ':' {
            let mut opens = P { t: &s.toks }.block_header();
            opens.dedup_by(|a, b| open_eq(a, b));
            match opens.len() {
                0 => diags.push(diag(
                    "SXP-103",
                    "no grammar rule matches this block header",
                    s.line,
                )),
                1 => {
                    let open = opens.remove(0);
                    if open_has_bad_empty(&open) {
                        diags.push(diag(
                            "SXP-108",
                            "'an empty list of …' needs an annotated declaration",
                            s.line,
                        ));
                    }
                    frames.push(Frame {
                        open,
                        line: s.line,
                        then: vec![],
                        otherwise: None,
                    });
                }
                n => diags.push(diag(
                    "SXP-104",
                    &format!("ambiguous block header: {n} readings"),
                    s.line,
                )),
            }
        } else if let Some(kind) = simple_statement(s, diags) {
            current(&mut frames, &mut root).push(Stmt { kind, line: s.line });
        }
    }
    if let Some(f) = frames.last() {
        diags.push(diag(
            "SXP-107",
            "block is never closed with 'So be it.'",
            f.line,
        ));
    }
    root
}

fn open_eq(a: &Open, b: &Open) -> bool {
    match (a, b) {
        (Open::If(x), Open::If(y))
        | (Open::While(x), Open::While(y))
        | (Open::Repeat(x), Open::Repeat(y)) => x == y,
        _ => false,
    }
}

fn open_has_bad_empty(o: &Open) -> bool {
    match o {
        Open::If(e) | Open::While(e) | Open::Repeat(e) => has_empty(e),
    }
}

fn has_empty(e: &Expr) -> bool {
    match e {
        Expr::EmptyList(_) => true,
        Expr::Unary(_, a) => has_empty(a),
        Expr::Binary(_, a, b) | Expr::Index(a, b) => has_empty(a) || has_empty(b),
        Expr::Call(_, xs) | Expr::List(xs) => xs.iter().any(has_empty),
        _ => false,
    }
}

fn kind_has_bad_empty(k: &StmtKind) -> bool {
    match k {
        StmtKind::Decl { init, .. } => has_empty(init),
        StmtKind::Assign { value, .. } => has_empty(value),
        StmtKind::Return(Some(e)) | StmtKind::Expr(e) => has_empty(e),
        _ => false,
    }
}

/// Resolves `an empty list of T` in an annotated declaration; returns false if misplaced.
fn resolve_empty(kind: &mut StmtKind) -> bool {
    if let StmtKind::Decl { ty, init, .. } = kind
        && let Expr::EmptyList(t) = init
    {
        if *ty == Some(Type::List(Box::new(t.clone()))) {
            *init = Expr::List(vec![]);
        } else {
            return false;
        }
    }
    !kind_has_bad_empty(kind)
}

fn simple_statement(s: &Sentence, diags: &mut Vec<Diagnostic>) -> Option<StmtKind> {
    let mut parses = dedup(P { t: &s.toks }.statement());
    match parses.len() {
        0 => {
            let words: Vec<&str> = s
                .toks
                .iter()
                .filter_map(|t| {
                    if let Tok::Word(w) = t {
                        Some(w.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            let ops = words
                .iter()
                .filter(|w| matches!(**w, "plus" | "minus" | "times" | "over" | "modulo" | "is"))
                .count();
            let prefix = words.iter().any(|w| matches!(*w, "both" | "either"));
            if ops >= 2 && !prefix {
                diags.push(diag(
                    "SXP-106",
                    "chained infix operators; nest with prefix forms ('the sum of … and …')",
                    s.line,
                ));
            } else {
                diags.push(diag(
                    "SXP-103",
                    "no grammar rule matches this sentence",
                    s.line,
                ));
            }
            None
        }
        1 => {
            let mut kind = parses.remove(0);
            if resolve_empty(&mut kind) {
                Some(kind)
            } else {
                diags.push(diag("SXP-108", "'an empty list of …' is only legal in a declaration annotated with that list type", s.line));
                None
            }
        }
        n => {
            diags.push(diag(
                "SXP-104",
                &format!("ambiguous sentence: {n} distinct readings"),
                s.line,
            ));
            None
        }
    }
}
