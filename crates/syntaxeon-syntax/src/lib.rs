//! Prose parser. Normative reference: `spec/PROSE_GRAMMAR.md`, ADR-005.
//!
//! The parser is a list-of-successes recognizer: every function returns *all* parses
//! of its input prefix, so a sentence with more than one distinct lowering is reported
//! as `SXP-104` instead of being resolved by an arbitrary choice.
//!
//! shortcut: normalization happens in the parse actions, not in rule tables (ADR-005
//! asks for rules as data); expression parsing is not memoized and is exponential in
//! the worst case. Revisit both before sentences get long or the grammar grows.

mod driver;
mod grammar;

pub use driver::parse_program;
