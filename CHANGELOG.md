# Changelog

All notable changes to **Syntaxeon** are documented here.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- Language charter, 13 architecture decision records and drafts of the semantic core, prose grammar and prosody specifications
- Pinned pronunciation lexicon (CMU Pronouncing Dictionary) with sha256 verification, the rhyme function and the permitted stanza schemes
- Lexer, literary layer (verses, stanzas, end words) and pronunciation pragmas
- Prose parser that returns every reading of a sentence and rejects ambiguity, with its closed verb sets, connectives and operator maps kept as data tables, including the declaration and assignment sentence shapes (`rules.rs`)
- Name resolution and static type checking
- Tree-walking evaluator with checked integers and a 10 000-frame call limit
- Whole-program differential fuzzing against an independent reference interpreter, mutation fuzzing of block structure and crash safety, grammar-driven, free-form and token-soup fuzzing of the prose parser, property fuzzing of rhyme and stanza schemes against an independent oracle over the raw dictionary, plus an `SXP-104` message that shows the competing readings
- `syntaxeon` command with `check`, `explain`, `ast`, `run`, `rhymes` (common words only, from a vendored ESDB list pinned by sha256), shell completions and a man page
- Examples (including FizzBuzz, gcd, powers of two and a greeter that reads input), a benchmark fixture, and the project's CI, release and dependency-update automation

### Changed

- Pinned toolchain moved from Rust 1.93.0 to 1.99.1; the CI and release workflows now read it from `rust-toolchain.toml`, so a toolchain bump is a one-file change
- CI actions updated: install-action 2.89.1, rust-toolchain, rust-cache 2.9.3, upload-artifact 7.0.2, download-artifact 8.0.2

### Fixed

- Sentences that the prose grammar could read two ways (found by fuzzing): arithmetic prefix forms take plain operands, an open call or list keeps the comma and the infix word that follow it, and variable names cannot contain `of`
- Hyphenated end words (and verbalized numerals such as `75`) rhyme on their last component, so `seventy-five` rhymes with `five`
- SHA-256 block iteration uses `as_chunks`, and tests assert emptiness with `assert_eq!`, to satisfy the clippy lints of Rust 1.99

---

[Unreleased]: https://github.com/core-red-project/syntaxeon/commits/main
