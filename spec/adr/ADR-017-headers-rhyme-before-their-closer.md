# ADR-017: Headers rhyme on the words before their closer

Status: **accepted** (2026-11-06)

## Context

Headers counted for rhyme with their last word as written, so `If …, then:` always ended in `then`, `As long as …, do:` in `do`, `Do this … times:` in `times` and a function header in its type word. The audit (`docs/AUDIT.md` §4) found that no two of those words rhyme with each other, that `number` has three common perfect partners, and that on generated programs only 35 % could rhyme at all by renaming, because two header verses in one group are impossible. Exempting headers would reopen the loophole `PROSODY.md` §6 closes (a condition escaping the rhyme check).

## Decision

A header's end word is the last word before its fixed closer (`, then`, `, do`, `times`), and a function header's is the last word of its title. Stanza headers already ended in their title. Headers still count, on words the author chooses.

## Consequences

No header is a fixed ending any more, so every stanza can be made to rhyme by naming and number choice alone. The bundled examples and the benchmark fixture were rewritten to rhyme under the rule (outputs of `fizzbuzz` and `powers` changed where a printed string had to rhyme; the others print what they did). A header split over several lines rhymes on the line that holds the closer. Not changed: how a function header's parameter list or return type rhymes (it does not).
