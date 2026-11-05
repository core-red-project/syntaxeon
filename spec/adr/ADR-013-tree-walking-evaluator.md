# ADR-013: v0 executes the canonical AST with a tree-walking evaluator

Status: **accepted** (2026-11-05); partially supersedes ADR-003

## Context

ADR-003 chose compile-to-IR plus a VM. The IR would currently be a re-encoding of an AST with
six statement kinds and no closures, and it would add a lowering phase with no semantic work to
do. Rhyme, grammar and typing are the language's risks, not execution speed.

## Decision

`syntaxeon-eval` interprets the checked canonical AST directly. The phase order in ADR-003 is
otherwise unchanged. An IR and VM are deferred until there is a reason for them (performance,
a second target, serialization of compiled programs); the evaluator's semantics are defined
by `SEMANTIC_CORE.md` §6, so an IR must preserve them, including the 10 000-frame limit.

## Consequences

- The evaluator recurses on the native stack. The CLI runs everything on a 256 MiB thread so
  the frame limit (`SXP-504`) is hit before the native stack is exhausted; library users must
  do the same.
- `--dump-ir` from the charter does not exist yet; `ast` is the only dump.
- A program that did not pass `syntaxeon-check` reports `SXP-599` rather than panicking.
