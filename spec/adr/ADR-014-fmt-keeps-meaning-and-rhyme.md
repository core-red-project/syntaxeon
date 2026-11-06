# ADR-014: `fmt` rewrites only what provably keeps meaning and rhyme

Status: **accepted** (2026-11-05)

## Context

Charter §12: `fmt` may re-order only within the equivalence classes of §6 and must never change a rhyme verdict. The forms of one declaration or assignment (`PROSE_GRAMMAR.md` §6.1–§6.2) are one class, but they end in different words (`… become ten.` / `ten shall the pile become.`), so a re-order can change which words rhyme.

## Decision

- `fmt` formats only programs that parse and type-check. Whitespace is always normalized (no trailing spaces, at most one blank line, one final newline).
- A declaration or assignment is re-spelled to the earliest form of its class in the `FORMS` table (`D1`, then `D2`, then `D3`; `A1`, `A2`, `A3`) only if the whole program, re-analysed, has no diagnostics and the same AST (positions aside). The grammar proposes spellings; the pipeline decides.
- A program that does not rhyme gets whitespace only: a verdict that is already "invalid" cannot be shown to be kept.
- A sentence may span several lines; `fmt` joins it, proposes the spelling, and the same check decides (joining changes the verse count, so it is usually refused when the lines rhyme on their own). Sentences with an aside are left alone, so no comment is ever dropped. Passes repeat until nothing changes, so the result is stable.
- Only classes with more than one spelling are touched: declarations, assignments, and `Give back` / `Return` (with or without `nothing`). Block headers, `Speak`/`Proclaim` (different builtins) and `Call` have a single form each.

## Consequences

`fmt` cannot make a valid program invalid, by construction. It does not fix rhyme or reflow sentences. Cost is one full analysis per proposal, kept small by sharing the lexicon between clones.
