//! Runs the compiler phases in the order fixed by ADR-009 and stops at the first failing one.

use syntaxeon_check::check;
use syntaxeon_core::Program;
use syntaxeon_lex::Diagnostic;
use syntaxeon_lex::lexer::{Lexed, lex};
use syntaxeon_lex::literary::{Stanza as Verses, stanzas};
use syntaxeon_lex::prosody;
use syntaxeon_lexicon::lexicon::Lexicon;
use syntaxeon_syntax::parse_program;

pub struct Analysis {
    pub lexed: Lexed,
    pub verses: Vec<Verses>,
    pub program: Program,
    /// The pronunciation lexicon with the file's pragmas applied (set once prosody ran).
    pub lexicon: Option<Lexicon>,
    pub diagnostics: Vec<Diagnostic>,
    /// Phase that produced `diagnostics`, if any.
    pub failed_phase: Option<&'static str>,
}

#[must_use]
pub fn analyze(src: &str, phase: &mut dyn FnMut(&str)) -> Analysis {
    phase("lexical and grammatical analysis");
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    let (program, parse_diags) = parse_program(&lexed, &verses);
    let mut a = Analysis {
        diagnostics: lexed
            .diagnostics
            .iter()
            .cloned()
            .chain(parse_diags)
            .collect(),
        lexed,
        verses,
        program,
        lexicon: None,
        failed_phase: None,
    };
    if !a.diagnostics.is_empty() {
        a.failed_phase = Some("syntax");
        return finish(a);
    }
    phase("semantic analysis");
    a.diagnostics = check(&a.program);
    if !a.diagnostics.is_empty() {
        a.failed_phase = Some("semantics");
        return finish(a);
    }
    phase("prosodic validation");
    match Lexicon::pinned() {
        Ok(mut lex) => {
            a.diagnostics = prosody::check(&a.lexed, &a.verses, &mut lex);
            a.lexicon = Some(lex);
        }
        Err(e) => {
            a.diagnostics = vec![Diagnostic {
                code: e.code(),
                message: e.to_string(),
                line: 0,
            }];
        }
    }
    if !a.diagnostics.is_empty() {
        a.failed_phase = Some("prosody");
    }
    finish(a)
}

fn finish(mut a: Analysis) -> Analysis {
    a.diagnostics.sort_by_key(|d| d.line);
    a
}

#[must_use]
pub fn category(code: &str) -> &'static str {
    match code.strip_prefix("SXP-").and_then(|n| n.chars().next()) {
        Some('1') if code == "SXP-101" || code == "SXP-102" => "lexical",
        Some('1') => "grammatical",
        Some('2') => "prosodic",
        Some('3') => "lexicon",
        Some('4') => "semantic",
        Some('5') => "runtime",
        _ => "internal",
    }
}

/// Literary headline for codes that have one; the structured record stays authoritative.
#[must_use]
pub fn headline(code: &str) -> Option<&'static str> {
    Some(match code {
        "SXP-201" => "A lone verse has nothing to rhyme with.",
        "SXP-202" => "The verse hath failed to rhyme.",
        "SXP-203" => "No word could be found to end the verse.",
        "SXP-301" => "This word is a stranger to the lexicon.",
        _ => return None,
    })
}
