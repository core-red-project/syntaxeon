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
- Prose parser that returns every reading of a sentence and rejects ambiguity
- Name resolution and static type checking
- Tree-walking evaluator with checked integers and a 10 000-frame call limit
- `syntaxeon` command with `check`, `explain`, `ast`, `run`, shell completions and a man page
- Examples, a benchmark fixture, and the project's CI, release and dependency-update automation

---

[Unreleased]: https://github.com/core-red-project/syntaxeon/commits/main
