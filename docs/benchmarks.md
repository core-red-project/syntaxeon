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

Recorded with the exact command above on release builds (`just bench`, hyperfine 2.0.0, 3 warm-up runs). Memory is the peak resident set reported by hyperfine.

| Date | Machine | Command | Mean | Memory |
|---|---|---|---|---|
| 2026-11-06 | Apple M4 Pro, 24 GiB, macOS 26.6 | `syntaxeon --version` | 2.0 ms ± 0.3 | 1.6 MiB |
| 2026-11-06 | Apple M4 Pro, 24 GiB, macOS 26.6 | `syntaxeon check benches/fixture.sxe` | 53.6 ms ± 1.3 | 72.3 MiB |
| 2026-11-06 | Apple M4 Pro, 24 GiB, macOS 26.6 | `syntaxeon run benches/fixture.sxe` | 105.0 ms ± 2.3 | 72.3 MiB |

`check` is dominated by building the pronunciation lexicon (about 135 000 entries), hence the 72 MiB; `--version` does not touch it. A release that is more than 10 % slower than the previous row needs a note in the changelog.
