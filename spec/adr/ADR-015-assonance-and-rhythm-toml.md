# ADR-015: Assonance is valid rhyme, and a project can turn it off in `rhythm.toml`

Status: **accepted** (2026-11-06)

## Context

ADR-001 makes prosody part of validity. Until now only perfect rhyme counted, so the language rejected near-rhymes such as `men` / `bed` that most poets accept. Loosening validity changes which programs are programs, and some projects will want the strict language back.

## Decision

- Assonance (`PROSODY.md` §4.1: the same vowels from the stressed vowel on, consonants free, never an identical rhyme) is valid rhyme **by default**.
- A project sets `[rhyme] assonance = false` in a `rhythm.toml` found like `rustfmt.toml` (`PROSODY.md` §12) to accept only perfect rhyme. The file is part of the project and is versioned with it, so the verdict stays deterministic (Law 5): no host setting, locale or flag changes it.
- The file accepts nothing it does not know (`SXP-305`). It has one key now; others (for example a consonance level) may be added by a later ADR.
- A word never rhymes with itself, whatever its alternative pronunciations do to each other. Fuzzing showed that assonance, unlike perfect rhyme, made that case reachable.

## Consequences

Every program valid under perfect rhyme stays valid; some invalid ones become valid. The stanza schemes (§7) are unchanged. Assonance is a weak constraint (a given stressed vowel is shared by many words), which is why turning it off is a one-line change. `rhymes --near` is only a search aid and never decides validity. Not decided: consonance, rhyme across different syllable counts, per-file or per-stanza overrides.
