//! The rhyme function (`spec/PROSODY.md` §4).

use crate::lexicon::{Lexicon, Phone};

#[derive(Debug, PartialEq, Eq)]
pub struct Unpronounceable(pub String);

/// Splits a pronunciation into (tail, onset). `None` when it has no vowel.
fn split(p: &[Phone]) -> Option<(&[Phone], Option<&Phone>)> {
    let idx = p
        .iter()
        .rposition(|x| x.stress == Some(1))
        .or_else(|| p.iter().rposition(|x| x.stress == Some(2)))
        .or_else(|| p.iter().rposition(Phone::is_vowel))?;
    Some((&p[idx..], idx.checked_sub(1).map(|i| &p[i])))
}

#[must_use]
pub fn pron_rhymes(a: &[Phone], b: &[Phone]) -> bool {
    let (Some((ta, oa)), Some((tb, ob))) = (split(a), split(b)) else {
        return false;
    };
    ta.len() == tb.len()
        && ta.iter().zip(tb).all(|(x, y)| x.base == y.base)
        && oa.map(|p| p.base) != ob.map(|p| p.base)
}

/// Assonance (`spec/PROSODY.md` §4): the vowels from the stressed one to the end are the same
/// in order, and the pair is not an identical rhyme. Perfect rhyme implies it.
#[must_use]
pub fn pron_assonates(a: &[Phone], b: &[Phone]) -> bool {
    let (Some((ta, oa)), Some((tb, ob))) = (split(a), split(b)) else {
        return false;
    };
    let vowels = |t: &[Phone]| {
        t.iter()
            .filter(|p| p.is_vowel())
            .map(|p| p.base)
            .collect::<Vec<_>>()
    };
    let identical = ta.len() == tb.len()
        && ta.iter().zip(tb).all(|(x, y)| x.base == y.base)
        && oa.map(|p| p.base) == ob.map(|p| p.base);
    !identical && vowels(ta) == vowels(tb)
}

/// How two words relate by sound (`spec/PROSODY.md` §4), whatever the project's rules accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RhymeKind {
    /// Same tail, different onsets.
    Perfect,
    /// Same vowels from the stressed vowel on, different tails.
    Assonant,
    /// The same sound (the same word, or equal tail and onset): not a rhyme.
    Identical,
    None,
}

impl RhymeKind {
    /// A short name for tools: `perfect`, `assonant`, `identical`, `none`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            RhymeKind::Perfect => "perfect",
            RhymeKind::Assonant => "assonant",
            RhymeKind::Identical => "identical",
            RhymeKind::None => "none",
        }
    }
}

fn pron_identical(a: &[Phone], b: &[Phone]) -> bool {
    let (Some((ta, oa)), Some((tb, ob))) = (split(a), split(b)) else {
        return false;
    };
    ta.len() == tb.len()
        && ta.iter().zip(tb).all(|(x, y)| x.base == y.base)
        && oa.map(|p| p.base) == ob.map(|p| p.base)
}

/// The best relation between two words over all their pronunciations: perfect, else assonant,
/// else identical, else none. A word is identical to itself.
pub fn rhyme_kind(lex: &Lexicon, a: &str, b: &str) -> Result<RhymeKind, Unpronounceable> {
    let pa = lex
        .lookup(a)
        .ok_or_else(|| Unpronounceable(a.to_string()))?;
    let pb = lex
        .lookup(b)
        .ok_or_else(|| Unpronounceable(b.to_string()))?;
    if a.to_lowercase() == b.to_lowercase() {
        return Ok(RhymeKind::Identical);
    }
    let any =
        |f: &dyn Fn(&[Phone], &[Phone]) -> bool| pa.iter().any(|x| pb.iter().any(|y| f(x, y)));
    Ok(if any(&pron_rhymes) {
        RhymeKind::Perfect
    } else if any(&pron_assonates) {
        RhymeKind::Assonant
    } else if any(&pron_identical) {
        RhymeKind::Identical
    } else {
        RhymeKind::None
    })
}

/// Whether two words rhyme under the lexicon's rules: perfectly, or by assonance when allowed.
/// A word never rhymes with itself, whatever its alternative pronunciations do to each other.
pub fn words_rhyme(lex: &Lexicon, a: &str, b: &str) -> Result<bool, Unpronounceable> {
    Ok(match rhyme_kind(lex, a, b)? {
        RhymeKind::Perfect => true,
        RhymeKind::Assonant => lex.rhyme_rules().assonance,
        RhymeKind::Identical | RhymeKind::None => false,
    })
}

/// The rhyming tail of a pronunciation as `ARPAbet` without stress digits (`"EH N"`).
#[must_use]
pub fn tail_string(p: &[Phone]) -> Option<String> {
    let (tail, _) = split(p)?;
    Some(tail.iter().map(|x| x.base).collect::<Vec<_>>().join(" "))
}

/// Every word in the lexicon that rhymes with `word` (`spec/PROSODY.md` §4), `word` excluded,
/// ordered by how common the word is ([`Lexicon::tier`]), then by length, then alphabetically,
/// so the result is deterministic.
pub fn rhymes_with<'a>(lex: &'a Lexicon, word: &str) -> Result<Vec<&'a str>, Unpronounceable> {
    let key = word.to_lowercase();
    let targets = lex
        .lookup(&key)
        .ok_or_else(|| Unpronounceable(word.to_string()))?;
    let mut found: Vec<&str> = lex
        .words()
        .filter(|(w, prons)| {
            *w != key
                && prons
                    .iter()
                    .any(|p| targets.iter().any(|t| pron_rhymes(t, p)))
        })
        .map(|(w, _)| w)
        .collect();
    found.sort_by(|a, b| {
        lex.tier(a)
            .cmp(&lex.tier(b))
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    Ok(found)
}

/// Words that rhyme with `word` only by assonance (no perfect rhyme between any of their
/// pronunciations), in the same order as [`rhymes_with`]. Independent of the lexicon's rules:
/// whether such a pair is *valid* is [`words_rhyme`]'s business.
pub fn near_rhymes_with<'a>(lex: &'a Lexicon, word: &str) -> Result<Vec<&'a str>, Unpronounceable> {
    let key = word.to_lowercase();
    let targets = lex
        .lookup(&key)
        .ok_or_else(|| Unpronounceable(word.to_string()))?;
    let mut found: Vec<&str> = lex
        .words()
        .filter(|(w, prons)| {
            *w != key
                && prons
                    .iter()
                    .any(|p| targets.iter().any(|t| pron_assonates(t, p)))
                && !prons
                    .iter()
                    .any(|p| targets.iter().any(|t| pron_rhymes(t, p)))
        })
        .map(|(w, _)| w)
        .collect();
    found.sort_by(|a, b| {
        lex.tier(a)
            .cmp(&lex.tier(b))
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    Ok(found)
}
