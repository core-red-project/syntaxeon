//! Drives the pronunciation lexicon over a lexed file (`spec/PROSODY.md` §3, §7–§8).

use syntaxeon_lexicon::lexicon::{Lexicon, PragmaError};
use syntaxeon_lexicon::scheme::{StanzaError, check_stanza};

use crate::Diagnostic;
use crate::lexer::Lexed;
use crate::literary::Stanza;

/// Applies the file's pragmas to `lex`, then checks every stanza.
pub fn check(lexed: &Lexed, stanzas: &[Stanza], lex: &mut Lexicon) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for p in &lexed.pragmas {
        if let Err(e) = lex.add_pragma(&p.word, &p.phones) {
            let message = match e {
                PragmaError::Malformed => format!("malformed pronunciation for {:?}", p.word),
                PragmaError::Redefines => format!("{:?} is already in the lexicon", p.word),
            };
            out.push(Diagnostic {
                code: e.code(),
                message,
                line: p.line,
            });
        }
    }
    for stanza in stanzas {
        let Some(first) = stanza.verses.first() else {
            continue;
        };
        let mut words: Vec<&str> = Vec::new();
        let mut broken = false;
        for v in stanza.verses.iter().filter(|v| !v.structural) {
            if let Ok(w) = &v.end_word {
                words.push(w);
            } else {
                broken = true;
                out.push(Diagnostic {
                    code: "SXP-203",
                    message: "verse has no extractable end word".to_string(),
                    line: v.line,
                });
            }
        }
        if broken {
            continue;
        }
        if let Err(e) = check_stanza(lex, &words) {
            let message = match &e {
                StanzaError::TooShort { count } => {
                    format!("stanza has {count} counting verse(s); at least 2 required")
                }
                StanzaError::Unpronounceable(w) => format!("{w:?} is not in the lexicon"),
                StanzaError::NoScheme { failing, .. } => {
                    let bad: Vec<&str> = failing.iter().map(|&i| words[i]).collect();
                    format!("verses ending in {bad:?} fail to rhyme")
                }
            };
            out.push(Diagnostic {
                code: e.code(),
                message,
                line: first.line,
            });
        }
    }
    out
}
