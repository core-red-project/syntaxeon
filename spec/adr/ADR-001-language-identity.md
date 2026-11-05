# ADR-001: Language identity: prose and poetry are both syntax

Status: **accepted** (2026-11-05)

## Context

Syntaxeon needs a property that survives removing all aesthetics. Option A (prose is syntax) yields a controlled natural language. Option B adds prosodic constraints to validity.

## Decision

Option B. A program is valid only if its prose parses AND its prosodic constraints hold. A prosodic violation is an error, never a warning.

## Consequences

Charter §0, Law 4. Rhyme is a language feature, not a compiler lint. Costs: grammar and lexicon work are much larger than A.
