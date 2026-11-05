# Roadmap

Open work, most important first. Shipped changes are in [CHANGELOG.md](../CHANGELOG.md); decisions are in [spec/adr/](../spec/adr/).

## Next

- **Real programs.** Write programs of 20–40 lines and see what breaks. The rule that headers must rhyme multiplies with nesting, and nobody has yet judged whether that is usable.
- **Ambiguity fuzzing.** `SXP-104` is the grammar's main risk (names near reserved words, comparisons mixed with `both`/`either`); generate sentences and look for unexpected double readings.
- **Normalizer as data.** ADR-005 asks for rules in tables; they live in parse actions today.
- **Expression spans.** Diagnostics point at a statement's line; expressions carry no position.

## Later

- `fmt`: reorder only inside the equivalence classes of the grammar and never change a rhyme verdict.
- `repl`.
- IR and VM (ADR-013 deferred them); `--dump-ir`.
- Meter, then stress patterns, then alliteration (the lexicon already carries stress).
- A `rhyme` analysis command beyond what `explain` shows.
- Records, modules, first-class functions, anaphora (excluded from v0 by ADR-010).
- Language server.

## Distribution

- crates.io publication of the workspace crates (metadata and workspace dependency versions are ready; the publish step and name check are not).
- Homebrew tap.
- A banner for the README.
