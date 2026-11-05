# ADR-011: Entry point is the first stanza, which must be titled

Status: **accepted** (2026-11-05)

## Context

A valid program needs exactly one entry point (Charter §10).

## Decision

The first stanza, introduced by a title line, is the entry. Other stanzas are named functions. Zero or multiple entries is a semantic error.

## Consequences

Charter §10, §13-D7. Title-line grammar is defined in PROSE_GRAMMAR.md.
