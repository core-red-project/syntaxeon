# Glossary

One concept, one term, across code, tests, specs and docs. Add a term in the same change that introduces it.

| Term | Meaning |
|---|---|
| **verse** | One physical line of source, after asides are removed. |
| **stanza** | A maximal run of non-blank verses. It is the unit of scope and of rhyme. |
| **entry stanza** | The first stanza of a file; it begins `Thus begins …:` and is where execution starts. |
| **function stanza** | Any later stanza; it begins `To …:` and may take parameters and give a result. |
| **header** | The sentence ending in `:` that opens a stanza or a block. |
| **block** | The sentences between a block header (`If`, `As long as`, `Do this … times`) and `So be it.`. |
| **sentence** | Tokens up to a terminator (`.`, `!`, `?`, or `:` for headers). May span verses. |
| **aside** | A parenthesized remark; ignored everywhere, including by rhyme. |
| **pragma** | An aside beginning with `pronounced`; it adds a word to the pronunciation lexicon. |
| **end word** | The last word of a verse, which is what rhyme compares. |
| **counting verse** | A verse that takes part in rhyme; all verses except `Otherwise:` and `So be it.`. |
| **structural verse** | `Otherwise:` or `So be it.`; exempt from rhyme. |
| **tail** | The phonemes from the stressed vowel of a word to its end. |
| **onset** | The single phoneme right before the stressed vowel. |
| **rhyme** | Equal tails and different onsets. Equal onsets are identical rhyme, which is rejected. |
| **scheme** | The assignment of rhyme groups to a stanza's verses (`AABB`, `ABAB`, `ABBA`, …). |
| **lexicon** | The pinned CMU Pronouncing Dictionary snapshot used for every pronunciation. |
| **reading** | One parse of a sentence. More than one distinct reading is an error. |
| **surface tree / canonical AST** | The parse as written, and the form after normalization, which is all that later phases see. |
| **phase** | One stage of the pipeline; checking stops at the first phase that fails. |
