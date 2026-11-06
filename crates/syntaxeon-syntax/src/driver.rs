//! Stanza and block driver (`spec/PROSE_GRAMMAR.md` §3, §6.6, §8, §10).

use syntaxeon_core::{Expr, ExprKind, Program, Span, Stanza, Stmt, StmtKind, Type};
use syntaxeon_lex::Diagnostic;
use syntaxeon_lex::lexer::{Lexed, Tok, lex};
use syntaxeon_lex::literary::Stanza as Verses;

use crate::grammar::{Open, P};
use crate::rules::{FORMS, Step};

struct Sentence {
    toks: Vec<Tok>,
    spans: Vec<Span>,
    term: char,
    line: usize,
}

fn diag(code: &'static str, message: &str, line: usize) -> Diagnostic {
    Diagnostic {
        code,
        message: message.to_string(),
        line,
        span: None,
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
    let mut cur_spans: Vec<Span> = Vec::new();
    let mut line = 0;
    for t in &lexed.tokens[first.tokens.start..last.tokens.end] {
        if cur.is_empty() {
            line = t.line;
        }
        match t.tok {
            Tok::Punct(c @ ('.' | '!' | '?' | ':')) => {
                out.push(Sentence {
                    toks: std::mem::take(&mut cur),
                    spans: std::mem::take(&mut cur_spans),
                    term: c,
                    line,
                });
            }
            ref other => {
                cur.push(other.clone());
                cur_spans.push(t.span);
            }
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
    let p = P::new(&first.toks, &first.spans);
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
            let mut opens = P::new(&s.toks, &s.spans).block_header();
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
    match &e.kind {
        ExprKind::EmptyList(_) => true,
        ExprKind::Unary(_, a) => has_empty(a),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => has_empty(a) || has_empty(b),
        ExprKind::Call(_, xs) | ExprKind::List(xs) => xs.iter().any(has_empty),
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
        && let ExprKind::EmptyList(t) = &init.kind
    {
        if *ty == Some(Type::List(Box::new(t.clone()))) {
            *init = Expr {
                kind: ExprKind::List(vec![]),
                span: init.span,
            };
        } else {
            return false;
        }
    }
    !kind_has_bad_empty(kind)
}

fn simple_statement(s: &Sentence, diags: &mut Vec<Diagnostic>) -> Option<StmtKind> {
    let mut parses = dedup(P::new(&s.toks, &s.spans).statement());
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
            let infix = words
                .iter()
                .filter(|w| matches!(**w, "plus" | "minus" | "times" | "over" | "modulo"))
                .count();
            let compares = words.iter().filter(|w| **w == "is").count();
            let logic = words.iter().any(|w| matches!(*w, "both" | "either"));
            if infix >= 1 || (compares >= 2 && !logic) {
                diags.push(diag(
                    "SXP-106",
                    "an infix word is where the grammar does not allow one: infix words take two plain operands, never chain, and never appear inside 'the sum of … and …'; nest with a prefix form",
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
            let message = ambiguity_message(n, &parses);
            diags.push(diag("SXP-104", &message, s.line));
            None
        }
    }
}

/// All distinct readings of one simple sentence given with its terminator, for tooling and
/// tests: none is `SXP-103`, more than one is `SXP-104`. Blocks and headers are not sentences
/// in this sense.
#[must_use]
pub fn statement_readings(sentence: &str) -> Vec<StmtKind> {
    let lexed = lex(sentence);
    let toks: Vec<Tok> = lexed.tokens.iter().map(|t| t.tok.clone()).collect();
    match toks.split_last() {
        Some((Tok::Punct('.' | '!' | '?'), body)) => {
            let spans: Vec<Span> = lexed.tokens.iter().map(|t| t.span).collect();
            dedup(P::new(body, &spans).statement())
        }
        _ => vec![],
    }
}

/// The `SXP-104` text: how many readings, the first three, and what to do.
fn ambiguity_message(n: usize, readings: &[StmtKind]) -> String {
    let shown: Vec<String> = readings
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, r)| {
            let text = format!("{r:?}");
            let cut = text
                .char_indices()
                .nth(110)
                .map_or(text.len(), |(at, _)| at);
            let dots = if cut < text.len() { "…" } else { "" };
            format!("[{}] {}{dots}", i + 1, &text[..cut])
        })
        .collect();
    format!(
        "ambiguous sentence: {n} distinct readings: {}; bind a value with 'Let' first, or nest with a prefix form",
        shown.join(" ")
    )
}

/// Other spellings of a declaration or assignment sentence (terminator removed): the same
/// words re-ordered into every form listed *before* the one it uses in `rules::FORMS`, most
/// preferred first. A candidate is only a proposal; the caller must check that it still reads
/// as the same statement (and, for `fmt`, still rhymes).
#[must_use]
pub fn preferred_spellings(sentence: &str) -> Vec<String> {
    let lexed = lex(sentence);
    if !lexed.diagnostics.is_empty() {
        return vec![];
    }
    let toks: Vec<Tok> = lexed.tokens.iter().map(|t| t.tok.clone()).collect();
    let spans: Vec<Span> = lexed.tokens.iter().map(|t| t.span).collect();
    let matches = P::new(&toks, &spans).form_matches();
    let Some(current) = matches.iter().map(|(k, _)| *k).min() else {
        return vec![];
    };
    let make = FORMS[current].make;
    let mut out: Vec<(usize, String)> = Vec::new();
    for (_, spans) in matches.iter().filter(|(k, _)| *k == current) {
        let text = |k: usize| -> String {
            let (a, b) = spans[k];
            if a == b {
                return String::new();
            }
            let (from, to) = (lexed.tokens[a].span.start, lexed.tokens[b - 1].span.end);
            let mut t = sentence[from..to].to_string();
            if a == 0 && matches!(lexed.tokens[0].tok, Tok::Word(_)) {
                lower_first(&mut t);
            }
            t
        };
        // Slots by role, from the form that matched.
        let slot = |step: Step| -> String {
            FORMS[current]
                .steps
                .iter()
                .position(|s| std::mem::discriminant(s) == std::mem::discriminant(&step))
                .map(text)
                .unwrap_or_default()
        };
        for (k, form) in FORMS.iter().enumerate().take(current) {
            if std::mem::discriminant(&form.make) != std::mem::discriminant(&make) {
                continue;
            }
            let mut s = String::new();
            for step in form.steps {
                let piece = match step {
                    Step::Words(ws) => ws.join(" "),
                    other => slot(*other),
                };
                if piece.is_empty() {
                    continue;
                }
                // An apposition hugs the name before it.
                if !s.is_empty() && !matches!(step, Step::Annot) {
                    s.push(' ');
                }
                s.push_str(&piece);
            }
            upper_first(&mut s);
            if s != sentence && !out.iter().any(|(_, o)| *o == s) {
                out.push((k, s));
            }
        }
    }
    out.sort_by_key(|(k, _)| *k);
    out.into_iter().map(|(_, s)| s).collect()
}

fn lower_first(s: &mut String) {
    if let Some(c) = s.chars().next().filter(|c| c.is_uppercase()) {
        s.replace_range(..c.len_utf8(), &c.to_lowercase().to_string());
    }
}

fn upper_first(s: &mut String) {
    if let Some(c) = s.chars().next().filter(|c| c.is_lowercase()) {
        s.replace_range(..c.len_utf8(), &c.to_uppercase().to_string());
    }
}

#[cfg(test)]
mod tests {
    use syntaxeon_core::Expr;

    use super::*;

    #[test]
    fn the_ambiguity_message_lists_the_readings() {
        let a = StmtKind::Return(Some(Expr::Int(1)));
        let b = StmtKind::Return(None);
        let m = ambiguity_message(2, &[a, b]);
        assert!(m.starts_with("ambiguous sentence: 2 distinct readings: [1] Return(Some(Int(1)))"));
        assert!(m.contains("[2] Return(None)") && m.contains("bind a value"));
    }
}
