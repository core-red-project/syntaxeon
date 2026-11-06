# ADR-019: Words that start a prefix form can still name variables

Status: **accepted** (2026-11-06)

## Context

After `the`, a name could not begin with `sum`, `difference`, `product`, `quotient`, `remainder`, `text`, `whole`, `fraction`, `length`, `negation` or `letter`, so a programmer could not call a variable `sum`, `length` or `text`, and the error did not say why (`docs/AUDIT.md` §8).

## Decision

Those words are free to name things. After `the`, a name is refused only when the words at that position are the whole lead of a prefix form (`sum of`, `text of`, `whole number of`, `letter with code`, …) or `letter` followed by a quoted string (the letter literal). The set of refused names is derived from the prefix-form tables, so a new prefix form protects itself. The other reserved words (`item`, `call`, `give`, `return`, `times`, …) stay reserved: they are the grammar's own words.

## Consequences

`the sum`, `the length`, `the text`, `the sum total` are variables. A variable named like a prefix form cannot be written with `the` directly before `of` (`item the sum of xs` reads as the prefix form); write it without the article or rename it. The ambiguity fuzzers (900 000 generated sentences over 3 seeds, with these names added to their vocabulary) found no sentence with two readings; the generator avoids the one case above on purpose.
