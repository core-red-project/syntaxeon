# Roadmap

Open work, most important first. Shipped changes are in [CHANGELOG.md](../CHANGELOG.md); decisions are in [spec/adr/](../spec/adr/).

## Next

- **What writing real programs showed** (`examples/powers`, `fizzbuzz`, `gcd`, `greeter`; 8–19 lines each, all valid):
  - Block headers have fixed end words (`If …, then:` → *then*, `As long as …, do:` → *do*, `Do this … times:` → *times*), so each header needs a partner line ending in *-en*, *-oo* or *-imes*. Programs pick up flavor lines (`Proclaim "Buzz, say when".`) whose job is to rhyme.
  - A function header always ends in its type word (`number`, `text`, `truth`), which pushes authors to name parameters after rhymes (`slumber`, `lumber`) and to end lines on parameter names.
  - Two declarations that both end in `zero` cannot pair: identical rhyme is rejected. Counters get rhyming names (`chimes`, to rhyme with `times`).
  - A chain of `Otherwise:` needs one `So be it.` per `If`; exempt from rhyme but noisy.
  - Rhyme tooling is the bottleneck: `explain` shows what failed, but finding a partner word needs a lexicon search. A `rhymes <word>` command is the highest-value next feature (the lexicon has a lot of noise: abbreviations and proper names, so suggestions need a frequency filter).
  - Fixed along the way: hyphenated words and verbalized numerals now rhyme on their last component (`seventy-five` → `five`).
  - Identifiers cannot contain `_` or start with a determiner word (`a`, `an`, `the`, …), which surprises at first.
- **Parser memoization: done.** The `operand`, `arith` and `expr` levels cache per start token; nested calls (16 levels never finished) now parse 80 levels in 0.05 s.
- **Language audit (2026-11-06): see [AUDIT.md](AUDIT.md).** Every finding is resolved (ADR-016 to ADR-019) or closed as not needing change.
- **Fuzzing: done.** Expressions and headers (`crates/syntaxeon-syntax/tests/fuzz.rs`): the first run found 16 % of random valid sentences ambiguous; after the rules in `spec/PROSE_GRAMMAR.md` §7.4 and the `of` rule for names, 600 000 valid sentences, 200 000 free-form ones and 1 000 000 random token sequences gave zero ambiguities. Whole programs (`crates/syntaxeon-cli/tests/fuzz_programs.rs`): random well-typed programs with functions, nested blocks, shadowing, early returns, overflow and division by zero are run through the real pipeline and compared with an independent reference interpreter; 60 000 programs agreed on output and error code, mutated programs (deleted, duplicated, swapped or corrupted lines) are always rejected without crashing, and an unbalanced `So be it.` is always reported. `SXP-104` has no known trigger; it stays as the guard for grammar changes. Rhyme (`crates/syntaxeon-lexicon/tests/fuzz_rhyme.rs`): the rhyme function, rhyme lists and stanza schemes agree with an independent oracle over the raw dictionary on 300 000 word pairs and stanzas. Strings, floats and the conversions between them have a second straight-line generator and model (20 000 programs per seed agree). `hear` is covered too (random input lines, `\n` and `\r\n` terminators, unterminated last line, end of input, and the text-to-number rule). Not covered: lists, and rhyme in the typed generator's control-flow programs (rhyme is fuzzed with pragma-steered stanzas instead).
  - Scale a run with `SYNTAXEON_FUZZ_ITERS` and `SYNTAXEON_FUZZ_SEED` (`cargo test --release -p syntaxeon-cli --test fuzz_programs`); the `explore_*` tests are ignored and print what a run covers.

## Later


- `repl`.
- Rhyme: consonance and rhyme across different syllable counts are not accepted; add them as further `rhythm.toml` keys if a project asks (ADR-015).
- Meter, then stress patterns, then alliteration (the lexicon already carries stress).
- Records, modules, first-class functions, anaphora (excluded from v0 by ADR-010).
- Language server: built (`syntaxeon-lsp`, ADR-020). Definition and completion are in. Next: rename and references (the name index exists); block-accurate scopes for completion; the `syntaxeon-vscode` extension (its own repository) with the audio layer.
- Execution trace (enter/leave stanza and block, output, error) for the VM (ADR-021), so the extension can sonify a run.
- Prosody v2: syllables and stress as information first, then as a constraint.

## Tooling

- Tool pins that Dependabot does not track: `just` 1.59.0, `cargo-deny` 0.20.2 and `git-cliff` 2.14.2 in the workflows. Bump them together when one moves.
- Confirm the macOS job of CI is green after the first full run.
- Machine-checked file names in `lint` (`ls-lint`).

## Distribution

- crates.io publication of the workspace crates (metadata and workspace dependency versions are ready; the publish step and name check are not).
- Homebrew tap.
- A banner for the README.
