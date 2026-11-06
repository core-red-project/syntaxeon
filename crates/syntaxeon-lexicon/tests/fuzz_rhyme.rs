#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
//! Rhyme and stanza fuzzing against an independent oracle written straight from
//! `spec/PROSODY.md` §4 and §7 over the raw dictionary text (no shared code with the crate), in
//! both modes: perfect rhyme only, and perfect rhyme plus assonance (the default).
//!
//! Deterministic xorshift; `SYNTAXEON_FUZZ_ITERS` and `SYNTAXEON_FUZZ_SEED` scale a run.

use std::collections::HashMap;
use syntaxeon_lexicon::lexicon::{Lexicon, RhymeRules};
use syntaxeon_lexicon::rhyme::{RhymeKind, rhyme_kind, rhymes_with, words_rhyme};
use syntaxeon_lexicon::scheme::{find_scheme, schemes};

const DICT: &str = include_str!("../lexicon/cmudict.dict");

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n as u64).unwrap()
    }
}

fn env_or(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

type Oracle = HashMap<String, Vec<Vec<String>>>;

fn oracle() -> Oracle {
    let mut m: Oracle = HashMap::new();
    for line in DICT.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut it = line.split_whitespace();
        let Some(head) = it.next() else { continue };
        let word = head.split('(').next().unwrap().to_lowercase();
        m.entry(word)
            .or_default()
            .push(it.map(str::to_string).collect());
    }
    m
}

/// (tail without stress digits, onset, vowels of the tail) per §4.
type Parts = (Vec<String>, Option<String>, Vec<String>);

fn split(p: &[String]) -> Option<Parts> {
    let ends = |d: char| p.iter().rposition(|t| t.ends_with(d));
    let idx = ends('1')
        .or_else(|| ends('2'))
        .or_else(|| p.iter().rposition(|t| t.ends_with(char::is_numeric)))?;
    let bare = |t: &String| t.trim_end_matches(char::is_numeric).to_string();
    let tail = p[idx..].iter().map(bare).collect();
    let vowels = p[idx..]
        .iter()
        .filter(|t| t.ends_with(char::is_numeric))
        .map(bare)
        .collect();
    Some((tail, idx.checked_sub(1).map(|i| bare(&p[i])), vowels))
}

fn rhyme(o: &Oracle, a: &str, b: &str, assonance: bool) -> bool {
    a != b
        && o[a].iter().any(|x| {
            o[b].iter().any(|y| match (split(x), split(y)) {
                (Some((tx, ox, vx)), Some((ty, oy, vy))) => {
                    let identical = tx == ty && ox == oy;
                    (tx == ty && ox != oy) || (assonance && !identical && vx == vy)
                }
                _ => false,
            })
        })
}

fn oracle_schemes(n: usize) -> Vec<Vec<usize>> {
    if n < 2 {
        return vec![];
    }
    let mut couplets: Vec<usize> = (0..n / 2 * 2).map(|i| i / 2).collect();
    if n % 2 == 1 {
        let last = couplets.last().copied().unwrap_or(0);
        couplets.push(last);
    }
    let mut out = vec![couplets];
    if n == 4 {
        out.extend([vec![0, 1, 0, 1], vec![0, 1, 1, 0]]);
    }
    out
}

fn stanza_ok(o: &Oracle, words: &[&str], assonance: bool) -> bool {
    oracle_schemes(words.len()).iter().any(|s| {
        // Every pair inside a group rhymes.
        (0..s.len())
            .all(|i| (0..i).all(|j| s[j] != s[i] || rhyme(o, words[j], words[i], assonance)))
    })
}

struct Fixture {
    lex: Lexicon,
    oracle: Oracle,
    words: Vec<String>,
    /// words by rhyme tail of their first pronunciation, groups of three or more only
    groups: Vec<Vec<String>>,
    /// the same by the vowels of the tail (assonance)
    vgroups: Vec<Vec<String>>,
}

impl Fixture {
    /// The lexicon under the rules of one mode.
    fn lex(&self, assonance: bool) -> Lexicon {
        let mut lex = self.lex.clone();
        lex.set_rhyme_rules(RhymeRules { assonance });
        lex
    }
}

fn fixture() -> Fixture {
    let lex = Lexicon::pinned().unwrap();
    let oracle = oracle();
    let mut words: Vec<String> = oracle
        .keys()
        .filter(|w| !w.contains('-') && lex.lookup(w).is_some())
        .cloned()
        .collect();
    words.sort();
    let mut by_tail: HashMap<Vec<String>, Vec<String>> = HashMap::new();
    let mut by_vowels: HashMap<Vec<String>, Vec<String>> = HashMap::new();
    for w in &words {
        if let Some((t, _, v)) = split(&oracle[w][0]) {
            by_tail.entry(t).or_default().push(w.clone());
            by_vowels.entry(v).or_default().push(w.clone());
        }
    }
    let collect = |m: HashMap<Vec<String>, Vec<String>>| {
        let mut g: Vec<Vec<String>> = m.into_values().filter(|g| g.len() >= 3).collect();
        g.sort();
        g
    };
    Fixture {
        lex,
        oracle,
        words,
        groups: collect(by_tail),
        vgroups: collect(by_vowels),
    }
}

#[test]
fn word_pairs_agree_with_the_oracle_and_are_symmetric() {
    let f = fixture();
    let iters = env_or("SYNTAXEON_FUZZ_ITERS", 20_000);
    for assonance in [false, true] {
        let lex = f.lex(assonance);
        let mut r = Rng(env_or("SYNTAXEON_FUZZ_SEED", 0x9E37_79B9_7F4A_7C15));
        let mut yes = 0;
        for _ in 0..iters {
            // A third of the pairs are random, the rest come from one tail group or one vowel
            // group so that rhymes (and near rhymes) actually occur.
            let (a, b) = match r.below(3) {
                0 => (
                    &f.words[r.below(f.words.len())],
                    &f.words[r.below(f.words.len())],
                ),
                k => {
                    let gs = if k == 1 { &f.groups } else { &f.vgroups };
                    let g = &gs[r.below(gs.len())];
                    (&g[r.below(g.len())], &g[r.below(g.len())])
                }
            };
            let got = words_rhyme(&lex, a, b).unwrap();
            assert_eq!(
                got,
                rhyme(&f.oracle, a, b, assonance),
                "{a}/{b} (assonance {assonance})"
            );
            assert_eq!(got, words_rhyme(&lex, b, a).unwrap(), "symmetry {a}/{b}");
            yes += usize::from(got);
        }
        assert!(
            yes * 20 > usize::try_from(iters).unwrap(),
            "too few rhyming pairs: {yes} (assonance {assonance})"
        );
        for w in f.words.iter().take(300) {
            assert!(!words_rhyme(&lex, w, w).unwrap(), "{w} rhymes with itself");
        }
    }
}

/// Assonance only adds rhymes: whatever rhymes perfectly still rhymes, and a pair that rhymes
/// only by assonance rhymes exactly when the project allows it.
#[test]
fn assonance_only_widens_what_rhymes() {
    let fixture = fixture();
    let (strict, loose) = (fixture.lex(false), fixture.lex(true));
    let mut rng = Rng(env_or("SYNTAXEON_FUZZ_SEED", 0xA550) + 3);
    let mut widened = 0;
    for _ in 0..env_or("SYNTAXEON_FUZZ_ITERS", 20_000) {
        let group = &fixture.vgroups[rng.below(fixture.vgroups.len())];
        let (first, second) = (
            &group[rng.below(group.len())],
            &group[rng.below(group.len())],
        );
        let (was, is) = (
            words_rhyme(&strict, first, second).unwrap(),
            words_rhyme(&loose, first, second).unwrap(),
        );
        assert!(
            !was || is,
            "{first}/{second} rhymes perfectly but not with assonance on"
        );
        widened += usize::from(!was && is);
    }
    assert!(widened > 100, "assonance never mattered: {widened}");
}

#[test]
fn rhyme_lists_match_pairwise_verdicts() {
    let f = fixture();
    let mut r = Rng(env_or("SYNTAXEON_FUZZ_SEED", 7) + 1);
    for _ in 0..env_or("SYNTAXEON_FUZZ_ITERS", 20_000).min(40) {
        let w = &f.words[r.below(f.words.len())];
        let list = rhymes_with(&f.lex, w).unwrap();
        assert!(!list.contains(&w.as_str()));
        for x in list.iter().take(25) {
            assert!(words_rhyme(&f.lex, w, x).unwrap(), "{w}/{x}");
        }
        for _ in 0..25 {
            let x = &f.words[r.below(f.words.len())];
            let want = x != w && rhyme(&f.oracle, w, x, false);
            assert_eq!(list.contains(&x.as_str()), want, "{w}/{x}");
        }
    }
}

#[test]
fn scheme_tables_agree() {
    for n in 0..12 {
        assert_eq!(schemes(n), oracle_schemes(n), "n = {n}");
    }
}

#[test]
fn stanzas_agree_with_the_oracle() {
    let f = fixture();
    for assonance in [false, true] {
        let lex = f.lex(assonance);
        let mut r = Rng(env_or("SYNTAXEON_FUZZ_SEED", 0x1234_5678_9ABC_DEF1));
        let (mut valid, mut invalid) = (0, 0);
        for _ in 0..env_or("SYNTAXEON_FUZZ_ITERS", 20_000) {
            let n = 1 + r.below(8);
            let mut verses: Vec<String> = Vec::new();
            // Build from one or two groups, then sometimes corrupt one verse.
            let gs = if assonance && r.below(2) == 0 {
                &f.vgroups
            } else {
                &f.groups
            };
            let g1 = &gs[r.below(gs.len())];
            let g2 = &gs[r.below(gs.len())];
            for i in 0..n {
                let g = if (i / 2) % 2 == 0 || r.below(4) == 0 {
                    g1
                } else {
                    g2
                };
                verses.push(g[r.below(g.len())].clone());
            }
            if r.below(3) == 0 {
                let i = r.below(n);
                verses[i] = f.words[r.below(f.words.len())].clone();
            }
            let refs: Vec<&str> = verses.iter().map(String::as_str).collect();
            let got = find_scheme(&lex, &refs);
            let want = n >= 2 && stanza_ok(&f.oracle, &refs, assonance);
            assert_eq!(got.is_ok(), want, "{refs:?} (assonance {assonance})");
            if let Ok(s) = &got {
                assert!(oracle_schemes(n).contains(s), "{refs:?} -> {s:?}");
            }
            if want {
                valid += 1;
            } else {
                invalid += 1;
            }
        }
        assert!(
            valid > 100 && invalid > 100,
            "unbalanced: {valid} valid / {invalid} invalid (assonance {assonance})"
        );
    }
}

/// The kind of relation, classified from the raw dictionary: perfect if any pair of
/// pronunciations rhymes perfectly, else assonant if any assonates, else identical if the words
/// are the same or any pair is an identical rhyme, else none.
fn oracle_kind(o: &Oracle, a: &str, b: &str) -> RhymeKind {
    if a == b {
        return RhymeKind::Identical;
    }
    let pairs = || {
        o[a].iter()
            .flat_map(|x| o[b].iter().map(move |y| (x, y)))
            .filter_map(|(x, y)| Some((split(x)?, split(y)?)))
    };
    if pairs().any(|((tx, ox, _), (ty, oy, _))| tx == ty && ox != oy) {
        RhymeKind::Perfect
    } else if pairs().any(|((tx, ox, vx), (ty, oy, vy))| !(tx == ty && ox == oy) && vx == vy) {
        RhymeKind::Assonant
    } else if pairs().any(|((tx, ox, _), (ty, oy, _))| tx == ty && ox == oy) {
        RhymeKind::Identical
    } else {
        RhymeKind::None
    }
}

#[test]
fn rhyme_kinds_agree_with_the_oracle() {
    let f = fixture();
    let lex = f.lex(true);
    let mut r = Rng(env_or("SYNTAXEON_FUZZ_SEED", 0xC1A5) + 5);
    let mut seen = [0_usize; 4];
    for _ in 0..env_or("SYNTAXEON_FUZZ_ITERS", 20_000) {
        let (a, b) = match r.below(3) {
            0 => (
                &f.words[r.below(f.words.len())],
                &f.words[r.below(f.words.len())],
            ),
            k => {
                let gs = if k == 1 { &f.groups } else { &f.vgroups };
                let g = &gs[r.below(gs.len())];
                (&g[r.below(g.len())], &g[r.below(g.len())])
            }
        };
        let got = rhyme_kind(&lex, a, b).unwrap();
        assert_eq!(got, oracle_kind(&f.oracle, a, b), "{a}/{b}");
        seen[got as usize] += 1;
    }
    assert!(
        seen.iter().all(|n| *n > 20),
        "every kind should occur: {seen:?}"
    );
}
