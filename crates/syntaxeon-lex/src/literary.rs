//! Literary layer: verses, stanzas, end words (`spec/PROSE_GRAMMAR.md` §3, `spec/PROSODY.md` §5–§6).

use std::ops::Range;

use crate::Span;
use crate::lexer::{Lexed, Tok, Token};

/// `SXP-203`: the verse has no extractable end word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoEndWord;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verse {
    pub line: usize,
    /// Indices into `Lexed::tokens`.
    pub tokens: Range<usize>,
    pub end_word: Result<String, NoEndWord>,
    /// The token the end word comes from, so an error can point at it.
    pub end_span: Option<Span>,
    /// The words of the verse, without any aside or the punctuation that ends it.
    pub span: Span,
    /// Exactly `Otherwise:` or `So be it.` — exempt from rhyme (`spec/PROSODY.md` §6).
    pub structural: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stanza {
    pub verses: Vec<Verse>,
}

/// Groups tokens into verses (physical lines) and stanzas (blank-line separated).
/// A line holding only an aside has no tokens, is dropped, and does not split a stanza.
#[must_use]
pub fn stanzas(lexed: &Lexed) -> Vec<Stanza> {
    let mut out = Vec::new();
    let mut current: Vec<Verse> = Vec::new();
    let mut j = 0;
    for (idx, &blank) in lexed.blank_lines.iter().enumerate() {
        let line = idx + 1;
        let start = j;
        while j < lexed.tokens.len() && lexed.tokens[j].line == line {
            j += 1;
        }
        if blank {
            if !current.is_empty() {
                out.push(Stanza {
                    verses: std::mem::take(&mut current),
                });
            }
        } else if j > start {
            let toks = &lexed.tokens[start..j];
            let (end_word, end_span) = end_word(toks);
            current.push(Verse {
                line,
                tokens: start..j,
                end_word,
                end_span,
                // The words, not the punctuation that ends the verse.
                span: Span {
                    start: toks[0].span.start,
                    end: toks
                        .iter()
                        .rev()
                        .find(|t| !matches!(t.tok, Tok::Punct(_)))
                        .map_or(toks[toks.len() - 1].span.end, |t| t.span.end),
                },
                structural: is_structural(toks),
            });
        }
    }
    if !current.is_empty() {
        out.push(Stanza { verses: current });
    }
    out
}

fn is_structural(toks: &[Token]) -> bool {
    let shape: Vec<&Tok> = toks.iter().map(|t| &t.tok).collect();
    let w = |s: &str| Tok::Word(s.to_string());
    matches!(shape.as_slice(), [a, Tok::Punct(':')] if **a == w("otherwise"))
        || matches!(shape.as_slice(), [a, b, c, Tok::Punct('.')]
            if **a == w("so") && **b == w("be") && **c == w("it"))
}

/// Hyphenated words rhyme on their last component: `seventy-five` stresses `seventy`, but a
/// reader hears `five` (`spec/PROSODY.md` §5).
fn last_component(word: &str) -> String {
    word.rsplit('-').next().unwrap_or(word).to_string()
}

/// The tokens whose last word is the verse's end word. A header does not rhyme on its fixed
/// closer (`, then`, `, do`, `times`) but on the words before it, and a one-line function header
/// rhymes on its title (`spec/PROSODY.md` §6); everything else is the verse as written.
fn rhyming_part(toks: &[Token]) -> &[Token] {
    let n = toks.len();
    if !matches!(toks.last().map(|t| &t.tok), Some(Tok::Punct(':'))) {
        return toks;
    }
    let body = &toks[..n - 1];
    let is = |t: &Token, w: &str| matches!(&t.tok, Tok::Word(x) if x == w);
    let comma = |t: &Token| matches!(t.tok, Tok::Punct(','));
    if let [.., c, w] = body
        && comma(c)
        && (is(w, "then") || is(w, "do"))
    {
        return &body[..body.len() - 2];
    }
    if let [.., w] = body
        && is(w, "times")
    {
        return &body[..body.len() - 1];
    }
    if body.first().is_some_and(|t| is(t, "to")) {
        let title_end = body.iter().position(comma).unwrap_or(body.len());
        return &body[1..title_end];
    }
    toks
}

/// The end word of a verse and the token it comes from (`None` when there is no word).
fn end_word(toks: &[Token]) -> (Result<String, NoEndWord>, Option<Span>) {
    let toks = rhyming_part(toks);
    let Some(last) = toks.iter().rev().find(|t| !matches!(t.tok, Tok::Punct(_))) else {
        return (Err(NoEndWord), None);
    };
    let word = match &last.tok {
        Tok::Word(w) => Ok(last_component(w)),
        Tok::Int(n) => Ok(last_component(&cardinal_last_word(n.unsigned_abs()))),
        Tok::Float { text, .. } => text
            .chars()
            .last()
            .and_then(|c| c.to_digit(10))
            .map(|digit| ONES[digit as usize].to_string())
            .ok_or(NoEndWord),
        Tok::Str(s) => last_word_in(s).ok_or(NoEndWord),
        Tok::Punct(_) => Err(NoEndWord),
    };
    (word, Some(last.span))
}

/// Last alphabetic word inside quoted text.
fn last_word_in(s: &str) -> Option<String> {
    s.split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '-'))
        .map(|w| w.trim_matches(|c| c == '\'' || c == '-'))
        .rfind(|w| w.chars().any(char::is_alphabetic))
        .map(|w| last_component(&w.to_lowercase()))
}

const ONES: [&str; 20] = [
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
const SCALES: [(u64, &str); 6] = [
    (1_000, "thousand"),
    (1_000_000, "million"),
    (1_000_000_000, "billion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000_000_000, "quintillion"),
];

/// Last word of the short-scale English cardinal without `and`
/// (`spec/PROSODY.md` §5): 10→ten, 21→twenty-one, 100→hundred, 1025→twenty-five.
#[must_use]
pub fn cardinal_last_word(n: u64) -> String {
    if n == 0 {
        return "zero".to_string();
    }
    let r = n % 1000;
    if r != 0 {
        let t = (r % 100) as usize;
        return match t {
            0 => "hundred".to_string(),
            1..=19 => ONES[t].to_string(),
            _ if t.is_multiple_of(10) => TENS[t / 10].to_string(),
            _ => format!("{}-{}", TENS[t / 10], ONES[t % 10]),
        };
    }
    // r == 0: the last spoken word is the lowest scale word present.
    SCALES
        .iter()
        .find(|(unit, _)| {
            unit.checked_mul(1000)
                .is_none_or(|next| !n.is_multiple_of(next))
        })
        .map_or("thousand", |(_, name)| name)
        .to_string()
}
