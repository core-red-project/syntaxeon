# ADR-016: Every pair in a rhyme group must rhyme

Status: **accepted** (2026-11-06)

## Context

`PROSODY.md` §7 checked each group against its first verse only, on the grounds that it kept the check linear. The audit (`docs/AUDIT.md` §5) showed the reason does not apply, because a group has at most three verses, and that the rule is asymmetric: among triples drawn from one tail group of common words, 32 % (perfect rhyme) or 3 % (assonance) got a different verdict depending on which verse led. The cause is always an identical pair: `men` and `amen` do not rhyme, yet `ten, men, amen` passed (leader `ten`) while `men, ten, amen` failed.

## Decision

A stanza is valid iff some permitted scheme has, in every group, a rhyme between every two verses. The verdict no longer depends on verse order. A failing verse is reported against the earlier members of its group.

## Consequences

Strictly stronger: some stanzas that passed (groups of three containing an identical pair, with a third verse leading) now fail with `SXP-202`. Couplets are unchanged, since a pair is one comparison. Programs that rhyme under the old rule and have no identical pair in a triple are unaffected; none of the bundled examples changed. The oracle in `crates/syntaxeon-lexicon/tests/fuzz_rhyme.rs` and the program fuzzer check the new rule independently.
