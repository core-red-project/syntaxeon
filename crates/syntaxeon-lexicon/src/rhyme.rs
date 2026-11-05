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

pub fn words_rhyme(lex: &Lexicon, a: &str, b: &str) -> Result<bool, Unpronounceable> {
    let pa = lex
        .lookup(a)
        .ok_or_else(|| Unpronounceable(a.to_string()))?;
    let pb = lex
        .lookup(b)
        .ok_or_else(|| Unpronounceable(b.to_string()))?;
    Ok(pa.iter().any(|x| pb.iter().any(|y| pron_rhymes(x, y))))
}

/// The rhyming tail of a pronunciation as `ARPAbet` without stress digits (`"EH N"`).
#[must_use]
pub fn tail_string(p: &[Phone]) -> Option<String> {
    let (tail, _) = split(p)?;
    Some(tail.iter().map(|x| x.base).collect::<Vec<_>>().join(" "))
}
