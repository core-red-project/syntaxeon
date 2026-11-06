//! Permitted stanza schemes and the stanza check (`spec/PROSODY.md` §7).

use crate::lexicon::Lexicon;
use crate::rhyme::words_rhyme;

#[derive(Debug, PartialEq, Eq)]
pub enum StanzaError {
    /// `SXP-201`
    TooShort { count: usize },
    /// `SXP-301`
    Unpronounceable(String),
    /// `SXP-202`: the closest permitted scheme and the verse indices that fail it.
    NoScheme {
        scheme: Vec<usize>,
        failing: Vec<usize>,
    },
}

impl StanzaError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            StanzaError::TooShort { .. } => "SXP-201",
            StanzaError::Unpronounceable(_) => "SXP-301",
            StanzaError::NoScheme { .. } => "SXP-202",
        }
    }
}

/// Couplet chain; an odd count ends in one group of three.
fn couplets(n: usize) -> Vec<usize> {
    let (mut scheme, mut group, mut i) = (Vec::with_capacity(n), 0, 0);
    while i < n {
        let take = if n - i == 3 { 3 } else { 2 };
        scheme.extend(std::iter::repeat_n(group, take));
        group += 1;
        i += take;
    }
    scheme
}

/// Permitted schemes for `n` counting verses, in tie-break order.
#[must_use]
pub fn schemes(n: usize) -> Vec<Vec<usize>> {
    if n < 2 {
        return vec![];
    }
    let mut out = vec![couplets(n)];
    if n == 4 {
        out.push(vec![0, 1, 0, 1]);
        out.push(vec![0, 1, 1, 0]);
    }
    out
}

/// Checks the end words of a stanza's counting verses.
pub fn check_stanza(lex: &Lexicon, end_words: &[&str]) -> Result<(), StanzaError> {
    find_scheme(lex, end_words).map(|_| ())
}

/// Like [`check_stanza`], returning the first permitted scheme the stanza satisfies
/// (one group index per verse).
pub fn find_scheme(lex: &Lexicon, end_words: &[&str]) -> Result<Vec<usize>, StanzaError> {
    let candidates = schemes(end_words.len());
    if candidates.is_empty() {
        return Err(StanzaError::TooShort {
            count: end_words.len(),
        });
    }
    if let Some(w) = end_words.iter().find(|w| lex.lookup(w).is_none()) {
        return Err(StanzaError::Unpronounceable((*w).to_string()));
    }
    let mut best: Option<(Vec<usize>, Vec<usize>)> = None;
    for scheme in candidates {
        // Every pair in a group must rhyme, so the verdict does not depend on which verse
        // leads; a verse is reported when it fails against an earlier member of its group.
        let mut failing = Vec::new();
        for (i, &g) in scheme.iter().enumerate() {
            // Words were verified pronounceable above.
            let fails = (0..i)
                .filter(|&j| scheme[j] == g)
                .any(|j| !words_rhyme(lex, end_words[j], end_words[i]).unwrap_or(false));
            if fails {
                failing.push(i);
            }
        }
        if failing.is_empty() {
            return Ok(scheme);
        }
        if best.as_ref().is_none_or(|(_, f)| failing.len() < f.len()) {
            best = Some((scheme, failing));
        }
    }
    // `candidates` was non-empty, so a best scheme exists; the fallback is unreachable.
    Err(best.map_or(
        StanzaError::TooShort {
            count: end_words.len(),
        },
        |(scheme, failing)| StanzaError::NoScheme { scheme, failing },
    ))
}
