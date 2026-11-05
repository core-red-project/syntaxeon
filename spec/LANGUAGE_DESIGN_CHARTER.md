# Syntaxeon — Language Design Charter

Status: **draft 0** — nothing here is frozen until its decision is marked
`DECIDED`. No grammar is written before §13 is closed.

The POC in `~/PycharmProjects/Syntaxeon` is historical evidence of intent,
not a constraint. Its EBNF is a keyword table with synonym lists
(`"Sing" | "Chant"`, `"Else the story unfolds:"`): it is Python with
fantasy-flavored keywords. It has no prose grammar, no prosody, and no way to
say why a program is *invalid* for literary reasons. Syntaxeon proper
starts from the opposite end.

---

## 0. The identity test

> What property makes Syntaxeon *Syntaxeon* if all aesthetics are removed?

> **A language in which natural-language-like prose encodes computation,
> while rhetorical and prosodic constraints participate in syntactic
> validity.**

Two claims, both load-bearing:

- **A. Prose is syntax.** Sentences have a grammar that maps to computation.
- **B. Poetry is syntax.** Rhyme, meter and stanza structure are *also*
  constraints on whether a program is well-formed.

**DECIDED: B.** A is a controlled natural language; B is the language that
deserves to exist. A program that does not rhyme is not a program with a
warning — it is not a program.

## 1. Design Laws

Normative, same status as Obfusku's laws.

1. **Prose is the only surface.** No hidden symbolic escape hatch.
2. **Controlled, not natural.** The grammar is closed and deterministic.
   No ML, no statistical parsing, no "understanding". Accepting a sentence
   means a rule matched it.
3. **Surface does not determine semantics.** Many surface forms normalize
   to one AST (§6). Semantics is defined on the AST only.
4. **Prosody is validity, not lint.** A prosodic violation is an `error`.
5. **Prosody must be deterministic.** Pronunciation, syllable count and
   stress come from a *versioned lexicon pinned by the spec* (§8). The
   host's locale, OS, or any online service must never change the verdict.
6. **Computation is not literature.** Semantics stays ordinary and typed;
   literature is the encoding, not the machine.
7. **Every complexity must serve the gimmick.** No feature because other
   languages have it.
8. **Static types, deterministic evaluation.** Inherited from Obfusku
   laws 7–9.
9. **The spec is authoritative; the implementation is subordinate.**
10. **Errors are literary in voice and mechanical in structure.** Every
    diagnostic has a stable code, category and severity (§11).

## 2. Paradigm — DECIDED

**Procedural, imperative, expression-oriented.**

A program is a sequence of *acts* (declare, change, test, repeat, return).
Expressions compose inside prose ("the sum of two and three" is a value).
No pure-functional core: it would not serve the gimmick.

Mutability: bindings are immutable unless declared otherwise in the prose
("shall change", "ever-changing"); the exact wording is a §6 matter.

## 3. Execution model — DECIDED

**Compiled to a Syntaxeon IR, then executed by a VM** (interpreter over the
IR first; native later, optional).

```
Source
  → Lexical analysis      words, numbers, punctuation, quoted text
  → Literary analysis     lines, verses, stanzas, sentence boundaries
  → Syntactic analysis    prose grammar → surface tree
  → Normalization         surface tree → canonical AST (multisyntax)
  → Semantic analysis     names, types
  → Prosodic validation   rhyme, meter, stanza constraints
  → Lowering              AST → IR
  → Execution             IR VM
```

Phases are explicit crates/modules and individually testable. `explain`
(§12) prints the output of any phase.

Ordering note: prosodic validation runs after semantic analysis so a
program that is both wrong and unmetrical reports the *meaning* error first.
Open: whether prosody can instead be a parse-time filter (§13-D3).

## 4. Typing — DECIDED

**Static, strong.** Core types: `Integer`, `Float`, `Boolean`, `String`,
`Character`; `List<T>`; function types. No implicit conversions. Type
inference at local scope only; declared types at function boundaries.

## 5. Memory model, data, control, functions — provisional

- **Memory**: automatic; reclamation strategy is not observable.
- **Data**: the types in §4 plus user records (open: §13-D5).
- **Control**: conditional, bounded and conditional loops, match. Spoken as
  prose, but each is an ordinary structure with ordinary semantics (Law 6).
- **Functions**: introduced by an infinitive clause —
  *"To greet the one who enters, speak 'Hello'."* — derived from the same
  grammar as every other act, not a special form.
- **Modules**: out of scope for v0.

## 6. Prose grammar and multisyntax

Three syntactic layers:

| Layer | Question it answers |
|---|---|
| Lexical | what are the atoms? |
| Grammatical | which roles do they play? (`[subject] [type] [declaring verb] [value]`) |
| Prosodic | does the verse obey its form? (§7) |

**Keywords are roles, not reserved words.** A *declaring verb* is any member
of a closed, spec-defined set of verbs; a *comparator* is any member of
another. The sets are part of the spec and versioned. Contrast with the
POC, whose synonym lists were the entire grammar.

**Multisyntax.** These are three surface forms of one AST node:

```
This integer shall become one.
Let this integer become one.
One shall this integer become.     (inverted order)
          ⇒ Declaration(type=Integer, name=…, value=1)
```

Therefore: the parser produces a *surface tree*; a **normalizer** maps
it to the canonical AST. All semantics and all tooling downstream see only
the canonical AST. The normalizer's rule set is the most technically
interesting part of Syntaxeon and must be specified as data (rule tables),
not scattered procedural code.

Constraint to keep ambiguity honest: a sentence with two valid
normalizations is a **compile error** (`SXP-1xx ambiguous verse`), never a
silent pick.

## 7. Prosodic constraint system

v1 enforces **exactly two** constraints; everything else is deferred so the
parser does not need therapy.

| Constraint | v1 | Later |
|---|---|---|
| End rhyme within a stanza | **yes** | |
| Stanza boundaries (blank line) delimit scope/blocks | **yes** | |
| Meter / syllable count per line | | v2 |
| Stress pattern | | v3 |
| Alliteration, repetition, cadence | | v3+ |

Example:

```
The counter shall rise tonight,
until its final value comes to light.      ← rhymes with "tonight": valid
```
```
The counter shall rise tonight,
until its final value reaches ten.         ← ERROR SXP-202 expected /aɪt/
```

**Stanza = block.** A stanza is the unit of scope and of rhyme. The rhyme
scheme (AA, ABAB, …) is a property of the stanza and constrains how many
lines it may contain, so block structure and prosody are *the same
mechanism* instead of two layers glued together. (Candidate to make
rhyme semantically load-bearing in a later version, see §13-D4.)

## 8. The determinism problem (the real risk)

Rhyme requires pronunciation. Pronunciation is an implementation accident
unless we pin it.

- The spec ships (or pins by hash) a **phonetic lexicon** (CMUdict-class,
  ARPAbet). The compiler uses only that lexicon.
- Rhyme = equality of the phoneme sequence from the last stressed vowel
  onward. Defined in the spec as a pure function, with test vectors.
- **Unknown word** → error `SXP-3xx unpronounceable`, not a guess. A
  `pronounced` pragma lets the program declare a pronunciation locally, so
  user identifiers can still rhyme.
- Language: **English only** for v1. Other languages = different lexicon
  + different grammar = a different dialect, out of scope.

If this section is not solved, Law 5 is false and the language is a toy.

## 9. Identifiers

Names are common-noun phrases ("the counter", "this integer") or quoted
names. Identifier resolution is part of semantic analysis, not parsing, so
that the same noun phrase can appear in different surface positions.
Open: anaphora ("it", "the former") — excluded from v0 (§13-D6).

## 10. Program validity

A source file is a valid Syntaxeon program iff **all** of:

1. it lexes and parses into a surface tree;
2. every sentence normalizes to exactly one AST node (§6);
3. it type-checks (§4);
4. every stanza satisfies its prosodic constraints (§7);
5. it contains exactly one entry point (§13-D7).

## 11. Diagnostics

Dual-layer, as in the pasted design:

```
code:     SXP-301          ← stable, machine-readable
category: prosodic         ← lexical | grammatical | semantic | prosodic
severity: error

  The verse hath failed to rhyme.
    expected /aɪt/   found /ɛn/      line 14, verse 2
```

Literary voice is a *renderer* over the structured record. `--json` output
(Obfusku CLI contract) emits only the structure.

## 12. Tooling

Binary: `syntaxeon`. Subcommands: `build`, `run`, `check`, `fmt`, `repl`,
`explain`, plus `--dump-ast` / `--dump-ir`, and `rhyme` (analyze a file's
rhyme classes). `explain` prints the phase tree for a verse (declaration →
type/name/value → rhyme constraint status) and is the flagship debug tool.

`fmt` is non-trivial here: it may re-order only within the equivalence
classes of §6 and must never change a rhyme verdict.

Implementation baseline (mirrors Obfusku): Rust Cargo workspace, crates per
phase (`core`, `diagnostics`, `lexicon`, `syntax`, `normalize`, `prosody`,
`typecheck`, `ir`, `vm`, `fmt`, `cli`), `spec/` + `spec/adr/ADR-NNN-*.md`,
`just check` as the single gate, Conventional Commits.

## 13. Decisions D1–D9 (all closed by ADR-001…012, see `spec/adr/`)

The recommendations below were accepted as written. Mapping: D1→ADR-008,
D2→ADR-007, D3→ADR-009, D4→ADR-006, D5/D6→ADR-010, D7→ADR-011,
D8/D9→ADR-012. Paradigm, execution, typing, multisyntax and identity are
ADR-001…005.

| # | Decision | Recommendation |
|---|---|---|
| D1 | File extension | `.sxe` (matches the design notes; POC used `.syn`) |
| D2 | Pronunciation source | pin a CMUdict snapshot by hash, vendored |
| D3 | Prosody as parse-time filter vs. post-semantic pass | post-semantic (§3) |
| D4 | Does rhyme carry *semantics* (e.g. rhyming lines are bound / run as a unit) beyond validity? | not in v1; decide after v1 grammar exists |
| D5 | User-defined records/types in v0? | no |
| D6 | Anaphora ("it") | no in v0 |
| D7 | Entry point form (a titled opening stanza? last stanza?) | titled first stanza |
| D8 | Comment syntax (parenthetical aside? "Prologue:"?) | spoken asides, not rhyme-checked |
| D9 | Mutability wording | closed verb set: *shall be* (immutable) / *shall become* (mutable) |

## 14. Roadmap

1. Close §13, write ADRs for each `DECIDED` item.
2. `spec/SEMANTIC_CORE.md` — canonical AST + type rules (before grammar).
3. `spec/PROSE_GRAMMAR.md` — roles, verb sets, normalization tables.
4. `spec/PROSODY.md` — lexicon, rhyme function, test vectors.
5. Vertical slice: one program that declares, loops, prints, and rhymes,
   through every phase to the VM.
