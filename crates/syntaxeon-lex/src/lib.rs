//! Lexical and literary layers of Syntaxeon plus the prosody driver.
//! Normative references: `spec/PROSE_GRAMMAR.md` §2–§3, `spec/PROSODY.md` §5–§7.

pub mod lexer;
pub mod literary;
pub mod prosody;

pub use syntaxeon_core::Span;

/// A structured diagnostic; the literary voice is a renderer over this (`spec/PROSODY.md` §8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub line: usize,
    /// The words the error is about, when it is about part of a line (expressions).
    pub span: Option<Span>,
}
