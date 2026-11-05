# ADR-007: Pronunciation comes from a pinned, vendored lexicon

Status: **accepted** (2026-11-05)

## Context

Rhyme needs pronunciation; host-dependent pronunciation would break determinism (Law 5).

## Decision

The spec pins a CMUdict-class ARPAbet snapshot by content hash and vendors it. Rhyme = equal phoneme sequence from the last stressed vowel onward, specified as a pure function with test vectors. Unknown word is error SXP-3xx (unpronounceable), never a guess. A `pronounced` pragma declares a local pronunciation. English only in v1.

## Consequences

Charter §8, §13-D2. Highest-risk ADR: if it fails, determinism fails.
