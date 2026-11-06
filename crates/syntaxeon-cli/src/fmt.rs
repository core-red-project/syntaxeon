//! `fmt`: canonical whitespace, and canonical spelling of declarations and assignments when
//! that keeps both the meaning and the rhyme (`spec/PROSE_GRAMMAR.md` §6.1–§6.2, ADR-014).
//!
//! Every rewrite is a proposal from the grammar and is accepted only after the whole pipeline
//! agrees: the program has no diagnostics and the same AST (positions aside). A rewrite that
//! would break a rhyme therefore never happens, whatever the grammar proposed.

use syntaxeon_core::{Program, Stmt, StmtKind};
use syntaxeon_lex::Diagnostic;
use syntaxeon_syntax::preferred_spellings;

use crate::pipeline::{Analysis, analyze};

/// Rewrites can enable one another through rhyme, so repeat until nothing changes.
const MAX_PASSES: usize = 8;

/// The formatted source, or the diagnostics that stop `fmt` (it only formats programs that
/// parse and type-check).
pub fn format(src: &str) -> Result<String, Vec<Diagnostic>> {
    let first = analyze(src, &mut |_| {});
    if matches!(first.failed_phase, Some("syntax" | "semantics")) {
        return Err(first.diagnostics);
    }
    let mut text = whitespace(src);
    let base = analyze(&text, &mut |_| {});
    // Whitespace never changes meaning or rhyme; if it somehow did, leave the file alone.
    let codes = |a: &Analysis| a.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
    if codes(&base) != codes(&first) || shape(&base.program) != shape(&first.program) {
        return Ok(src.to_string());
    }
    if !base.diagnostics.is_empty() {
        // A program that does not rhyme gets whitespace only: rewriting could not be
        // shown to keep a verdict that is already "invalid".
        return Ok(text);
    }
    let want = shape(&base.program);
    for _ in 0..MAX_PASSES {
        let mut changed = false;
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let mut i = 0;
        while i < lines.len() {
            let end = sentence_end(&lines, i);
            let sentence = lines[i..=end].join(" ");
            for candidate in spellings(&sentence) {
                let mut trial_lines = lines.clone();
                trial_lines.splice(i..=end, [candidate]);
                let trial = trial_lines.join("\n") + "\n";
                let a = analyze(&trial, &mut |_| {});
                if a.diagnostics.is_empty() && shape(&a.program) == want {
                    lines = trial_lines;
                    changed = true;
                    break;
                }
            }
            i += 1;
        }
        text = lines.join("\n") + "\n";
        if !changed {
            break;
        }
    }
    Ok(text)
}

/// Index of the last line of the sentence that starts on line `i`: a sentence runs on until a
/// line ends with its terminator (`.`, `!`, `?`), or `:` for a header. A blank line or an
/// aside stops the search, which leaves `i` alone.
fn sentence_end(lines: &[String], i: usize) -> usize {
    let mut j = i;
    while j < lines.len() && !lines[j].is_empty() && !lines[j].contains('(') {
        if lines[j].ends_with(['.', '!', '?', ':']) {
            return j;
        }
        j += 1;
    }
    i
}

/// Candidate replacement lines for one sentence (possibly joined from several lines). Only
/// sentences without an aside qualify, since a rewrite would drop it.
fn spellings(line: &str) -> Vec<String> {
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let Some(term) = body.chars().last().filter(|c| matches!(c, '.' | '!' | '?')) else {
        return vec![];
    };
    if body.contains('(') {
        return vec![];
    }
    let sentence = &body[..body.len() - term.len_utf8()];
    preferred_spellings(sentence)
        .into_iter()
        .map(|s| format!("{indent}{s}{term}"))
        .collect()
}

/// No trailing spaces, no leading blank lines, at most one blank line in a row (never inside
/// an aside, where blank lines may carry meaning for the author), one final newline.
fn whitespace(src: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut depth: i32 = 0;
    for raw in src.lines() {
        let line = raw.trim_end();
        let blank = line.is_empty() && depth == 0;
        if blank && out.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        let mut in_string = false;
        for c in line.chars() {
            match c {
                '"' => in_string = !in_string,
                '(' if !in_string => depth += 1,
                ')' if !in_string => depth = (depth - 1).max(0),
                _ => {}
            }
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    if out.is_empty() {
        return String::new();
    }
    out.join("\n") + "\n"
}

/// The program without source positions, for comparing meaning.
fn shape(p: &Program) -> Program {
    fn strip(body: &mut [Stmt]) {
        for s in body {
            s.line = 0;
            match &mut s.kind {
                StmtKind::If {
                    then, otherwise, ..
                } => {
                    strip(then);
                    if let Some(o) = otherwise {
                        strip(o);
                    }
                }
                StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } => strip(body),
                _ => {}
            }
        }
    }
    let mut p = p.clone();
    for s in &mut p.stanzas {
        s.line = 0;
        strip(&mut s.body);
    }
    p
}
