# ADR-006: Prosody scope for v1: end rhyme and stanza-as-block only

Status: **accepted** (2026-11-05)

## Context

Rhyme + meter + stress + alliteration at once would make the parser intractable.

## Decision

v1 enforces end rhyme within a stanza and uses blank-line-delimited stanzas as the block/scope unit; the stanza's rhyme scheme constrains its line count. Meter (v2), stress (v3), alliteration/cadence (v3+) are deferred. Whether rhyme carries semantics beyond validity is deferred until the v1 grammar exists.

## Consequences

Charter §7, §13-D4. Block structure and prosody are one mechanism.
