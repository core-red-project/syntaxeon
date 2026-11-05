# ADR-005: Multisyntax via a closed normalizer to a canonical AST

Status: **accepted** (2026-11-05)

## Context

Several surface forms must denote one computation (`This integer shall become one.` / `Let this integer become one.` / inverted order).

## Decision

The parser yields a surface tree; a rule-table normalizer maps it to the canonical AST. Semantics and tooling see only the AST. Grammar is closed and deterministic (no ML/statistical parsing). Keywords are roles drawn from closed, versioned verb sets, not reserved words. A sentence with more than one valid normalization is a compile error (SXP-1xx), never a silent pick. Rules are data, not scattered code.

## Consequences

Charter §6, Laws 2-3. `fmt` may reorder only within one equivalence class.
