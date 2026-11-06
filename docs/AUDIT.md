# Language audit — 2026-11-06

What the language can and cannot do today, measured rather than assumed. Each finding says how it was checked and what it recommends. Nothing here changes the language except where marked **done**; the recommendations are decisions for the project. Decisions go to `spec/adr/`, open work to [ROADMAP.md](ROADMAP.md).

## 1. Library

**Checked** by writing realistic programs and asking the compiler whether they pass syntax and types (rhyme aside).

| Program | Result |
|---|---|
| reverse a text, split on a letter, remove an element, bubble sort, list of lists | expressible, in loops |
| digit value of a character (`the whole number of the text of c`) | expressible |
| `abs`, power, text ordering (`is less than` on texts), float arithmetic | expressible |
| code of a character, or a character from a code | **impossible**: `to_integer` of a `Character` is `SXP-402` |
| upper/lower case of any character | impossible without a 26-branch table (needs the two above) |
| read until the end of input | **impossible** (see §3) |
| check that a line is a number before converting | **impossible**: a bad `to_integer` is `SXP-506` and ends the program |

So the library is *tedious* (no substring, find, split, sort, remove, `abs`/`min`/`max`/power) but only three things cannot be written at all, and they are primitives, not conveniences.

**Done (ADR-018).** The three primitives exist: `the code of E` / `the letter with code E`, `there is more to hear`, and `E reads as a whole number`. Everything else on the list is writable with them, and each builtin costs a prose form and a rhyming verse, so the set stays small (Law 7).

## 2. `break`, `continue`, `If` as an expression

**Checked.** A function leaves a loop with `Give back` (`find` over a counter works); the entry stanza leaves with `Return nothing` inside a loop; `continue` is an `If` around the rest of the body; a value needing an `If` is computed into a local. All type-check.

**Recommendation.** No change. The workarounds are ordinary and the decision in `SEMANTIC_CORE.md` §11 stands.

## 3. Input and output

**Checked.** The program `echo` reads a fixed number of lines; `hear` at end of input is `SXP-505` and ends the program; there is no way to ask whether input remains. No arguments, files or recoverable errors exist, by design (`SEMANTIC_CORE.md` §11).

**Finding.** Without an end-of-input test no filter program (`cat`, `wc`, sum a column) can be written, and without a numeric check every program that reads numbers dies on bad input. Arguments and files can stay out for an esolang; these two cannot.

**Done (ADR-018).** An end-of-input test and a "reads as a whole number" test, both pure questions, so no exception mechanism is needed.

## 4. Verse endings the grammar forces

**Checked** in three ways.

1. *Which fixed endings can pair.* Header end words are `then`, `do`, `times`, and a type word (`number`, `fraction`, `truth`, `text`, `letter`). No two of them rhyme perfectly (and a word never rhymes with itself), so two headers can never share a couplet. Assonance adds only `then`/`text` and `do`/`truth`.
2. *How many partners a forced word has* (common words, perfect rhyme): `number` 3 (`lumber slumber encumber`), `truth` 4, `text` 5, `fraction` 8, `letter` 7, `times` 9, `then` 13, `do` 100+. With assonance every one has 240–950.
3. *Feasibility on generated programs* (`explore_feasibility` in `fuzz_programs.rs`; 2 000 random typed programs; every ending that is not a header word assumed freely renamable, which is optimistic): **35 % of the programs and 62 % of the stanzas can rhyme at all**. A stanza fails when two header verses fall in the same rhyme group, which dense nesting does constantly (`If … then:` straight after `If … then:`). The generator nests far more than people do, so read this as a worst case, not a typical case.

**Finding.** The constraint is real and structural, not only poetic: some correct programs cannot be made to rhyme by renaming, they need an inserted filler line.

**Options.**
- *A. Header end word = the last word before the closer* (`If the pile is ten, then:` ends in `ten`; a function header ends before `, giving …`). Headers keep taking part in rhyme, so the loophole `PROSODY.md` §6 closes stays closed, and they stop being fixed. Natural as a `rhythm.toml` key.
- *B. Exempt headers.* Reopens the loophole; not recommended.
- *C. Keep.* Honest if the difficulty is the point; then publish the filler idiom.

**Done (ADR-017).** Option A, as the rule of the language rather than a key: a header rhymes on the words before its closer, a function header on its title. The examples and the fixture were rewritten to rhyme. The feasibility measurement above was of the old rule and no longer applies (nothing is fixed now).

## 5. First-member grouping

**Checked.** A group is validated against its first verse. Groups have at most three verses, so the stated reason (a linear check) does not apply. Over random triples drawn from one tail group of common words, **32 % (perfect rhyme) or 3 % (assonance) get a different verdict depending on which verse leads**. The cause is always an identical-rhyme pair: `men`/`amen` do not rhyme, yet `ten, men, amen` passes (leader `ten`) while `men, ten, amen` fails. (That test, `first_member_grouping_depends_on_which_verse_leads`, now asserts the opposite: `a_group_is_valid_only_if_every_pair_rhymes`.) The feasibility figure in §4 was re-measured under the pairwise rule and did not change (707 of 2 000 programs, 3 079 of 5 000 stanzas).

**Done (ADR-016).** Every pair in a group must rhyme. It is symmetric, costs at most three comparisons, and rejects only groups that contain an identical pair, which a reader would call a rhyme failure anyway.

## 6. Strength of the rhyme

**Measured** on common words. A random pair rhymes perfectly 0.04 % of the time (0.10 % among the most common words) and by assonance 1.25 % (2.34 %): assonance multiplies the partners of a word by 20–130 (`then` 13 → 947, `love` 5 → 636).

**Finding.** Perfect rhyme is a demanding constraint, assonance a light one. Both are plausible as defaults; the project file (ADR-015) lets each project choose.

**Recommendation.** Keep as is. If a middle level is wanted, a `rhythm.toml` key for assonance restricted to the same final consonant is the cheapest one.

## 7. Numerals

**Checked.** The end-word algorithm was only specified by example. It is now written out (`PROSODY.md` §5) and verified against an independent construction of the whole English cardinal for 0…400 000, for every power-of-ten boundary up to 2⁶⁴ − 1, and for 200 000 random values (`crates/syntaxeon-lex/tests/numerals.rs`). No defect found. **Done.**

## 8. Identifiers

**Checked.** 91 of 38 656 common words (0.24 %) cannot be a one-word name: the reserved words and number-words, but also nouns a programmer would reach for: `sum`, `product`, `difference`, `quotient`, `remainder`, `length`, `text`, `fraction`, `letter`, `item`, `call`, `give`, `return`, `times`, `least`, `most`. Any longer name containing one fails too (`sum total`, `item count`). `_` and digits are not letters, so they cannot appear (by design: names are prose).

**Finding.** The restriction is small, but the old error for all of these was the same "no grammar rule matches this sentence". **Done:** the error now names the word (`it would read if 'sum' were an ordinary word, but 'sum' has a meaning in the grammar …`), explains digits, and explains `_`.

**Done (ADR-019).** The words that start a prefix form (`sum`, `length`, `text`, …) are now free to name variables; only the whole lead of a prefix form (`the sum of`) is refused after `the`. The remaining restriction is a variable named like a prefix form right before `of` (`item the sum of xs`). The rest of the reserved set stays.

## Summary

| # | Topic | Verdict | Action |
|---|---|---|---|
| 1 | Library | three unwritable primitives | added — done (ADR-018) |
| 2 | `break`/`continue`/`If` value | fine | none |
| 3 | I/O | filters and validation impossible | end-of-input and numeric tests — done (ADR-018) |
| 4 | Forced endings | structural, 35 % worst case | headers rhyme before their closer — done (ADR-017) |
| 5 | First-member grouping | order-dependent for identical pairs | pairwise groups — done (ADR-016) |
| 6 | Rhyme strength | measured | none |
| 7 | Numerals | correct | specified and tested — done |
| 8 | Identifiers | small set, bad error | error explained, prefix words freed — done (ADR-019) |
