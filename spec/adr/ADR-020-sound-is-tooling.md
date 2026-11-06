# ADR-020: Sound is part of the developer experience, not of the language

Status: **accepted** (2026-11-06)

## Context

The planned `syntaxeon-vscode` extension would recite verses, play the two words of a failed rhyme, give each kind of error its own sound and accompany an execution. The question was whether sound belongs to the language (programs that produce sound), to the tooling (any program can be heard), or both.

## Decision

Sound is **tooling** (option A). The language does not change.

- The language server reports linguistic facts only: diagnostics with ranges, and structured data about each verse (end word, pronunciation as ARPAbet, rhyme tail, rhyme group, and the kind of rhyme between verses). It never generates or plays audio.
- Speech, MIDI and playback live in the extension (or an audio engine it drives), which turns those facts into sound. Playback profiles (recitation, dramatic, metric, orchestral, silent) are extension settings. They never go in `rhythm.toml`, which changes validity; nothing audible may change a verdict (Law 5).
- What is recited is the pronunciation the compiler judged, from the pinned lexicon (ARPAbet), so that what is heard is what was checked. A system speech engine may be offered as flavour, never as the reference.
- Hearing an execution is driven by an event trace from the evaluator or the VM (enter and leave a stanza or a block, text printed, error), not by any new construct in programs.
- Sound as a language feature (a program that plays notes) is **not decided**. If it comes, it is a new ADR with its own semantics, fuzz model and an output device; the extension would already know how to play it.

## Consequences

The server crate needs a rhyme *kind* (perfect, assonant, identical, none), a per-verse phonetic report and an execution trace, none of which changes the language. Metric information (syllables, stress) can be offered as information; a metric *warning* needs prosody v2 first (today every diagnostic is an error, Law 4).
