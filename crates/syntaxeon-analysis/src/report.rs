//! A phonetic report on the verses of a file, for tools that show or play the sound of the
//! program (`syntaxeon-lsp`, ADR-020). It states facts; it never produces audio.

use syntaxeon_core::Span;
use syntaxeon_lex::lexer::Lexed;
use syntaxeon_lex::literary::Stanza;
use syntaxeon_lexicon::lexicon::{Lexicon, Phone, RhymeRules};
use syntaxeon_lexicon::rhyme::{RhymeKind, rhyme_kind, tail_string};
use syntaxeon_lexicon::scheme::{StanzaError, find_scheme};

/// One verse and, if it counts, the sound of its end word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseInfo {
    /// 1-based source line.
    pub line: usize,
    /// The words of the verse.
    pub span: Span,
    /// `Otherwise:` and `So be it.`, which take no part in rhyme.
    pub structural: bool,
    pub end_word: Option<String>,
    /// Where the end word is written.
    pub end_span: Option<Span>,
    /// Every pronunciation of the end word, as `ARPAbet` with stress digits (`"EH1 N"`). The
    /// first is the one to recite. Empty when the word is unknown to the lexicon.
    pub prons: Vec<String>,
    /// The rhyme tail of each pronunciation, without stress digits (`"EH N"`).
    pub tails: Vec<String>,
    /// The group letter this verse has in the stanza's scheme (`'A'`, `'B'`, …).
    pub group: Option<char>,
    /// The first verse of its group, as an index into the stanza's verses; `None` for the
    /// first itself and for verses outside any group.
    pub leader: Option<usize>,
    /// How this verse sounds against its leader.
    pub kind: Option<RhymeKind>,
    /// Whether that relation is accepted under the project's rhyme rules.
    pub rhymes: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StanzaInfo {
    pub verses: Vec<VerseInfo>,
    /// The scheme as letters (`"AABB"`), when the stanza has one to report.
    pub scheme: Option<String>,
    /// Whether every group rhymes under the rules.
    pub valid: bool,
}

fn phones(p: &[Phone]) -> String {
    p.iter()
        .map(|x| match x.stress {
            Some(d) => format!("{}{d}", x.base),
            None => x.base.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A lexicon with the file's pronunciation pragmas applied; a bad pragma is skipped (it is
/// reported by the prosody check, not here).
#[must_use]
pub fn lexicon_for(lexed: &Lexed, rules: RhymeRules) -> Option<Lexicon> {
    let mut lex = Lexicon::pinned().ok()?;
    lex.set_rhyme_rules(rules);
    for p in &lexed.pragmas {
        let _ = lex.add_pragma(&p.word, &p.phones);
    }
    Some(lex)
}

/// The report for every stanza of `stanzas`. Works whatever the other phases found, so an editor
/// can show the sound of a program that does not yet type-check.
#[must_use]
pub fn verse_report(stanzas: &[Stanza], lex: &Lexicon) -> Vec<StanzaInfo> {
    stanzas.iter().map(|s| stanza_report(s, lex)).collect()
}

fn stanza_report(stanza: &Stanza, lex: &Lexicon) -> StanzaInfo {
    let mut verses: Vec<VerseInfo> = stanza
        .verses
        .iter()
        .map(|v| {
            let end_word = v.end_word.clone().ok();
            // Structural verses take no part in rhyme, so they have no sound to report.
            let prons = end_word
                .as_deref()
                .filter(|_| !v.structural)
                .and_then(|w| lex.lookup(w))
                .unwrap_or_default();
            VerseInfo {
                line: v.line,
                span: v.span,
                structural: v.structural,
                end_span: v.end_span,
                prons: prons.iter().map(|p| phones(p)).collect(),
                tails: prons.iter().filter_map(|p| tail_string(p)).collect(),
                end_word,
                group: None,
                leader: None,
                kind: None,
                rhymes: None,
            }
        })
        .collect();
    // Counting verses with a pronounceable end word, in order, as indices into `verses`.
    let counting: Vec<usize> = (0..verses.len())
        .filter(|&i| !verses[i].structural && verses[i].end_word.is_some())
        .collect();
    let all_counting = stanza.verses.iter().filter(|v| !v.structural).count();
    let words: Vec<&str> = counting
        .iter()
        .filter_map(|&i| verses[i].end_word.as_deref())
        .collect();
    let (scheme, valid) = if counting.len() == all_counting {
        match find_scheme(lex, &words) {
            Ok(sc) => (Some(sc), true),
            Err(StanzaError::NoScheme { scheme, .. }) => (Some(scheme), false),
            Err(_) => (None, false),
        }
    } else {
        (None, false)
    };
    if let Some(scheme) = &scheme {
        for (k, &g) in scheme.iter().enumerate() {
            let i = counting[k];
            verses[i].group = char::from_u32('A' as u32 + u32::try_from(g).unwrap_or(0));
            let lead = scheme.iter().position(|&x| x == g).unwrap_or(k);
            if lead != k {
                let lead_i = counting[lead];
                verses[i].leader = Some(lead_i);
                let pair = (verses[lead_i].end_word.clone(), verses[i].end_word.clone());
                if let (Some(a), Some(b)) = pair {
                    verses[i].kind = rhyme_kind(lex, &a, &b).ok();
                    verses[i].rhymes = crate::words_rhyme_under(lex, &a, &b);
                }
            }
        }
    }
    StanzaInfo {
        scheme: scheme.map(|sc| {
            sc.iter()
                .map(|&g| char::from_u32('A' as u32 + u32::try_from(g).unwrap_or(0)).unwrap_or('?'))
                .collect()
        }),
        verses,
        valid,
    }
}
