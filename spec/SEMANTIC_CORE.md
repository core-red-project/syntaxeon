# Syntaxeon — Semantic Core

Status: **draft 0**. Normative for the canonical AST (ADR-005), the type
system (ADR-004) and evaluation. It says nothing about surface prose or
prosody: those are `PROSE_GRAMMAR.md` and `PROSODY.md`, and they must lower
*into* this document, never the reverse (Charter Law 3, Law 9).

Scope is v0 (ADR-010): no records, no anaphora, no modules.

---

## 1. Layering contract

```
surface tree ──normalize──▶ canonical AST (this doc) ──▶ IR
                                   │
                         stanza table (side data) ──▶ prosody pass
```

- The canonical AST contains **no prose, no rhyme, no word order**.
- Every node carries a `Span` into the source and a `StanzaId`.
- The **stanza table** maps `StanzaId → { lines, scheme }`. It is not part
  of the AST proper; the prosody pass (ADR-009) reads it, the evaluator
  never does. Two programs with the same AST and different stanza tables
  evaluate identically.

## 2. Names

`Name` is a normalized identifier: the noun phrase lowercased, leading
determiner (`the`, `this`, `a`) stripped, internal whitespace collapsed to
one space. `the counter` and `This counter` are the same `Name`. A quoted
name is taken verbatim. Scoping is lexical, block-structured; a stanza is
the outermost scope of its own body (ADR-006).

Shadowing within one block is an error `SXP-401`. Shadowing an outer
block's name is allowed.

## 3. Types

```
Type ::= Integer | Float | Boolean | String | Character
       | List(Type)
       | Fn(Type*, Type)          -- result may be Unit
       | Unit
```

- `Unit` is the type of statements-as-expressions with no value (assignment,
  `Speak`, loops). It cannot be written by the programmer, only inferred.
- No implicit conversions. Conversions are explicit builtins (§8).
- `List(T)` is homogeneous. The empty list literal requires a type from
  context or annotation, else `SXP-410 ambiguous type`.
- Functions are not first-class values in v0 (they are called by name only);
  `Fn` exists for signatures and for the IR.
- No equality on `Fn`. Equality on all other types is structural.

## 4. Canonical AST

```
Program   ::= Stanza+
Stanza    ::= { id: StanzaId, title: Name, params: Param*, ret: Type?, body: Block }
Param     ::= { name: Name, type: Type }
Block     ::= Stmt*
Stmt      ::= Decl | Assign | If | While | Repeat | Return | ExprStmt
Decl      ::= { name: Name, type: Type?, mut: Bool, init: Expr }
Assign    ::= { target: Target, value: Expr }
Target    ::= Name | Index(Name, Expr)
If        ::= { cond: Expr, then: Block, else: Block? }
While     ::= { cond: Expr, body: Block }
Repeat    ::= { count: Expr, body: Block }       -- count is Integer
Return    ::= { value: Expr? }
ExprStmt  ::= Expr
Expr      ::= Lit | Var(Name) | Unary(op, Expr) | Binary(op, Expr, Expr)
            | Call(Name, Expr*) | ListLit(Expr*) | Index(Expr, Expr)
Lit       ::= Int(i64) | Float(f64) | Bool | Str | Char
UnOp      ::= Neg | Not
BinOp     ::= Add | Sub | Mul | Div | Mod
            | Eq | Ne | Lt | Le | Gt | Ge | And | Or
```

Decisions folded in here:

- **Statements are not expressions in v0.** Charter §2 says
  expression-oriented; the expression-ness lives in *prose composition*
  ("the sum of two and three"), which the normalizer flattens to `Expr`.
  Making `If` yield values would double the type rules for no gimmick gain
  and is deferred.
- `Decl.mut` comes from ADR-012 (`shall become` ⇒ `true`).
- The **entry stanza** is the first one (ADR-011); it has no params and
  `ret = Unit`. It is not special in the AST beyond position.
- There is no `Break`/`Continue` in v0.

## 5. Static semantics

Judgement `Γ ⊢ e : T`, where Γ maps `Name → { type, mut }`.

```
Γ ⊢ Int(_)   : Integer      Γ ⊢ Float(_) : Float
Γ ⊢ Bool(_)  : Boolean      Γ ⊢ Str(_)   : String      Γ ⊢ Char(_) : Character

Γ(x) = { T, _ }                          ⟹  Γ ⊢ Var(x) : T
Γ ⊢ e : Integer|Float                    ⟹  Γ ⊢ Neg(e) : same
Γ ⊢ e : Boolean                          ⟹  Γ ⊢ Not(e) : Boolean

Γ ⊢ a : T, Γ ⊢ b : T, T ∈ {Integer, Float}
                                         ⟹  Γ ⊢ Add|Sub|Mul|Div(a,b) : T
Γ ⊢ a, b : Integer                       ⟹  Γ ⊢ Mod(a,b) : Integer
Γ ⊢ a, b : String   (Add only)           ⟹  Γ ⊢ Add(a,b) : String
Γ ⊢ a : T, Γ ⊢ b : T, T ∉ Fn             ⟹  Γ ⊢ Eq|Ne(a,b) : Boolean
Γ ⊢ a : T, Γ ⊢ b : T, T ∈ {Integer, Float, Character, String}
                                         ⟹  Γ ⊢ Lt|Le|Gt|Ge(a,b) : Boolean
Γ ⊢ a, b : Boolean                       ⟹  Γ ⊢ And|Or(a,b) : Boolean

Γ ⊢ eᵢ : T  (all i)                      ⟹  Γ ⊢ ListLit(e₁…eₙ) : List(T)
Γ ⊢ l : List(T), Γ ⊢ i : Integer         ⟹  Γ ⊢ Index(l,i) : T
Γ ⊢ s : String, Γ ⊢ i : Integer          ⟹  Γ ⊢ Index(s,i) : Character
f : Fn(T₁…Tₙ, R), Γ ⊢ eᵢ : Tᵢ            ⟹  Γ ⊢ Call(f, e₁…eₙ) : R
```

Statements:

- `Decl{x, T?, mut, e}`: `Γ ⊢ e : U`; if `T` given, `T = U`. Binds
  `x : U` with `mut`. Error `SXP-402` on mismatch.
- `Assign{x, e}`: `x` must be `mut` (`SXP-403` otherwise) and types equal.
  `Assign{Index(x,i), e}` requires `x : List(T)`, `x` mut, `e : T`.
- `If`/`While`: cond is `Boolean`. `Repeat`: count is `Integer`.
- `Return e?`: must match the stanza's `ret`; non-`Unit` functions must
  return on **every** path (`SXP-404 missing return`).
- Entry stanza: `Return` with a value is `SXP-405`.
- Mixed `Integer`/`Float` arithmetic is `SXP-406`; convert explicitly.

Inference is local: a `Decl` without an annotation takes its initializer's
type. Parameters and `ret` must always be declared. No generalization.

## 6. Dynamic semantics

Eager, strict, left-to-right, deterministic. Evaluation order within `Binary`
is left then right; `And`/`Or` short-circuit.

- **Integer** is signed 64-bit. Overflow is a runtime error `SXP-501`
  (never wraps). `Div` and `Mod` by zero: `SXP-502`. `Div` truncates
  toward zero; `Mod` takes the sign of the dividend.
- **Float** is IEEE 754 binary64. `Eq` follows IEEE (`NaN ≠ NaN`);
  float division by zero yields ±∞ / NaN, no error.
- **String** is a sequence of Unicode scalar values; `Index` is by scalar,
  zero-based. **Character** is one scalar value.
- **List** has value semantics: assignment and argument passing copy
  logically (an implementation may share, but no alias is observable).
  `Index` out of range: `SXP-503`.
- `Call` evaluates arguments left to right, then runs the body in a fresh
  scope containing only parameters and top-level stanza names. No closures.
- Recursion is allowed (stanza names are in scope in all bodies).
- Stack exhaustion is `SXP-504`, deterministic limit fixed by the spec at
  **10 000** call frames.
- A runtime error aborts the program: exit code 1, diagnostic with the
  `SXP-5xx` code. No exceptions in v0.

## 7. Mutability

`Decl.mut = false` bindings can never appear as an `Assign` target. A
parameter is immutable. Loop counters of `Repeat` are not exposed in v0.

## 8. Builtins

Builtins are ordinary names in the outermost scope; the normalizer maps
prose verbs to them (verb sets are closed — ADR-005).

| Name | Type | Notes |
|---|---|---|
| `speak` | `Fn(String, Unit)` | writes to stdout, no newline |
| `speakln` | `Fn(String, Unit)` | with newline |
| `hear` | `Fn(String)` (no params) | reads one line from stdin; EOF ⇒ `SXP-505` |
| `length` | `Fn(List(T)\|String, Integer)` | |
| `append` | `Fn(List(T), T, List(T))` | returns a new list |
| `to_string` | `Fn(T, String)` | `T ∈ {Integer, Float, Boolean, Character}`; Float uses the shortest round-trip form that always shows a fraction or exponent (`1.0`, `0.1`, `1e21`, `NaN`, `inf`) |
| `to_integer` | `Fn(Float\|String, Integer)` | truncates toward zero; bad string, NaN, infinity or out-of-range float ⇒ `SXP-506` |
| `to_float` | `Fn(Integer, Float)` | |

Builtins may be called but not redefined (`SXP-407`).

## 9. Program-level checks

In order: (1) exactly one entry stanza (the first) — `SXP-408` if zero
stanzas; (2) stanza names unique — `SXP-409`; (3) every stanza body
type-checks under Γ₀ = builtins + all stanza signatures; (4) definite
return (§5). Only then does the compiler hand the stanza table to the
prosody pass (ADR-009).

## 10. Diagnostic codes owned by this document

| Range | Meaning |
|---|---|
| `SXP-4xx` | static semantic errors (names, types, returns) |

Codes in use (`syntaxeon-check`):

| Code | Meaning |
|---|---|
| 401 | name declared twice in one block (parameters and the top level of a body share one block) |
| 402 | type mismatch, including: value-less call bound to a name; value returned from a stanza that declares no result; missing value where one is required |
| 403 | assignment to an immutable binding |
| 404 | non-`Unit` stanza does not return on every path (a loop may run zero times) |
| 405 | entry stanza returns a value |
| 406 | `Integer`/`Float` mixed without explicit conversion |
| 407 | stanza named like a builtin |
| 408 | program has no stanzas |
| 409 | stanza name defined twice |
| 410 | empty list literal without a determinable element type |
| 411 | unknown name |
| 412 | unknown stanza or function in a call |
| 413 | wrong number of arguments |

Runtime codes in use (`syntaxeon-eval`):

| Code | Meaning |
|---|---|
| 501 | `Integer` overflow (including `MIN / -1` and negating `MIN`; `MIN mod -1` is 0) |
| 502 | `Integer` division or remainder by zero |
| 503 | list or string index out of range (negative included) |
| 504 | more than 10 000 call frames (the entry stanza is frame 1) |
| 505 | `hear` at end of input, or an input error |
| 506 | failed conversion (`to_integer`) |
| 507 | cannot write output |
| 599 | internal: an unchecked program reached the evaluator |

An expression whose type could not be determined is *poisoned*: it reports
once and silently poisons whatever uses it, so one mistake is one diagnostic.
| `SXP-5xx` | runtime errors |

(`SXP-1xx` grammatical, `SXP-2xx` / `SXP-3xx` prosodic and lexicon, assigned
in their own documents. Charter §11 uses `SXP-204` / `SXP-301` for rhyme
examples; those examples are illustrative until `PROSODY.md` fixes the
ranges, and this table supersedes them.)

## 11. Deliberately absent

Records, first-class functions, closures, generics, exceptions, early
`break`, `If` as expression, integer/float coercion, I/O beyond stdin/stdout,
concurrency. Each needs an ADR before it enters this document.

## 12. Open items for the next documents

- `PROSE_GRAMMAR.md` must show how *every* AST node in §4 is reachable from
  at least one prose form, and that no prose form lowers to two ASTs.
- `PROSODY.md` must fix whether a function stanza's rhyme scheme can
  constrain parameter lists (currently: no, only line count).
- Whether `Repeat` needs an index binding (leaning: no, use `While`).
