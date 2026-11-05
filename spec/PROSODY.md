# Syntaxeon — Prosody

Status: **draft 0**. Defines the prosodic layer of v0: the pinned
pronunciation lexicon, the rhyme function, the permitted stanza schemes and
the diagnostics. Governing ADRs: 001 (prosody is validity), 006 (v1 scope),
007 (pinned lexicon), 009 (runs after semantic analysis).

Input: the **stanza table** produced by the literary layer
(`PROSE_GRAMMAR.md` §3). Output: pass, or `SXP-2xx` / `SXP-3xx` errors.
Prosody never reads the AST and never changes it.

v1 checks **only end rhyme**. Meter and stress patterns are out of scope,
but the lexicon already carries stress so v2 needs no data change.

---

## 1. Determinism contract

1. The compiler uses **only** the vendored lexicon (§2) plus in-file
   pragmas (§3). No OS speech API, no network, no locale, no G2P model.
2. The rhyme function (§4) is pure: `(wordA, wordB, lexicon) → bool`.
3. A conforming implementation must pass the vectors in §9 on the pinned
   snapshot. A verdict that differs between two conforming implementations
   is a spec bug.
4. Language: English only (ADR-007).

## 2. The lexicon

- **Source**: `cmudict.dict` from `github.com/cmusphinx/cmudict` at commit
  `74790861f652b15e4ac49015a90074ad62a27690` (2025-10-24), 135 166 lines,
  sha256 `81917843…1c3d22`. ARPAbet with stress digits (`0` none,
  `1` primary, `2` secondary), `word(2)` alternates, some lines carry a
  trailing `# note` which the loader strips. Licence: CMU BSD-style, kept in
  `crates/syntaxeon-lexicon/lexicon/CMUDICT-LICENSE`. Pin recorded in
  `lexicon/lexicon.lock`.
- **Integrity**: the build verifies the sha256 of the vendored file against
  `lexicon.lock` and the spec's `syntaxeon-lexicon` version. Mismatch ⇒
  build failure `SXP-304` (tooling error, not a user error).
- **Lookup key**: the word folded to lowercase, NFC, with the trailing
  `'s` kept as written (`dog's` is looked up as `dog's`; if absent,
  `SXP-301`). No stemming, no inflection guessing.
- **Hyphenated words** (`twenty-one`): whole word first; if absent, the
  last hyphen component.
- **Multiple pronunciations**: an entry may list several (`word`, `word(2)`).
  All are kept.

A second pinned file, `common-words.txt` (ESDB size 35, US, lowercase, 38 658 words), is **not** part of the rhyme rule: it only decides which words `syntaxeon rhymes` suggests by default. Its pin and notice are recorded in the same `lexicon.lock`, and a mismatch is also `SXP-304`.

Only **verse end words** are looked up (§5), so a program uses unusual
vocabulary freely everywhere else.

## 3. Pronunciation pragma

An aside whose first word is `pronounced` is a **pragma**, not a comment.
It is lexed like an aside (excluded from verses, ignored by rhyme counts)
but retained for this phase.

```
(pronounced "zyzzyva" as "Z IH1 Z AH0 V AH0")
```

- Phonemes are space-separated ARPAbet from the table in §10; stress
  digits required on vowels. Invalid ⇒ `SXP-302`.
- File-scoped, effective from the start of the file (position irrelevant).
- **Cannot override** a lexicon word (`SXP-303`): the lexicon stays
  authoritative. It only adds words.
- A pragma word participates as one pronunciation like any other entry.

## 4. The rhyme function

For a pronunciation `p` (phoneme list):

- **stressed vowel**: the last vowel with stress `1`; if none, the last
  with stress `2`; if none, the last vowel (so unstressed words like `the`
  use their only vowel).
- `tail(p)` = phonemes from the stressed vowel to the end, **stress digits
  removed**.
- `onset(p)` = the single phoneme immediately before the stressed vowel (stress digit ignored),
  or `∅` at word start.

Two pronunciations `p`, `q` **rhyme** iff `tail(p) = tail(q)` and
`onset(p) ≠ onset(q)`. (Equal onsets make it identical rhyme, which is not
rhyme: this excludes `night`/`knight`, `eye`/`I`, and a word with itself.)

Two *words* rhyme iff **some** pronunciation of the first rhymes with
**some** pronunciation of the second. (Existential, hence deterministic.)

## 5. End words

The **end word** of a verse (from the literary layer) is its last word before
terminal punctuation, with these rules:

- **Verse ends inside a quoted literal** (`Speak "Hello again".`): the end
  word is the last word *inside* the quotes. A literal with no alphabetic
  word (`"…"`, `"42"`) ⇒ `SXP-203`.
- **Verse ends in a numeral** (`ten`, `10`, `3.14`): numerals are verbalized
  to English short-scale cardinals without `and` (`10`→`ten`, `21`→
  `twenty-one` → `one`, `100`→`hundred`, `3.14`→…`four`); the end word is the last
  word of that reading. Number-words are already words.
- **Verse ends in an aside or pragma**: those are excluded; the end word is
  the previous word. A verse that is only an aside is dropped (literary
  layer).
- **Hyphenated words rhyme on their last component** (`seventy-five` → `five`, `mother-in-law` → `law`). Compounds carry their primary stress on the first part, so the whole word's tail does not match what a reader hears. This applies to verbalized numerals too.
- Apostrophes inside a word are kept (`o'er`).
- A word absent from lexicon and pragmas ⇒ `SXP-301`.

## 6. Which verses count

A verse is **exempt** iff it is exactly `Otherwise:` or `So be it.`
(`PROSE_GRAMMAR.md` §8), since those carry no content. Exempt verses are
removed before counting and checking.

**Everything else counts, including headers.** `Thus begins the tale of
men:` has end word `men`; `To double, taking … giving whole number:` has
`number`. This closes the loophole where conditions in block headers
(`If E, then:`) would otherwise escape the rhyme check.

## 7. Stanza schemes (closed set)

Let `n` be the number of counting verses in a stanza.

| n | permitted schemes |
|---|---|
| 1 | none — `SXP-201` |
| even ≥ 2 | **couplet chain** `AABB…`; and for `n = 4` also `ABAB`, `ABBA` |
| odd ≥ 3 | couplet chain whose last three verses form one group: `AABB…CCC`, `AAA` for `n = 3` |

A **scheme** assigns a letter to each verse. A stanza is valid iff there is
a permitted scheme such that, for every letter, **every verse of that letter
rhymes with the first verse of that letter** (§4). Letters need not be
distinct from each other: `AAAA` satisfies `AABB`. (Rhyme with the *first*
member, not pairwise, keeps the check linear and deterministic.)

Invalid ⇒ `SXP-202`. The diagnostic reports the permitted scheme with the
fewest failing groups (ties: the first in the order `AABB…`, `ABAB`, `ABBA`)
and, for each failing verse, the expected tail and the tail found.

## 8. Diagnostics

| Code | Category | Meaning |
|---|---|---|
| 201 | prosodic | stanza has fewer than two counting verses |
| 202 | prosodic | stanza satisfies no permitted scheme |
| 203 | prosodic | end word cannot be extracted (literal with no word) |
| 301 | lexicon | end word not in lexicon or pragmas |
| 302 | lexicon | malformed pronunciation pragma |
| 303 | lexicon | pragma tries to redefine a lexicon word |
| 304 | tooling | lexicon file does not match `lexicon.lock` |

Voice (renderer over the structure; `--json` emits only fields):

```
ERROR SXP-202   category: prosodic
The verse hath failed to rhyme.

  stanza 1, scheme AA
  verse 1  "...the tale of men:"   tail /EH N/
  verse 2  "Speak "Hello"."        tail /OW/    ← expected /EH N/
```

This table fixes the ranges left open in `SEMANTIC_CORE.md` §10 and
supersedes the illustrative `SXP-204` in the Charter.

## 9. Test vectors

Each pair is checked against the pinned snapshot by
`crates/syntaxeon-lexicon/tests/prosody.rs`. All rows below, plus `fire` /
`higher` (yes), pass on the real data. Note the snapshot has
`amen EY0 M EH1 N`, so `men`/`amen` fail on equal onset `M`.

| A | B | rhymes | reason |
|---|---|---|---|
| tonight | light | yes | tail `AY T`, onsets `N` ≠ `L` |
| ten | then | yes | tail `EH N`, onsets `T` ≠ `DH` |
| love | dove | yes | tail `AH V`, onsets `L` ≠ `D` |
| eye | high | yes | tail `AY`, onsets `∅` ≠ `HH` |
| night | knight | no | identical rhyme (onsets equal) |
| eye | I | no | identical rhyme (both `∅`) |
| cat | cat | no | same word |
| ten | tonight | no | tails differ |
| men | amen | no | onset `M` = `M` |

Whole-program vectors:

```
Thus begins the tale of men:        stanza of 4, verses: men / again / ten / amen
Speak "Hello again".                AABB: {men,again} {ten,amen}  → valid
Proclaim the text of ten.
Speak "Amen".
```
```
Thus begins the tale of men:        n = 2, AA: men vs hello → SXP-202
Speak "Hello".
```
(`again` is checked through its pronunciations `AH0 G EH1 N` and `AH0 G EY1
N`; the first rhymes with `men` — onset `G` ≠ `M`.)

## 10. ARPAbet inventory (39 phonemes)

Vowels (take stress digits): `AA AE AH AO AW AY EH ER EY IH IY OW OY UH UW`.
Consonants: `B CH D DH F G HH JH K L M N NG P R S SH T TH V W Y Z ZH`.

## 11. Known gaps and open items

- **Pairwise vs first-member grouping** (§7). First-member is simple but
  asymmetric: a stanza can pass or fail depending on which verse leads.
  Leaning: keep, and revisit if real programs hit it.
- **Rhyme quality**: only perfect rhyme. No assonance, no slant rhyme.
  Multi-syllable "feminine" rhyme works only through the single stressed
  vowel's tail, so `fire`/`higher` rhyme, but `ending`/`bending` rhyme only
  by accident of the dictionary's stress marks.
- **Cost of constraint**: every counting verse ends a rhyme group. Block
  closers are exempt but headers are not, so deep nesting forces real
  poetry. Whether that is the intended difficulty is the main language-design
  question for the vertical slice.
- No locale/dialect support; American pronunciations only (CMUdict).
- Verbalization of large numerals and exotic scripts is specified by
  example, not by a complete algorithm; the implementation must supply a
  test table.
- Meter/stress/alliteration: reserved for v2+, not specified.
