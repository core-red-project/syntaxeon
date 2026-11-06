# Syntaxeon

![Version](https://img.shields.io/badge/version-0.0.1-blue)
![License](https://img.shields.io/badge/License-MIT-green)
![MSRV](https://img.shields.io/badge/rust-1.93-orange)
[![CI](https://github.com/core-red-project/syntaxeon/workflows/CI/badge.svg)](https://github.com/core-red-project/syntaxeon/actions)

<p align="center">
  <strong>Prose is syntax ✦ Rhyme is validity ✦ Deterministic by design</strong><br>
  <em>An esoteric, statically typed programming language in which English prose that also rhymes is the program.</em>
</p>

<p align="center">
  <a href="#about">About</a> ✦
  <a href="#features">Features</a> ✦
  <a href="#installation">Installation</a> ✦
  <a href="#usage">Usage</a> ✦
  <a href="#architecture">Architecture</a> ✦
  <a href="#contributing">Contributing</a>
</p>

---

## About

**Syntaxeon** is an esoteric programming language whose programs are English prose organized in stanzas. A program is valid only if its sentences parse, its types check, **and** the end words of its verses rhyme according to a fixed scheme. A program that does not rhyme is not a program with a warning: it is rejected.

Most esolangs borrow a theme and keep an ordinary grammar underneath. Syntaxeon makes the poetic constraint part of the grammar of validity, so the language is the combination of a controlled natural language and a prosodic checker. It is an experiment and an art object, not a production tool.

The compiler pipeline is explicit and each phase is its own crate: lexical analysis, literary analysis (verses and stanzas), a prose parser that returns every reading of a sentence and refuses ambiguity, semantic and type analysis, prosodic validation against a pinned pronunciation lexicon, and an evaluator.

### Philosophy

> *"A program that does not rhyme is not a program."*

Syntaxeon is a CoreRed project, part of the Sxnnyside Project's experimental branch.

## Features

- **Rhyme as validity**: end rhyme within a stanza is checked like a type; a failure is an error with a stable code (`SXP-202`), not a lint.
- **Prose grammar**: closed, deterministic, no statistical parsing; several surface forms of one sentence normalize to one AST.
- **Ambiguity is an error**: a sentence with two distinct readings is rejected (`SXP-104`), never resolved silently.
- **Static, strong typing**: `Integer`, `Float`, `Boolean`, `String`, `Character`, `List<T>`, no implicit conversions.
- **Deterministic rhyme**: pronunciation comes only from a vendored, sha256-pinned CMU dictionary, never from the host.
- **Explainable**: `syntaxeon explain` shows every end word's tail, rhyme group and verdict.
- **Reproducible evaluation**: checked integers, a fixed 10 000-frame call limit, no hidden state.

## Installation

### Prerequisites

- Rust (1.93 or newer; the repository pins `1.99.1`)
- [just](https://github.com/casey/just) (to run the project recipes)

### From Source

```bash
git clone https://github.com/core-red-project/syntaxeon.git
cd syntaxeon

just install
cargo install --path crates/syntaxeon-cli --locked
```

Prebuilt archives for Linux, macOS (Apple silicon) and Windows are attached to each [release](https://github.com/core-red-project/syntaxeon/releases) together with a `SHA256SUMS` file.

## Usage

Save this as `tale.sxe`:

```
Thus begins the tale of men:
Speak "Hello again".
Proclaim the text of ten.
Speak "Amen".
```

```bash
syntaxeon check tale.sxe      # syntax, types and rhyme
syntaxeon explain tale.sxe    # end-word tails, rhyme groups, verdict
syntaxeon run tale.sxe        # check, then execute
syntaxeon rhymes then         # find a word to end the next verse with
```

The four end words (`men`, `again`, `ten`, `amen`) form the stanza's `AABB` scheme, which is why the program is valid. Change `"Hello again"` to `"Hello"` and `check` fails with `SXP-202`. More programs are in [examples/](examples/).

### Commands

| Command | Does |
|---|---|
| `check <file>` | Check syntax, types and rhyme; exit 1 if the program is invalid |
| `fmt <file>` | Print the program in canonical form, keeping its meaning and its rhymes (`--write` rewrites the file, `--check` exits 1 if it would change) |
| `explain <file>` | Show how each stanza reads and rhymes |
| `ast <file>` | Dump the canonical AST |
| `run <file>` | Check, then run the program (`hear` reads standard input) |
| `rhymes <word>` | List common words that rhyme with a word, one per line (`--limit <n>`, `--all` for every lexicon word, `--json`) |
| `completions <shell>` | Print a completion script: bash, zsh, fish, powershell |
| `help`, `version` | Usage and version |

Global options: `-q`/`--quiet`, `-v`/`--verbose`, `--json` (`check` and `rhymes`), `--no-color`, `-h`/`--help`, `-V`/`--version`. Use `-` as the file to read standard input (except for `run`).

### Output and exit codes

Data goes to stdout; diagnostics, notes and errors go to stderr. Color is off when `NO_COLOR` is set, with `--no-color`, or when the stream is not a terminal.

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Invalid program, runtime failure or I/O failure |
| `2` | Usage error |

`check --json` prints one object on stdout: `file`, `ok`, `phase` (`syntax`, `semantics`, `prosody` or `null`), and `diagnostics`, each with `code`, `category`, `severity`, `line`, `column`, `span` and `message`. `column` and `span` (`{"start", "end"}`, byte offsets) are `null` unless the error is about one expression; human output then underlines it. Diagnostic codes are documented in [spec/](spec/): `SXP-1xx` syntax, `SXP-2xx` prosody, `SXP-3xx` lexicon, `SXP-4xx` static semantics, `SXP-5xx` runtime.

### Configuration

Syntaxeon reads no configuration files and no environment variables except `NO_COLOR`. The pronunciation lexicon is embedded in the binary.

### Benchmarks

`just bench` times startup, `check` and `run` on a committed fixture. Baselines are in [docs/benchmarks.md](docs/benchmarks.md).

## Architecture

Topology: **Non-Monolithic** Cargo workspace, one language (Rust). A single root `Justfile` drives every crate.

```
syntaxeon/
├── crates/     # Cargo workspace members, one crate per compiler phase
├── spec/       # Language charter, grammar, prosody, semantics and ADRs
├── examples/   # Valid programs, checked by the test suite
├── benches/    # Benchmark fixture
├── docs/       # Roadmap, glossary, benchmarks, release process
├── scripts/    # Commit checks and release packaging
└── .github/    # CI, release, templates
```

| Crate | Responsibility |
|---|---|
| `syntaxeon-core` | Canonical AST and types |
| `syntaxeon-lexicon` | Pinned pronunciation lexicon, rhyme function, stanza schemes |
| `syntaxeon-lex` | Lexer, verses and stanzas, prosody driver |
| `syntaxeon-syntax` | Prose parser (all readings) lowering to the AST |
| `syntaxeon-check` | Name resolution and type checking |
| `syntaxeon-eval` | Tree-walking evaluator |
| `syntaxeon-cli` | The `syntaxeon` command |

The language is defined in [spec/](spec/); start with [the charter](spec/LANGUAGE_DESIGN_CHARTER.md).

## Contributing

Contributions are accepted. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Before contributing, read the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details. The vendored pronunciation dictionary and the common-word list used by `rhymes` keep their own notices in [CMUDICT-LICENSE](crates/syntaxeon-lexicon/lexicon/CMUDICT-LICENSE) and [COMMON-WORDS-LICENSE](crates/syntaxeon-lexicon/lexicon/COMMON-WORDS-LICENSE).

---

<p align="center">
  <strong>Syntaxeon</strong> — A CoreRed Project<br>
  <em>&copy; 2026 Sxnnyside Project</em>
</p>
