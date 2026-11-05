//! Prose parser. Normative reference: `spec/PROSE_GRAMMAR.md`, ADR-005.
//!
//! The parser is a list-of-successes recognizer: every function returns *all* parses
//! of its input prefix, so a sentence with more than one distinct lowering is reported
//! as `SXP-104` instead of being resolved by an arbitrary choice.
//!
//! Surface forms are data (`rules`); `grammar` holds the generic readers.
//!
//! shortcut: expression parsing is not memoized and is exponential in the worst case;
//! revisit before sentences get long or the grammar grows.

mod driver;
mod grammar;
mod rules;

pub use driver::{parse_program, statement_readings};
