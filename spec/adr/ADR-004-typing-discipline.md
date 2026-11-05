# ADR-004: Typing: static and strong

Status: **accepted** (2026-11-05)

## Context

Surface syntax is already ambiguous by design; types must not add dynamic ambiguity.

## Decision

Static, strong types: Integer, Float, Boolean, String, Character, List<T>, function types. No implicit conversions. Inference is local; declared types at function boundaries.

## Consequences

Charter §4, Law 8.
