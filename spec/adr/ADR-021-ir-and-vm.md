# ADR-021: `run` executes an IR on a VM; the tree evaluator stays as the reference

Status: **accepted** (2026-11-06); completes ADR-003, supersedes the "deferred" part of ADR-013

## Context

ADR-013 deferred the IR until there was a reason for it. There are now three: a run needs an
execution trace for the editor extension (ADR-020), which is one place to emit in a VM and many
in a recursive walk; deep recursion should not depend on a 256 MiB native stack; and a flat,
dumpable form lets anyone see what a stanza turned into (`--dump-ir` in the charter).

## Decision

- `syntaxeon-eval` gains `ir` (lowering and listing) and `vm`. `lower` turns a checked program
  into one flat list of stack operations per stanza: names are slots, `If`/`While`/`Repeat`/
  `and`/`or` are jumps, a `repeat` count lives in a hidden slot, every operation carries its
  statement line and expression span.
- The VM runs on an explicit stack of frames. The 10 000-frame limit (`SXP-504`) is unchanged.
- `syntaxeon run` uses the VM. `syntaxeon ir <file>` prints the listing (a command, like `ast`,
  not a flag, to keep the CLI one shape).
- The tree evaluator is kept as the reference. Values, operators and builtins live once, in
  `rt.rs`, and both engines call them, so they cannot drift in what an operation means.
- The two must be indistinguishable: same output, same error code, line and span. Every
  evaluator test runs both engines and compares; the shipped programs are compared too, and the
  whole-program fuzz suite runs the VM against its independent model.

## Consequences

- Making the engines agree found one real bug in the evaluator: the condition of a `While` was
  evaluated on later passes with the line and span of the last statement of the body. It now
  reports the statement itself.
- Builtins are called by name (`rt::builtin` matches a string); an index would be faster. Marked
  as the place to look if a run needs to be quicker.
- Lowering keeps one slot per declaration and never reuses slots; a function with many
  declarations has many slots. Fine at this size.
- The trace for the audio layer is added to the VM only.
