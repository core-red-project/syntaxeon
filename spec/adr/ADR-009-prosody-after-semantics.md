# ADR-009: Prosodic validation runs after semantic analysis

Status: **accepted** (2026-11-05)

## Context

A program can be both meaningless and unmetrical; the meaning error is the more useful one.

## Decision

Prosody is a post-semantic pass, not a parse-time filter. Diagnostics order: lexical, grammatical, semantic, prosodic.

## Consequences

Charter §3, §13-D3. Revisit if stanza scoping needs parse-time prosody.
