# Syntaxeon — Prose Grammar

Status: **draft 0**. Defines the surface language of v0 and its lowering
into the canonical AST of `SEMANTIC_CORE.md`. It says nothing about rhyme
(`PROSODY.md`); it only fixes *which lines and words exist* so prosody has
something to inspect.

Governing ADRs: 005 (closed grammar, normalizer, ambiguity = error),
010 (v0 exclusions), 011 (entry), 012 (comments, mutability).

---

## 1. Principles

1. **Closed.** Every accepted sentence matches a rule in this document. No
   inference about meaning, no fuzzy matching.
2. **Role words, not keywords.** Verbs and connectives are drawn from closed
   sets (§5). A rule lists the set, not a single word.
3. **Context-free surface, no semantic state.** Parsing never consults
   scope or types. In particular a declaration and an assignment are told
   apart by *a marker word* (`now`), never by whether the name already
   exists.
4. **All parses or none.** The parser is a general CFG parser (Earley or
   GLR) that returns every parse. Exactly one ⇒ normalize it. More than
   one ⇒ `SXP-104`. Zero ⇒ `SXP-103`.
5. **Inversion is allowed only where a rule says so.** Multisyntax is an
   enumerated equivalence class per construct (§6), not free word order.
6. **No precedence.** Operator nesting is expressed by prefix forms with
   arity (§7.3). Infix word forms exist but cannot chain.

## 2. Lexical layer

- **Encoding** UTF-8, NFC-normalized before lexing. Tabs are errors.
- **Case**: words are case-insensitive (folded with Unicode simple folding);
  quoted text is verbatim.
- **Word**: letters, internal `'` and `-` (`don't`, `twenty-one`).
- **Number**: digits `0-9` (optionally one `.` between digit runs for a
  Float, `3.14`), or a **number-word** `zero … ninety-nine` (Integer only).
  A `.` followed by a space or end of line is a terminator, not a decimal.
- **String**: `"…"` on one line, escapes `\" \\ \n`. **Char**:
  `the letter "x"` (exactly one scalar value in the quotes, §7.1).
- **Punctuation**: `, ; : . ! ?` and `(` `)` for asides.
- **Aside (comment, ADR-012)**: `( … )`, may nest, may span lines, ignored
  by every later phase **including prosody** (it does not contribute to line
  endings). Unbalanced ⇒ `SXP-101`. An aside whose first word is
  `pronounced` is a **pragma** (`PROSODY.md` §3): still excluded from
  verses, but retained for the lexicon.
- **Terminators** `.` `!` `?` are equivalent. `:` ends a *header*. `;` is
  unused in v0 (reserved).
- Anything else ⇒ `SXP-101 unknown token`.

## 3. Literary layer

- **Verse** = one physical line (after stripping asides). The verse's
  *end word* is its last word before terminal punctuation; this is what
  prosody sees. A verse with no word (only an aside) is dropped.
- **Stanza** = maximal run of non-blank verses. Blank lines separate
  stanzas. Stanza = scope of its own body (ADR-006).
- **Sentence** = tokens up to a terminator. A sentence may span verses and a
  verse may contain several sentences; the two structures are independent.
- A **header** is a sentence ending in `:`. Every stanza begins with
  exactly one header (§4); a header may also open a nested block (§8).

The literary layer produces the **stanza table** (`SEMANTIC_CORE.md` §1):
`StanzaId → { verses: [Verse{end_word, span, structural: bool}], … }`.
A verse is `structural` iff it is exactly `Otherwise:` or the closer
`So be it.` (§8). Headers are **not** structural: they carry content and
count for rhyme (`PROSODY.md` §6).

## 4. Stanza headers

```
Entry     ::= "Thus begins" Name ":"
Function  ::= "To" Name [ "," "taking" Param { "," Param } [ "," "and" Param ] ]
                         [ "," "giving" Type ] ":"
Param     ::= Name "as" Type
Type      ::= "whole number" | "fraction" | "truth" | "text" | "letter"
            | "list of" Type
```

`Thus begins X:` is legal only on the first stanza of the file, and the
first stanza must use it (ADR-011). Any later stanza uses `To … :`.
`giving` omitted ⇒ `ret = Unit`.

Lowering: `Entry → Stanza{ title, params: [], ret: Unit }`;
`Function → Stanza{ title, params, ret }`.

Type words map: whole number→Integer, fraction→Float, truth→Boolean,
text→String, letter→Character, `list of T`→List(T).

## 5. Closed word sets

Reserved words cannot appear inside a `Name` (use a quoted name
`"the end"` if you need one; `SXP-105`).

| Role | Words |
|---|---|
| determiner | `the` `this` `that` `a` `an` `my` `thy` `our` |
| decl-immutable | `be` |
| decl-mutable | `become` |
| modal | `shall` |
| assign-marker | `now` |
| let | `let` |
| output | `speak` (no newline), `proclaim` (newline) |
| return | `give back`, `return` |
| call | `call` |
| conditional | `if` `then` `otherwise` |
| loop | `as long as` `do` `times` |
| block-close | `so be it` |
| boolean literal | `true` `false` |
| logic | `both … and`, `either … or`, `not` |
| compare | `is` `is not` `is less than` `is greater than` `is at least` `is at most` |
| infix arith | `plus` `minus` `times` `over` `modulo` |
| prefix arith | `the sum of` `the difference of` `the product of` `the quotient of` `the remainder of` |

Sets are versioned with the grammar (`syntaxeon-grammar 0`). Adding a
word to a set is a minor version; removing one is major.

## 6. Statements and multisyntax classes

`N` = Name, `E` = Expression (§7), `T` = Type, `X` = assignment target
(`N` or `item E of N`). Each class lists every accepted surface template;
all templates in a class lower to the same AST.

### 6.1 Declaration → `Decl{name, type?, mut, init}`

`V ∈ {be → mut=false, become → mut=true}`. The optional annotation is the
apposition `, a T,` / `, an T,` right after `N`.

```
D1   N [", a" T ","] "shall" V E "."
D2   "Let" N [", a" T ","] V E "."
D3   E "shall" N [", a" T ","] V "."          (inversion)
```

`an empty list of T` is legal **only** as `E` of a Decl that carries an
annotation `List(T)`; elsewhere `SXP-108`. (The AST has no typed-empty-list
node; the annotation carries it.)

### 6.2 Assignment → `Assign{target, value}`

The word `now` is the marker; it is what separates this class from §6.1.

```
A1   X "shall now be" E "."
A2   "Let" X "now be" E "."
A3   E "shall" X "now be" "."                 (inversion)
```

`X = "item" E "of" N` lowers to `Target::Index(N, E)` (zero-based).

### 6.3 Output → `ExprStmt(Call(speak|speakln, [E]))`

```
O1   "Speak" E "."          → speak
O2   "Proclaim" E "."       → speakln
```

### 6.4 Call statement → `ExprStmt(Call(N, args))`

```
C1   "Call" N [ "with" Args ] "."
Args ::= E { "," E } [ "," "and" E ]
```

Only calls and output verbs may stand alone as statements; any other bare
expression ⇒ `SXP-103`.

### 6.5 Return → `Return{value?}`

```
R1   "Give back" E "."      R2   "Return" E "."
R3   "Give back nothing."   R4   "Return nothing."
```

### 6.6 Control → `If` / `While` / `Repeat`

Blocks open with a header sentence and close with the closer sentence
`So be it.` (§8).

```
I1   "If" E "," "then" ":"   Stmt* [ "Otherwise" ":" Stmt* ]  "So be it."
W1   "As long as" E "," "do" ":"   Stmt*  "So be it."
P1   "Do this" E "times" ":"   Stmt*  "So be it."
```

`If` lowers to `If{cond, then, else?}`; `W1` to `While`; `P1` to `Repeat`.

## 7. Expressions

### 7.1 Atoms

| Form | AST |
|---|---|
| number / number-word | `Lit::Int` / `Lit::Float` |
| `"…"` | `Lit::Str` |
| `the letter "x"` | `Lit::Char` |
| `true` `false` | `Lit::Bool` |
| `N` (with optional determiner) | `Var(N)` |
| `item E of E` | `Index` |
| `a list of E { "," E } [ "," "and" E ]` | `ListLit` |
| `what is heard` | `Call(hear, [])` |
| `calling N [ "with" Args ]` | `Call(N, args)` |

A single-element list is `a list of E`; a zero-element list is §6.1's
`an empty list of T`.

### 7.2 Conversion and list sugar

| Form | Lowers to |
|---|---|
| `the text of E` | `Call(to_string, [E])` |
| `the whole number of E` | `Call(to_integer, [E])` |
| `the fraction of E` | `Call(to_float, [E])` |
| `the length of E` | `Call(length, [E])` |
| `E with E appended` | `Call(append, [E, E])` |

### 7.3 Operators: prefix with arity, infix without chaining

**Operand classes** (implemented in `syntaxeon-syntax`; they remove the
ambiguities that appear when every prefix operand is a full expression):

- *operand* — atom, `not`/`negation`/conversion form, or `base with E appended`;
- *arith* — an operand, or `operand infix-word operand`;
- *expr* — arith, a comparison (`operand is … operand`), or `both`/`either` over
  full exprs.

The five arithmetic prefix forms take two *arith* operands, so
`the sum of a and b is 5` is only `(the sum of a and b) is 5`. `both`/`either`
exist only at *expr* level, so their result is never the left side of a
comparison, and `the text of …` cannot wrap one (bind it with `Let` first).
Names must not start with `the` + a prefix-form head word, nor `a`/`an` +
`list`/`empty`; determiners are reserved words, so `a` cannot be a name.

Prefix forms take operands separated by `and` **by arity**: `the sum of`
takes exactly two operands, so in `the sum of the product of 2 and 3 and 4`
the inner `product` consumes `2 and 3` first and `4` belongs to `sum`. Parsing
by arity is deterministic and needs no precedence table.

```
Add   "the sum of" E "and" E             |  A "plus" B
Sub   "the difference of" E "and" E      |  A "minus" B
Mul   "the product of" E "and" E         |  A "times" B
Div   "the quotient of" E "and" E        |  A "over" B
Mod   "the remainder of" E "and" E       |  A "modulo" B
Neg   "the negation of" E
Not   "not" E
And   "both" E "and" E
Or    "either" E "or" E
Eq    E "is" E          Ne  E "is not" E
Lt    E "is less than" E       Gt  E "is greater than" E
Le    E "is at most" E         Ge  E "is at least" E
```

Infix and comparison forms accept only **atoms or prefix-form operands**,
and may not chain: `1 plus 2 plus 3`, `a is b is c`, or `a plus b times c`
⇒ `SXP-106 chained infix` (use prefix forms to nest). In a comparison,
operands extend to the next comparison/infix word.

Argument lists (§6.4, §7.1) are delimited by `,`, so `and` inside one
operand never leaks into the list.

## 8. Blocks and the closer

A header sentence ending in `:` that is not a stanza header opens a block.
The closer `So be it.` closes the innermost open block. `Otherwise:` is legal
only directly inside an open `If` block and may appear at most once.
Violations: stray closer or `Otherwise` ⇒ `SXP-107`; stanza ends with an
open block ⇒ `SXP-107`. Blocks nest freely; they never cross a stanza
boundary.

## 9. Name resolution at the surface

`Name ::= [determiner] word { word }`, stopping at the next role word the
current rule expects. Normalization to `Name` (determiner stripped, folded,
spaces collapsed) is defined in `SEMANTIC_CORE.md` §2. Because role words
are reserved, `Let the counter be one.` is unambiguous: `the counter` is the
maximal word run before `be`.

## 10. Diagnostic codes (`SXP-1xx`)

| Code | Meaning |
|---|---|
| 101 | unknown token / unbalanced aside / unterminated string |
| 102 | invalid number or character literal |
| 103 | no grammar rule matches the sentence |
| 104 | ambiguous sentence (two or more parses) — lists the parses |
| 105 | reserved word used inside a name |
| 106 | chained infix operators |
| 107 | block structure error (stray/unclosed closer, `Otherwise`) |
| 108 | `an empty list of T` outside an annotated Decl |
| 109 | malformed stanza header; entry header not first |

## 11. Reachability (answers `SEMANTIC_CORE.md` §12, item 1)

| AST node | Surface |
|---|---|
| `Decl` | D1–D3 |
| `Assign` (name / index) | A1–A3 |
| `If` / `While` / `Repeat` | I1 / W1 / P1 |
| `Return` | R1–R4 |
| `ExprStmt` | O1, O2, C1 |
| `Lit`, `Var`, `Index`, `ListLit`, `Call` | §7.1 |
| `Unary`, `Binary` | §7.3 |
| builtins | O1/O2 (`speak`/`speakln`), §7.1 (`hear`), §7.2 |

Every node in `SEMANTIC_CORE.md` §4 is reachable. The converse
(no sentence lowers to two ASTs) is enforced by `SXP-104` rather than proved.

## 12. Worked example

```
Thus begins the counting:
Let the counter, a whole number, become zero.
Do this five times:
Let the counter now be the sum of the counter and one.
Proclaim the text of the counter.
So be it.

To double, taking the number as whole number, giving whole number:
Give back the product of the number and two.
```

Lowers to `Stanza(counting){ Decl(counter,Integer,mut,0); Repeat(5){ Assign(
counter, Add(Var counter, 1)); ExprStmt(Call(speakln,[Call(to_string,
[Var counter])])) } }` and `Stanza(double){ Return(Mul(Var number, 2)) }`.
Rhyme is **not** satisfied here; this example is syntactically valid and
prosodically invalid until `PROSODY.md` rules on it.

## 13. Known gaps and open items

- **`This integer shall become one`** (Charter §6) is not accepted: it
  leaves the binding unnamed. The apposition form `the N, a T, shall become
  one` replaces it. The Charter text should be updated.
- **Structural verses vs rhyme** — resolved in `PROSODY.md` §6: only
  `Otherwise:` and `So be it.` are exempt; headers rhyme.
- No `;`, no early exit, no nested lists of lists in literals (types allow
  them; no literal form does).
- Number-words stop at ninety-nine; larger numbers need digits.
- The grammar is unimplemented and untested. Ambiguity (`SXP-104`) risk is
  highest in names containing words close to role sets and in `is`
  comparisons mixed with `and`/`or` prefix forms; the vertical slice must
  fuzz these.
