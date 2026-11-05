# ADR-003: Execution model: compile to IR, run on a VM

Status: **accepted** (2026-11-05)

## Context

Explicit phases are a feature: lexical, literary, syntactic, normalization, semantic, prosodic, lowering, execution.

## Decision

Compile to a Syntaxeon IR executed by an IR interpreter/VM; native targets are optional and later. Each phase is its own module, individually testable and dumpable (`--dump-ast`, `--dump-ir`, `explain`).

## Consequences

Charter §3, §12. Implementation baseline: Rust workspace, crate per phase, mirroring Obfusku.
