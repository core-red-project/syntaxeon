# Benchmarks

Measured with `just bench` (requires [hyperfine](https://github.com/sharkdp/hyperfine)). It builds `syntaxeon` in release mode and times three commands:

```bash
hyperfine --warmup 3 \
  'target/release/syntaxeon --version' \
  'target/release/syntaxeon check benches/fixture.sxe' \
  'target/release/syntaxeon run benches/fixture.sxe'
```

`benches/fixture.sxe` runs a loop 100 000 times that prints two lines per pass, so `run` measures the evaluator and output, and `check` measures the whole front end including loading the pronunciation lexicon.

`bench` is not part of `just check`: timings are noisy.

## Baselines

No baseline is recorded yet: hyperfine was not installed on the machine where the benchmark recipe was written. A single unmeasured `time` run on an Apple silicon laptop (2026-11-05) gave about 0.003 s for `--version` and 0.13 s for `run benches/fixture.sxe`; treat these as a rough order of magnitude, not a baseline.

When a baseline is recorded, add a row with the date, machine, exact command and means, and note any release that is more than 10 % slower than the previous row in the changelog.

| Date | Machine | Command | Mean |
|---|---|---|---|
| — | — | — | — |
