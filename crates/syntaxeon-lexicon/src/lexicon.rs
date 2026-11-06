//! Pronunciation lexicon in `CMUdict` format (`spec/PROSODY.md` §2–§3, §10).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::sha256::sha256_hex;

const VOWELS: [&str; 15] = [
    "AA", "AE", "AH", "AO", "AW", "AY", "EH", "ER", "EY", "IH", "IY", "OW", "OY", "UH", "UW",
];
const CONSONANTS: [&str; 24] = [
    "B", "CH", "D", "DH", "F", "G", "HH", "JH", "K", "L", "M", "N", "NG", "P", "R", "S", "SH", "T",
    "TH", "V", "W", "Y", "Z", "ZH",
];

/// One `ARPAbet` phoneme. Vowels carry a stress digit, consonants never do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phone {
    pub base: &'static str,
    pub stress: Option<u8>,
}

impl Phone {
    #[must_use]
    pub fn parse(token: &str) -> Option<Phone> {
        let (base, stress) = match token.chars().last() {
            Some(c) if c.is_ascii_digit() => (
                &token[..token.len() - 1],
                Some(u8::try_from(c.to_digit(10)?).ok()?),
            ),
            _ => (token, None),
        };
        if let Some(v) = VOWELS.iter().find(|v| **v == base) {
            return matches!(stress, Some(0..=2)).then_some(Phone { base: v, stress });
        }
        let c = CONSONANTS.iter().find(|c| **c == base)?;
        stress.is_none().then_some(Phone {
            base: c,
            stress: None,
        })
    }

    #[must_use]
    pub fn is_vowel(&self) -> bool {
        self.stress.is_some()
    }
}

pub type Pron = Vec<Phone>;

#[derive(Debug, PartialEq, Eq)]
pub enum LexiconError {
    BadEntry { line: usize },
}

impl fmt::Display for LexiconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexiconError::BadEntry { line } => write!(f, "malformed lexicon entry at line {line}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PragmaError {
    /// `SXP-302`
    Malformed,
    /// `SXP-303`
    Redefines,
}

impl PragmaError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            PragmaError::Malformed => "SXP-302",
            PragmaError::Redefines => "SXP-303",
        }
    }
}

/// Which rhymes are valid (`spec/PROSODY.md` §4). Perfect rhyme is always valid; a project can
/// turn the looser kinds off in its `rhythm.toml` (ADR-015).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RhymeRules {
    /// Assonance: the same vowels from the stressed vowel on, whatever the consonants.
    pub assonance: bool,
}

impl Default for RhymeRules {
    fn default() -> Self {
        RhymeRules { assonance: true }
    }
}

impl RhymeRules {
    /// Only perfect rhyme.
    #[must_use]
    pub fn perfect() -> Self {
        RhymeRules { assonance: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Lexicon {
    /// The dictionary, shared between clones (a clone per analyzed file must stay cheap).
    entries: Arc<HashMap<String, Vec<Pron>>>,
    /// Words added by pragmas; never overlaps `entries`.
    extra: HashMap<String, Vec<Pron>>,
    /// Common English words, for `rhymes` suggestions; empty unless loaded.
    common: Arc<HashSet<String>>,
    /// Frequency tier (10 most common, then 20) of the words that have one; see [`Lexicon::tier`].
    tiers: Arc<HashMap<String, u8>>,
    rules: RhymeRules,
}

impl Lexicon {
    /// Parses `CMUdict` text: `WORD  PH PH ...`, `WORD(2)` alternates, `;;;` comments.
    pub fn parse(text: &str) -> Result<Lexicon, LexiconError> {
        let mut entries: HashMap<String, Vec<Pron>> = HashMap::new();
        for (i, raw) in text.lines().enumerate() {
            // Some CMUdict entries carry a trailing `# note`.
            let line = raw.split_once('#').map_or(raw, |(code, _)| code).trim();
            if line.is_empty() || line.starts_with(";;;") {
                continue;
            }
            let bad = || LexiconError::BadEntry { line: i + 1 };
            let mut tokens = line.split_whitespace();
            let word = tokens.next().ok_or_else(bad)?;
            let word = word.split('(').next().unwrap_or(word).to_lowercase();
            let pron: Option<Pron> = tokens.map(Phone::parse).collect();
            let pron = pron.filter(|p| !p.is_empty()).ok_or_else(bad)?;
            entries.entry(word).or_default().push(pron);
        }
        Ok(Lexicon {
            entries: Arc::new(entries),
            ..Lexicon::default()
        })
    }

    /// Adds a pragma word (`spec/PROSODY.md` §3). Never overrides an existing word.
    pub fn add_pragma(&mut self, word: &str, phones: &str) -> Result<(), PragmaError> {
        let pron: Option<Pron> = phones.split_whitespace().map(Phone::parse).collect();
        let pron = pron
            .filter(|p| !p.is_empty())
            .ok_or(PragmaError::Malformed)?;
        let key = word.to_lowercase();
        if self.get(&key).is_some() {
            return Err(PragmaError::Redefines);
        }
        self.extra.insert(key, vec![pron]);
        Ok(())
    }

    /// Every word with its pronunciations, in no particular order.
    pub fn words(&self) -> impl Iterator<Item = (&str, &[Pron])> {
        self.entries
            .iter()
            .chain(&self.extra)
            .map(|(w, p)| (w.as_str(), p.as_slice()))
    }

    /// Loads the common-word list (one lowercase word per line).
    pub fn set_common_words(&mut self, text: &str) {
        self.common = Arc::new(
            text.lines()
                .map(str::trim)
                .filter(|w| !w.is_empty())
                .map(String::from)
                .collect(),
        );
    }

    /// Sets which rhymes are valid for this lexicon.
    pub fn set_rhyme_rules(&mut self, rules: RhymeRules) {
        self.rules = rules;
    }

    #[must_use]
    pub fn rhyme_rules(&self) -> RhymeRules {
        self.rules
    }

    /// Loads the frequency tiers: `word tier` per line, `#` lines ignored.
    pub fn set_word_tiers(&mut self, text: &str) {
        self.tiers = Arc::new(
            text.lines()
                .filter(|l| !l.starts_with('#'))
                .filter_map(|l| {
                    let (w, t) = l.split_once(' ')?;
                    Some((w.to_string(), t.trim().parse().ok()?))
                })
                .collect(),
        );
    }

    /// How common `word` is, smaller meaning more common: 10 and 20 from the tier list, 35 for
    /// any other common word, 99 for everything else. Only used to order `rhymes` output.
    #[must_use]
    pub fn tier(&self, word: &str) -> u8 {
        self.tiers
            .get(word)
            .copied()
            .unwrap_or(if self.is_common(word) { 35 } else { 99 })
    }

    /// Whether `word` is in the common-word list (`false` when no list is loaded).
    #[must_use]
    pub fn is_common(&self, word: &str) -> bool {
        self.common.contains(word)
    }

    /// Whole word first, then the last hyphen component (`spec/PROSODY.md` §2).
    #[must_use]
    pub fn lookup(&self, word: &str) -> Option<&[Pron]> {
        let w = word.to_lowercase();
        self.get(&w)
            .or_else(|| w.rsplit_once('-').and_then(|(_, last)| self.get(last)))
            .map(Vec::as_slice)
    }

    fn get(&self, key: &str) -> Option<&Vec<Pron>> {
        self.entries.get(key).or_else(|| self.extra.get(key))
    }
}

const PINNED_DICT: &str = include_str!("../lexicon/cmudict.dict");
const PINNED_LOCK: &str = include_str!("../lexicon/lexicon.lock");
const PINNED_COMMON: &str = include_str!("../lexicon/common-words.txt");
const PINNED_TIERS: &str = include_str!("../lexicon/word-tiers.txt");

/// `SXP-304`: the vendored lexicon does not match `lexicon.lock` (tooling error).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinError(pub String);

impl PinError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        "SXP-304"
    }
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn verify_hash(data: &str, lock: &str, key: &str, what: &str) -> Result<(), PinError> {
    let prefix = format!("{key} = \"");
    let expected = lock
        .lines()
        .find_map(|l| l.strip_prefix(prefix.as_str())?.strip_suffix('"'))
        .ok_or_else(|| PinError(format!("lexicon.lock has no {key} entry")))?;
    let found = sha256_hex(data.as_bytes());
    if found == expected {
        Ok(())
    } else {
        Err(PinError(format!(
            "{what} sha256 mismatch: expected {expected}, found {found}"
        )))
    }
}

/// Checks `dict` against the `sha256 = "..."` line of `lock`.
pub fn verify_pin(dict: &str, lock: &str) -> Result<(), PinError> {
    verify_hash(dict, lock, "sha256", "lexicon")
}

/// Checks the common-word list against the `common_words_sha256 = "..."` line of `lock`.
pub fn verify_common_pin(list: &str, lock: &str) -> Result<(), PinError> {
    verify_hash(list, lock, "common_words_sha256", "common-word list")
}

/// Checks the frequency-tier list against the `word_tiers_sha256 = "..."` line of `lock`.
pub fn verify_tiers_pin(list: &str, lock: &str) -> Result<(), PinError> {
    verify_hash(list, lock, "word_tiers_sha256", "word-tier list")
}

impl Lexicon {
    /// The vendored, hash-verified lexicon (`spec/PROSODY.md` §2). Verified and parsed once.
    pub fn pinned() -> Result<Lexicon, PinError> {
        static CELL: OnceLock<Result<Lexicon, PinError>> = OnceLock::new();
        CELL.get_or_init(|| {
            verify_pin(PINNED_DICT, PINNED_LOCK)?;
            verify_common_pin(PINNED_COMMON, PINNED_LOCK)?;
            verify_tiers_pin(PINNED_TIERS, PINNED_LOCK)?;
            let mut lex = Lexicon::parse(PINNED_DICT).map_err(|e| PinError(e.to_string()))?;
            lex.set_common_words(PINNED_COMMON);
            lex.set_word_tiers(PINNED_TIERS);
            Ok(lex)
        })
        .clone()
    }
}
