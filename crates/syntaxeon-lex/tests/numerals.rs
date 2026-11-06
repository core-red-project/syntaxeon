#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
//! The end word of a numeral (`spec/PROSODY.md` §5) against an independent construction: build
//! the whole English cardinal, then take its last word.

use syntaxeon_lex::literary::cardinal_last_word;

const SMALL: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALE: [&str; 7] = [
    "",
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion",
];

/// Words of 1..=999.
fn below_thousand(n: u64, out: &mut Vec<String>) {
    if n >= 100 {
        out.push(SMALL[usize::try_from(n / 100).unwrap()].to_string());
        out.push("hundred".to_string());
    }
    let rest = n % 100;
    if rest == 0 {
        return;
    }
    if rest < 20 {
        out.push(SMALL[usize::try_from(rest).unwrap()].to_string());
    } else if rest.is_multiple_of(10) {
        out.push(TENS[usize::try_from(rest / 10).unwrap()].to_string());
    } else {
        out.push(format!(
            "{}-{}",
            TENS[usize::try_from(rest / 10).unwrap()],
            SMALL[usize::try_from(rest % 10).unwrap()]
        ));
    }
}

/// The full short-scale cardinal without `and`.
fn cardinal(n: u64) -> Vec<String> {
    if n == 0 {
        return vec!["zero".to_string()];
    }
    let mut groups = Vec::new();
    let mut rest = n;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }
    let mut out = Vec::new();
    for (k, g) in groups.iter().enumerate().rev() {
        if *g == 0 {
            continue;
        }
        below_thousand(*g, &mut out);
        if k > 0 {
            out.push(SCALE[k].to_string());
        }
    }
    out
}

fn expected(n: u64) -> String {
    cardinal(n).last().unwrap().clone()
}

#[test]
fn small_numbers_match_the_full_cardinal() {
    for n in 0..=400_000 {
        assert_eq!(cardinal_last_word(n), expected(n), "{n}");
    }
}

#[test]
fn every_scale_boundary_and_the_largest_values_match() {
    let mut cases: Vec<u64> = vec![u64::MAX, u64::MAX - 1, i64::MAX as u64, i64::MAX as u64 + 1];
    let mut p = 1_u64;
    while let Some(next) = p.checked_mul(10) {
        for d in [0, 1, 2, 11, 20, 99, 100, 101, 999] {
            cases.extend([
                p.saturating_add(d),
                p.saturating_mul(7).saturating_add(d),
                next - 1,
            ]);
        }
        p = next;
    }
    for n in cases {
        assert_eq!(cardinal_last_word(n), expected(n), "{n}");
    }
}

#[test]
fn random_numbers_match() {
    let mut x = 0x9E37_79B9_7F4A_7C15_u64;
    for _ in 0..200_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        // Mask to a random width so every magnitude is exercised, and zero some groups.
        let n = x >> (x % 64);
        let n = if x.is_multiple_of(3) { n - n % 1000 } else { n };
        assert_eq!(cardinal_last_word(n), expected(n), "{n}");
    }
}
