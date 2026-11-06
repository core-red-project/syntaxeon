//! The compiler phases run in order (ADR-009) and what a tool can learn from them: diagnostics
//! with their headlines, and a per-verse phonetic report. Shared by the CLI and the language
//! server so both say the same thing.

pub mod format;
mod pipeline;
pub mod report;
pub mod rhythm;

pub use pipeline::{Analysis, analyze, category, headline};

use syntaxeon_lexicon::lexicon::Lexicon;
use syntaxeon_lexicon::rhyme::words_rhyme;

/// Whether two words rhyme under the lexicon's rules; `None` if either is unknown.
pub(crate) fn words_rhyme_under(lex: &Lexicon, a: &str, b: &str) -> Option<bool> {
    words_rhyme(lex, a, b).ok()
}
