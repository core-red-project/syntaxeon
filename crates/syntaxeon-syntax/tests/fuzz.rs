#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
//! Grammar-driven fuzzing of the prose parser. Every generated sentence is well-formed
//! according to `spec/PROSE_GRAMMAR.md`, so each must have exactly one reading; any other
//! count is either a grammar bug (zero readings) or an ambiguity (`SXP-104`) to classify.
//!
//! Deterministic: a fixed xorshift seed, no dependencies. `SYNTAXEON_FUZZ_ITERS` and
//! `SYNTAXEON_FUZZ_SEED` scale and vary a run.

use std::collections::BTreeMap;

use syntaxeon_syntax::statement_readings;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

/// Which constructs the generator may use.
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // one switch per construct the generator may use
struct Features {
    /// Obey the open-construct rules (so every sentence is valid). Off, the generator also
    /// writes what a person might: a call followed by an infix word, a prefix form holding an
    /// infix, a list in the middle of a list. Those may be rejected, but never ambiguous.
    respect_open: bool,
    item_of: bool,
    append: bool,
    lists: bool,
    logic: bool,
}

const VALID: Features = Features {
    respect_open: true,
    item_of: true,
    append: true,
    lists: true,
    logic: true,
};
const WILD: Features = Features {
    respect_open: false,
    ..VALID
};

struct Gen {
    rng: Rng,
    f: Features,
}

const NAMES: &[&str] = &[
    "counter",
    "pile",
    "chimes",
    "slumber",
    "hero",
    "bird",
    // Words that start a prefix form after `the` and are still free to name a variable.
    "sum",
    "difference",
    "product",
    "quotient",
    "remainder",
    "text",
    "whole",
    "fraction",
    "length",
    "negation",
    "letter",
    "sum total",
    "text length",
];
const FUNCS: &[&str] = &["twice", "gcd", "spin", "greet"];
const NUMBER_WORDS: &[&str] = &["one", "two", "three", "ten", "twenty-one", "zero"];
const STRINGS: &[&str] = &["\"hi\"", "\"and then\"", "\"a, b\"", "\"is not\"", "\"x\""];
const CMP: &[&str] = &[
    "is",
    "is not",
    "is less than",
    "is greater than",
    "is at most",
    "is at least",
];
const INFIX: &[&str] = &["plus", "minus", "times", "over", "modulo"];

/// Generated text plus whether it ends inside an unbounded construct (a list literal or a call
/// with arguments), which the grammar does not let a comma or an infix word follow
/// (`spec/PROSE_GRAMMAR.md` §7.4).
struct Tx {
    text: String,
    open: bool,
}

fn tx(text: impl Into<String>, open: bool) -> Tx {
    Tx {
        text: text.into(),
        open,
    }
}

/// Whether `text` ends in `the <word>` for a word that starts a prefix form (`the sum of …`).
fn ends_like_prefix_form(text: &str) -> bool {
    const LEADS: &[&str] = &[
        "sum",
        "difference",
        "product",
        "quotient",
        "remainder",
        "text",
        "fraction",
        "length",
        "negation",
    ];
    let words: Vec<&str> = text.split(' ').collect();
    words.len() >= 2 && words[words.len() - 2] == "the" && LEADS.contains(&words[words.len() - 1])
}

impl Gen {
    fn var(&mut self) -> Tx {
        let n = self.rng.pick(NAMES);
        match self.rng.below(4) {
            0 => tx(n, false),
            _ => tx(format!("the {n}"), false),
        }
    }

    /// Retries until `make` yields something closed (it falls back to an atom).
    fn closed(&mut self, d: usize, make: fn(&mut Gen, usize) -> Tx) -> Tx {
        for _ in 0..6 {
            let t = make(self, d);
            if !t.open || !self.f.respect_open {
                return t;
            }
        }
        self.var()
    }

    fn atom(&mut self, d: usize) -> Tx {
        let limit = if d == 0 { 8 } else { 12 };
        match self.rng.below(limit) {
            0 => tx(self.rng.below(100).to_string(), false),
            1 => tx(self.rng.pick(NUMBER_WORDS), false),
            2 => tx(self.rng.pick(STRINGS), false),
            3 => tx(self.rng.pick(&["true", "false"]), false),
            6 => tx("the letter \"x\"", false),
            7 if self.rng.chance(30) => tx("there is more to hear", false),
            7 => tx("what is heard", false),
            8 => self.call(d - 1),
            9 if self.f.item_of => {
                let idx = self.operand(d - 1);
                // `item the sum of xs` can only be read as `the sum of xs`: a variable named like
                // the start of a prefix form cannot sit just before `of`.
                let idx = if ends_like_prefix_form(&idx.text) {
                    tx("ten", false)
                } else {
                    idx
                };
                let list = self.operand(d - 1);
                tx(format!("item {} of {}", idx.text, list.text), list.open)
            }
            10 if self.f.lists => self.list(d - 1),
            _ => self.var(),
        }
    }

    fn call(&mut self, d: usize) -> Tx {
        let f = self.rng.pick(FUNCS);
        if self.rng.chance(30) {
            return tx(format!("calling {f}"), false);
        }
        let args = self.comma_list(d, 3);
        tx(format!("calling {f} with {}", self.join(&args)), true)
    }

    fn list(&mut self, d: usize) -> Tx {
        let items = self.comma_list(d, 3);
        tx(format!("a list of {}", self.join(&items)), true)
    }

    /// Elements of a comma list: every element but the last must be closed.
    fn comma_list(&mut self, d: usize, max: usize) -> Vec<Tx> {
        let mut items = Vec::new();
        loop {
            let t = self.expr(d);
            let more = self.rng.chance(35) && items.len() + 1 < max;
            if more && t.open && self.f.respect_open {
                let c = self.closed(d, Gen::expr);
                items.push(c);
            } else {
                items.push(t);
            }
            if !more {
                return items;
            }
        }
    }

    fn join(&mut self, items: &[Tx]) -> String {
        let texts: Vec<&str> = items.iter().map(|t| t.text.as_str()).collect();
        match texts.as_slice() {
            [only] => (*only).to_string(),
            [init @ .., last] if self.rng.chance(50) => format!("{}, and {last}", init.join(", ")),
            _ => texts.join(", "),
        }
    }

    fn operand(&mut self, d: usize) -> Tx {
        if d == 0 {
            return self.atom(0);
        }
        match self.rng.below(12) {
            5 => {
                let w = self
                    .rng
                    .pick(&["sum", "difference", "product", "quotient", "remainder"]);
                let (a, b) = if self.f.respect_open {
                    (self.operand(d - 1), self.operand(d - 1))
                } else {
                    (self.arith(d - 1), self.arith(d - 1))
                };
                tx(format!("the {w} of {} and {}", a.text, b.text), b.open)
            }
            6 => {
                let w = self
                    .rng
                    .pick(&["text", "whole number", "fraction", "length", "code"]);
                let a = self.operand(d - 1);
                tx(format!("the {w} of {}", a.text), a.open)
            }
            7 => {
                let a = self.operand(d - 1);
                if self.rng.chance(25) {
                    return tx(format!("the letter with code {}", a.text), a.open);
                }
                tx(format!("the negation of {}", a.text), a.open)
            }
            8 => {
                let a = self.operand(d - 1);
                tx(format!("not {}", a.text), a.open)
            }
            9 if self.f.append => {
                let v = self.var();
                let x = self.operand(0);
                tx(format!("{} with {} appended", v.text, x.text), false)
            }
            _ => self.atom(d),
        }
    }

    fn arith(&mut self, d: usize) -> Tx {
        if self.rng.chance(70) {
            self.operand(d)
        } else {
            let a = self.closed(d, Gen::operand);
            let b = self.operand(d);
            tx(
                format!("{} {} {}", a.text, self.rng.pick(INFIX), b.text),
                b.open,
            )
        }
    }

    fn expr(&mut self, d: usize) -> Tx {
        match self.rng.below(10) {
            6..=7 => {
                let a = self.closed(d, Gen::operand);
                let rhs = loop {
                    let o = self.operand(d);
                    if !o.text.starts_with("not ") {
                        break o;
                    }
                };
                tx(
                    format!("{} {} {}", a.text, self.rng.pick(CMP), rhs.text),
                    rhs.open,
                )
            }
            5 => {
                let a = self.closed(d, Gen::operand);
                tx(format!("{} reads as a whole number", a.text), false)
            }
            8 if self.f.logic && d > 0 => {
                let (a, b) = (self.expr(d - 1), self.expr(d - 1));
                tx(format!("both {} and {}", a.text, b.text), b.open)
            }
            9 if self.f.logic && d > 0 => {
                let (a, b) = (self.expr(d - 1), self.expr(d - 1));
                tx(format!("either {} or {}", a.text, b.text), b.open)
            }
            _ => self.arith(d),
        }
    }

    fn sentence(&mut self, d: usize) -> String {
        let e = self.expr(d).text;
        let n = self.rng.pick(NAMES);
        match self.rng.below(9) {
            0 => format!("Let the {n} become {e}."),
            1 => format!("Let the {n} be {e}."),
            2 => format!("The {n} shall be {e}."),
            3 => format!("{e} shall the {n} become."),
            4 => format!("Let the {n} now be {e}."),
            5 => format!("Speak {e}."),
            6 => format!("Proclaim {e}."),
            7 => format!("Give back {e}."),
            _ => format!("Call {} with {}.", self.rng.pick(FUNCS), e),
        }
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// A coarse signature of the constructs in a sentence, for grouping ambiguities.
/// Iteration count: `SYNTAXEON_FUZZ_ITERS` when set, else the test's own default.
fn iters_or(default: usize) -> usize {
    env_or("SYNTAXEON_FUZZ_ITERS", default as u64)
        .try_into()
        .unwrap()
}

/// Varies a test's seed with `SYNTAXEON_FUZZ_SEED` (0 keeps the fixed seeds).
fn seed_offset() -> u64 {
    env_or("SYNTAXEON_FUZZ_SEED", 0)
}

fn features(s: &str) -> String {
    let mut tags = Vec::new();
    for (needle, tag) in [
        ("calling", "call"),
        ("item", "item"),
        ("appended", "append"),
        ("a list of", "list"),
        ("both", "both"),
        ("either", "either"),
        ("the text of", "conv"),
        ("the length of", "conv"),
        ("the whole number of", "conv"),
        ("the fraction of", "conv"),
        ("the code of", "conv"),
        ("letter with code", "conv"),
        ("more to hear", "input"),
        ("reads as", "reads"),
        ("not", "not"),
        ("negation", "neg"),
    ] {
        if s.contains(needle) && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.join("+")
}

struct Report {
    total: usize,
    zero: Vec<String>,
    ambiguous: BTreeMap<String, Vec<String>>,
}

fn run(seed: u64, iters: usize, depth: usize, f: Features) -> Report {
    let mut g = Gen {
        rng: Rng(seed | 1),
        f,
    };
    let mut r = Report {
        total: iters,
        zero: vec![],
        ambiguous: BTreeMap::new(),
    };
    for _ in 0..iters {
        let s = g.sentence(depth);
        match statement_readings(&s).len() {
            1 => {}
            0 => r.zero.push(s),
            _ => r.ambiguous.entry(features(&s)).or_default().push(s),
        }
    }
    r
}

fn summarize(r: &Report) {
    let amb: usize = r.ambiguous.values().map(Vec::len).sum();
    println!(
        "total {} | zero readings {} | ambiguous {}",
        r.total,
        r.zero.len(),
        amb
    );
    let mut zeros = r.zero.clone();
    zeros.sort_by_key(String::len);
    for z in zeros.iter().take(10) {
        println!("ZERO  {z}");
    }
    let mut classes: Vec<_> = r.ambiguous.iter().collect();
    classes.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (class, sentences) in classes {
        let mut shortest = sentences.clone();
        shortest.sort_by_key(String::len);
        println!("\n[{class}] {} sentences", sentences.len());
        for s in shortest.iter().take(3) {
            println!("   {s}");
        }
    }
}

fn explore(f: Features) {
    let iters = usize::try_from(env_or("SYNTAXEON_FUZZ_ITERS", 20_000)).unwrap();
    let depth = usize::try_from(env_or("SYNTAXEON_FUZZ_DEPTH", 3)).unwrap();
    summarize(&run(
        env_or("SYNTAXEON_FUZZ_SEED", 0x9E37_79B9_7F4A_7C15),
        iters,
        depth,
        f,
    ));
}

#[test]
#[ignore = "exploratory: prints every class; run with --ignored --nocapture"]
fn explore_valid() {
    explore(VALID);
}

#[test]
#[ignore = "exploratory: prints every class; run with --ignored --nocapture"]
fn explore_wild() {
    explore(WILD);
}

/// Every sentence the grammar defines has exactly one reading.
#[test]
fn valid_sentences_have_exactly_one_reading() {
    for seed in 1_u64..=4 {
        let r = run(
            seed.wrapping_add(seed_offset())
                .wrapping_mul(0x9E37_79B9_7F4A_7C15),
            iters_or(4_000),
            3,
            VALID,
        );
        assert!(
            r.zero.is_empty(),
            "no reading: {:?}",
            &r.zero[..r.zero.len().min(3)]
        );
        assert!(r.ambiguous.is_empty(), "ambiguous: {:?}", r.ambiguous);
    }
}

/// Sentences a person might write but the grammar may refuse are never ambiguous.
#[test]
fn free_sentences_are_never_ambiguous() {
    for seed in 1_u64..=4 {
        let r = run(
            seed.wrapping_add(seed_offset())
                .wrapping_mul(0xD1B5_4A32_D192_ED03),
            iters_or(4_000),
            3,
            WILD,
        );
        assert!(r.ambiguous.is_empty(), "ambiguous: {:?}", r.ambiguous);
    }
}

const SOUP: &[&str] = &[
    "Let",
    "Speak",
    "Proclaim",
    "Call",
    "Give back",
    "the",
    "a",
    "an",
    "this",
    "counter",
    "pile",
    "of",
    "men",
    "be",
    "become",
    "now",
    "shall",
    "and",
    "or",
    "both",
    "either",
    "is",
    "not",
    "less than",
    "greater than",
    "at most",
    "at least",
    "plus",
    "minus",
    "times",
    "over",
    "modulo",
    "with",
    "appended",
    "calling",
    "item",
    "list",
    "text",
    "length",
    "sum",
    "difference",
    "negation",
    "whole number",
    "what is heard",
    "there is more to hear",
    "reads as a whole number",
    "code",
    "the letter with code",
    "heard",
    ",",
    ",",
    "3",
    "ten",
    "\"x\"",
    "true",
    "false",
    "twice",
    "gcd",
    ", a whole number,",
];

/// Random token soup: most of it is rejected, none of it may be ambiguous.
fn soup(seed: u64, iters: usize) -> Vec<String> {
    let mut rng = Rng(seed | 1);
    let mut ambiguous = Vec::new();
    for _ in 0..iters {
        let len = 2 + rng.below(11);
        let mut words: Vec<&str> = (0..len).map(|_| rng.pick(SOUP)).collect();
        if rng.chance(50) {
            words.insert(
                0,
                rng.pick(&["Let", "Speak", "Proclaim", "Call", "Give back"]),
            );
        }
        let sentence = format!("{}.", words.join(" ").replace(" ,", ","));
        if statement_readings(&sentence).len() > 1 {
            ambiguous.push(sentence);
        }
    }
    ambiguous
}

#[test]
fn token_soup_is_never_ambiguous() {
    let amb = soup(0xA5A5_5A5A_1234_5678 ^ seed_offset(), iters_or(60_000));
    assert!(
        amb.is_empty(),
        "{} ambiguous, e.g. {:?}",
        amb.len(),
        &amb[..amb.len().min(5)]
    );
}

#[test]
#[ignore = "exploratory: a long soup run; scale with SYNTAXEON_FUZZ_ITERS"]
fn explore_soup() {
    let iters = usize::try_from(env_or("SYNTAXEON_FUZZ_ITERS", 1_000_000)).unwrap();
    let mut amb = soup(env_or("SYNTAXEON_FUZZ_SEED", 99), iters);
    amb.sort_by_key(String::len);
    println!("soup: {} ambiguous of {iters}", amb.len());
    for s in amb.iter().take(15) {
        println!("   {s}");
    }
}

fn parse_codes(src: &str) -> Vec<&'static str> {
    use syntaxeon_lex::lexer::lex;
    use syntaxeon_lex::literary::stanzas;
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    let (_, diags) = syntaxeon_syntax::parse_program(&lexed, &verses);
    lexed
        .diagnostics
        .iter()
        .chain(&diags)
        .map(|d| d.code)
        .collect()
}

fn random_type(rng: &mut Rng) -> String {
    let base = rng.pick(&["whole number", "fraction", "truth", "text", "letter"]);
    let mut t = base.to_string();
    while rng.chance(25) {
        t = format!("list of {t}");
    }
    t
}

/// Stanza headers with random parameter lists, result types and titles read one way only.
#[test]
fn stanza_headers_are_never_ambiguous() {
    let mut rng = Rng(0xFEED_FACE_CAFE_BEEF ^ seed_offset());
    for _ in 0..iters_or(3_000) {
        let title = rng.pick(&["double", "tale of men", "rhyme in reason", "song of ten"]);
        let mut header = format!("To {title}");
        let n = rng.below(4);
        if n > 0 {
            let params: Vec<String> = (0..n)
                .map(|i| {
                    let name = ["slumber", "lumber", "men", "bird"][i];
                    format!("{name} as {}", random_type(&mut rng))
                })
                .collect();
            let joined = match params.as_slice() {
                [init @ .., last] if rng.chance(50) && !init.is_empty() => {
                    format!("{}, and {last}", init.join(", "))
                }
                _ => params.join(", "),
            };
            header = format!("{header}, taking {joined}");
        }
        if rng.chance(60) {
            header = format!("{header}, giving {}", random_type(&mut rng));
        }
        let src = format!("Thus begins t:\nReturn nothing.\n\n{header}:\nReturn nothing.\n");
        let codes = parse_codes(&src);
        assert!(codes.is_empty(), "{header}: {codes:?}");
    }
}

/// Block headers over random expressions: one reading, and the blocks close correctly.
#[test]
fn block_headers_are_never_ambiguous() {
    let mut g = Gen {
        rng: Rng(0x00DD_BA11_C0FF_EE00 ^ seed_offset()),
        f: VALID,
    };
    for _ in 0..iters_or(3_000) {
        let e = g.expr(2).text;
        let header = match g.rng.below(3) {
            0 => format!("If {e}, then:"),
            1 => format!("As long as {e}, do:"),
            _ => format!("Do this {e} times:"),
        };
        let src = format!("Thus begins t:\n{header}\nSpeak \"x\".\nSo be it.\n");
        let codes = parse_codes(&src);
        assert!(codes.is_empty(), "{header}: {codes:?}");
    }
}

/// Which ordinary English words cannot be used as a one-word name, in a declaration and in an
/// expression? Prints the count and the words (`--ignored --nocapture`).
#[test]
#[ignore = "exploratory: prints the words the grammar refuses as names"]
fn explore_names() {
    let words: Vec<&str> = include_str!("../../syntaxeon-lexicon/lexicon/common-words.txt")
        .lines()
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_lowercase()))
        .collect();
    let mut refused: Vec<(&str, &str)> = Vec::new();
    for w in &words {
        let declare = statement_readings(&format!("Let the {w}, a whole number, become ten."));
        let usage = statement_readings(&format!("Proclaim the text of the {w}."));
        let assign = statement_readings(&format!("Let the {w} now be ten."));
        let why = match (declare.len(), usage.len(), assign.len()) {
            (1, 1, 1) => continue,
            (0, _, _) => "declaration",
            (_, 0, _) => "use",
            (_, _, 0) => "assignment",
            _ => "ambiguous",
        };
        refused.push((w, why));
    }
    // Per ten thousand, to stay in integers.
    eprintln!(
        "{} of {} common words are refused as names ({} per 10 000)",
        refused.len(),
        words.len(),
        refused.len() * 10_000 / words.len()
    );
    for (w, why) in &refused {
        eprint!("{w}({why}) ");
    }
    eprintln!();
}
