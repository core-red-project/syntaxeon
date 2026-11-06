# Changelog

All notable changes to **Syntaxeon** are documented here.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- An IR and a VM (ADR-021). `syntaxeon ir <file>` lists what each stanza lowers to (slots, jumps, the source line of each operation); `syntaxeon run` now executes that IR on an explicit frame stack. The tree evaluator stays as the reference: every evaluator test runs both engines and compares output and errors, and the whole-program fuzz suite runs the VM. `run` on the benchmark fixture: 105.0 ms before, 97.5 ms after
- `syntaxeon-lsp`, the language server for the future `syntaxeon-vscode` (ADR-020): diagnostics with ranges, hover with the sound and group of an end word, go to definition (functions, parameters, declarations), completion (names in reach and, on an end word, words that rhyme with its group's leader), formatting, rhyme quick fixes, tokens coloured by rhyme group, and two requests (`syntaxeon/verses`, `syntaxeon/compare`) that give an audio layer the pronunciations and the kind of each rhyme. It plays no sound itself. Release archives now carry both binaries
- A rhyme *kind* (perfect, assonant, identical, none) next to the yes/no verdict, and a phonetic verse report
- `the code of E` and `the letter with code E` (letters and their Unicode codes), `there is more to hear` (an end-of-input test) and `E reads as a whole number` (a test that `the whole number of` would succeed); `hear` becomes a reserved word (ADR-018)
- Language charter, 13 architecture decision records and drafts of the semantic core, prose grammar and prosody specifications
- Pinned pronunciation lexicon (CMU Pronouncing Dictionary) with sha256 verification, the rhyme function and the permitted stanza schemes
- Lexer, literary layer (verses, stanzas, end words) and pronunciation pragmas
- Prose parser that returns every reading of a sentence and rejects ambiguity, memoized so deeply nested expressions parse in linear time, with its closed verb sets, connectives and operator maps kept as data tables, including the declaration and assignment sentence shapes (`rules.rs`)
- Name resolution and static type checking
- `rhymes --near` for assonant partners, and the `rhythm.toml` project file (`SXP-305` when malformed)
- Differential fuzzing of strings, floats, `hear` and their conversions against a second reference model
- `syntaxeon fmt` (`--write`, `--check`): canonical whitespace and canonical spelling of declarations, assignments and `Give back` / `Return`, also for sentences split over lines, applied only when the program still has the same meaning and still rhymes (ADR-014)
- Tree-walking evaluator with checked integers and a 10 000-frame call limit
- Whole-program differential fuzzing against an independent reference interpreter, mutation fuzzing of block structure and crash safety, grammar-driven, free-form and token-soup fuzzing of the prose parser, property fuzzing of rhyme and stanza schemes against an independent oracle over the raw dictionary, plus an `SXP-104` message that shows the competing readings
- `syntaxeon` command with `check`, `explain`, `ast`, `run`, `rhymes` (common words only, from a vendored ESDB list pinned by sha256, most common first using SCOWL frequency tiers 10 and 20, also pinned), shell completions and a man page
- Examples (including FizzBuzz, gcd, powers of two and a greeter that reads input), a benchmark fixture, and the project's CI, release and dependency-update automation

### Changed

- Every diagnostic that can point somewhere does: duplicate names and assignments to an immutable name underline the name, grammar errors underline the sentence, and rhyme errors underline the end word of the verse that breaks the rhyme (and name that verse's line); only errors about a whole program or a whole stanza keep a line. Every error code now has a literary headline above its technical line
- `sum`, `product`, `length`, `text` and the other words that start a prefix form can name variables (`the sum`, `the length`); only the whole prefix form (`the sum of`) is refused after `the` (ADR-019)
- Headers rhyme on the words before their closer (`If the pile is ten, then:` ends in `ten`; a function header ends in its title) instead of on `then`, `do`, `times` or a type word (ADR-017); the examples and the benchmark fixture were rewritten, and `fizzbuzz` and `powers` print different strings
- A rhyme group is valid only if every two of its verses rhyme, not only each against the first, so the verdict no longer depends on verse order (ADR-016); a triple holding an identical pair such as `men` and `amen` now fails
- Assonance (the same vowels from the stressed vowel on, such as `men` / `bed`) is now valid rhyme by default; a `rhythm.toml` with `[rhyme] assonance = false` restores perfect-only rhyme (ADR-015). A word no longer rhymes with itself through its alternative pronunciations
- Every expression carries the byte span of its words; type errors and runtime errors underline the offending expression (`file:line:col` and a caret line), and `check --json` reports `column` and `span`
- Pinned toolchain moved from Rust 1.93.0 to 1.99.1; the CI and release workflows now read it from `rust-toolchain.toml`, so a toolchain bump is a one-file change
- CI actions updated: install-action 2.90.1, action-gh-release 3.0.4, just 1.59.0, cargo-deny 0.20.2, rust-toolchain, rust-cache 2.9.3, upload-artifact 7.0.2, download-artifact 8.0.2

### Fixed

- An error in the condition of a `While` loop, on any pass after the first, was reported at the last statement of the body; it now points at the loop's own statement

- Sentences that the prose grammar could read two ways (found by fuzzing): arithmetic prefix forms take plain operands, an open call or list keeps the comma and the infix word that follow it, and variable names cannot contain `of`
- Hyphenated end words (and verbalized numerals such as `75`) rhyme on their last component, so `seventy-five` rhymes with `five`
- SHA-256 block iteration uses `as_chunks`, and tests assert emptiness with `assert_eq!`, to satisfy the clippy lints of Rust 1.99

---

[Unreleased]: https://github.com/core-red-project/syntaxeon/commits/main
