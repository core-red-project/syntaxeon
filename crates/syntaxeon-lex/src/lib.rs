//! Lexical and literary layers of Syntaxeon plus the prosody driver.
//! Normative references: `spec/PROSE_GRAMMAR.md` §2–§3, `spec/PROSODY.md` §5–§7.

pub mod lexer;
pub mod literary;
pub mod prosody;

/// Byte range into the source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A structured diagnostic; the literary voice is a renderer over this (`spec/PROSODY.md` §8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub line: usize,
}
