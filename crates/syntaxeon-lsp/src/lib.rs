//! The Syntaxeon language server (ADR-020). It reports linguistic facts: diagnostics with ranges,
//! hover, go to definition, completion, formatting, rhyme quick fixes, rhyme-coloured tokens, and a phonetic report of the
//! verses. Producing or playing audio is the editor extension's job, never this server's.

mod features;
mod names;
mod position;
mod server;

pub use server::{capabilities, run};
