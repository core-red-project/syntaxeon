# ADR-018: Letter codes, an end-of-input test and a number test

Status: **accepted** (2026-11-06)

## Context

The audit (`docs/AUDIT.md` §1, §3) found three things no program could express: turning a letter into its code and back (so no case conversion, no ciphers), asking whether input remains (so no filters: `hear` at the end of input is `SXP-505` and ends the program), and checking that a text is a number before converting it (a bad `to_integer` is `SXP-506` and ends the program). All three are pure questions; none needs an exception mechanism.

## Decision

Four closed prose forms, four builtins (`SEMANTIC_CORE.md` §8, `PROSE_GRAMMAR.md` §7):

- `the code of E` and `the letter with code E`: `to_code` and `to_letter`, between `Character` and `Integer` (Unicode scalar values; an invalid code is `SXP-506`).
- `there is more to hear`: `has_input`, a `Boolean` that consumes nothing.
- `E reads as a whole number`: `reads_as_integer`, true exactly when `to_integer` would succeed.

`hear` is now a reserved word, because the phrase `there is more to hear` would otherwise also read as a comparison of two names (`there` is `more to hear`); the fuzzers found that ambiguity the first time the form ran. Substring, case conversion, splitting, searching and sorting stay out of the language: they are now writable with these primitives.

## Consequences

Filters (`cat`, a column sum that skips bad lines) and character programs (upper case by subtracting 32) can be written. The scalar fuzz model gained letters, truths and the input tests, and checks them (and the edges of what is a letter) against the evaluator. A condition built from these forms ends in `hear` or `number` as a header end word, which are ordinary words to rhyme with.
