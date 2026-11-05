# Syntaxeon Assistant Guide

## Project Purpose
Syntaxeon is an esoteric programming language whose programs are English prose in stanzas. A program is valid only if its sentences parse (with exactly one reading each), its types check, and the end words of its verses rhyme according to the stanza scheme. The specification in `spec/` is authoritative; the implementation follows it.

## Repository Topology
Non-Monolithic Cargo workspace, a single language (Rust), one root `Justfile`. Members live under `crates/`:

- `crates/syntaxeon-core`: canonical AST and types (`spec/SEMANTIC_CORE.md`).
- `crates/syntaxeon-lexicon`: pinned pronunciation lexicon, rhyme function, stanza schemes (`spec/PROSODY.md`).
- `crates/syntaxeon-lex`: lexer, verses and stanzas, end words, prosody driver (`spec/PROSE_GRAMMAR.md` §2–§3).
- `crates/syntaxeon-syntax`: prose parser returning every reading of a sentence, lowering to the AST (`spec/PROSE_GRAMMAR.md`).
- `crates/syntaxeon-check`: name resolution and type checking (`spec/SEMANTIC_CORE.md` §5).
- `crates/syntaxeon-eval`: tree-walking evaluator (`spec/SEMANTIC_CORE.md` §6, ADR-013).
- `crates/syntaxeon-cli`: the `syntaxeon` binary (`check`, `explain`, `ast`, `run`, `rhymes`, completions, man).

## Standard Command Surface
Use `just` for every common task:

```bash
just install       # Toolchain components, dependency fetch, git hooks (core.hooksPath = .githooks)
just dev           # Explain a bundled example
just build         # Build all workspace crates
just test          # Run the workspace test suite
just typecheck     # cargo check on all targets
just lint          # Clippy with warnings denied
just format        # Apply rustfmt
just format-check  # Verify formatting without modifying files
just deny          # Audit dependencies (DENY_OFFLINE=1 skips the advisory fetch)
just check         # The complete gate: format-check, lint, typecheck, test, deny; never modifies files
just bench         # Startup, check and run timings (not part of check)
just package       # Archive a release build; reads TARGET and VERSION
just changelog     # Write RELEASE_NOTES.md; reads VERSION
just clean         # Remove build artifacts
```

## Where Things Go
| What | Where |
|---|---|
| Language semantics, grammar, prosody, decisions | `spec/` (`spec/adr/ADR-NNN-*.md` for each decision) |
| Planning and open work | `docs/ROADMAP.md`; shipped changes in `CHANGELOG.md` |
| Domain terms | `docs/glossary.md` |
| Release process and asset names | `docs/RELEASING.md` |
| Benchmark baselines | `docs/benchmarks.md`; fixture in `benches/` |
| Compiler, runtime and CLI code | `crates/<crate>/src/`; integration tests in `crates/<crate>/tests/` |
| Pronunciation lexicon, common-word list and their pins | `crates/syntaxeon-lexicon/lexicon/` (`cmudict.dict`, `common-words.txt`, `lexicon.lock`); never edit by hand |
| Example programs | `examples/` (every file must pass `syntaxeon check`; a test enforces it) |
| Git hooks and commit-message checks | `.githooks/`, `scripts/` |
| Release packaging | `scripts/package.sh` |
| CI, release and dependency automation | `.github/workflows/`, `.github/dependabot.yml` |

## Architecture & Data Flow
1. **Source** → `syntaxeon-lex::lexer`: tokens, asides removed, pragmas collected.
2. **Tokens** → `syntaxeon-lex::literary`: verses, stanzas, end words.
3. **Sentences** → `syntaxeon-syntax`: every reading of each sentence; zero is `SXP-103`, more than one distinct reading is `SXP-104`.
4. **AST** → `syntaxeon-check`: names and types.
5. **Verses** → `syntaxeon-lex::prosody` with `syntaxeon-lexicon`: rhyme per stanza.
6. **AST** → `syntaxeon-eval`: execution.

Phases run in that order (prosody after semantics) and checking stops at the first phase that reports errors. Pronunciation comes only from the vendored lexicon; never add a host-dependent source.

## Coding Conventions
- Rust 2024, toolchain pinned in `rust-toolchain.toml`; `rustfmt` with a 100-column limit.
- Workspace lints: `unsafe_code` forbidden, clippy `all` denied, `pedantic` warn, `unwrap_used` and `expect_used` warn. CI runs clippy with `-D warnings`, so warnings fail the gate. Every `allow` carries a comment with its reason.
- Pipeline stages report `Diagnostic` values with a stable `SXP-NNN` code; they never panic on user input.
- Comments are proportional and timeless; cite spec sections instead of repeating them. A deliberate shortcut is marked `shortcut: <limit>, <when to upgrade>`.
- Names: English, one term per concept (see `docs/glossary.md`), snake_case files and modules, no `mod.rs`.
- Internal crate versions are declared once in `[workspace.dependencies]` and must equal the workspace version.

## Testing
`just test` runs unit, integration and fuzz tests with small, fixed iteration counts. To search harder: `SYNTAXEON_FUZZ_ITERS=20000 SYNTAXEON_FUZZ_SEED=7 cargo test --release -p syntaxeon-cli --test fuzz_programs` (whole programs against a reference interpreter) or `-p syntaxeon-syntax --test fuzz` (sentences and headers) or `-p syntaxeon-lexicon --test fuzz_rhyme` (rhyme and stanzas against an oracle); `cargo test --release … explore_ -- --ignored --nocapture` prints what a run covers. A failing seed is printed with its program; keep it as a regression test.

## Commit Conventions
Conventional Commits, enforced by the `commit-msg` hook and by CI: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
