//! Runs the compiler phases in the order fixed by ADR-009 and stops at the first failing one.

use syntaxeon_check::check;
use syntaxeon_core::Program;
use syntaxeon_lex::Diagnostic;
use syntaxeon_lex::lexer::{Lexed, lex};
use syntaxeon_lex::literary::{Stanza as Verses, stanzas};
use syntaxeon_lex::prosody;
use syntaxeon_lexicon::lexicon::{Lexicon, RhymeRules};
use syntaxeon_syntax::parse_program;

pub struct Analysis {
    pub lexed: Lexed,
    pub verses: Vec<Verses>,
    pub program: Program,
    /// The pronunciation lexicon with the file's pragmas applied (set once prosody ran).
    pub lexicon: Option<Lexicon>,
    pub diagnostics: Vec<Diagnostic>,
    /// Phase that produced `diagnostics`, if any.
    pub failed_phase: Option<&'static str>,
}

#[must_use]
pub fn analyze(src: &str, rules: RhymeRules, phase: &mut dyn FnMut(&str)) -> Analysis {
    phase("lexical and grammatical analysis");
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    let (program, parse_diags) = parse_program(&lexed, &verses);
    let mut a = Analysis {
        diagnostics: lexed
            .diagnostics
            .iter()
            .cloned()
            .chain(parse_diags)
            .collect(),
        lexed,
        verses,
        program,
        lexicon: None,
        failed_phase: None,
    };
    if !a.diagnostics.is_empty() {
        a.failed_phase = Some("syntax");
        return finish(a);
    }
    phase("semantic analysis");
    a.diagnostics = check(&a.program);
    if !a.diagnostics.is_empty() {
        a.failed_phase = Some("semantics");
        return finish(a);
    }
    phase("prosodic validation");
    match Lexicon::pinned() {
        Ok(mut lex) => {
            lex.set_rhyme_rules(rules);
            a.diagnostics = prosody::check(&a.lexed, &a.verses, &mut lex);
            a.lexicon = Some(lex);
        }
        Err(e) => {
            a.diagnostics = vec![Diagnostic {
                code: e.code(),
                message: e.to_string(),
                line: 0,
                span: None,
            }];
        }
    }
    if !a.diagnostics.is_empty() {
        a.failed_phase = Some("prosody");
    }
    finish(a)
}

fn finish(mut a: Analysis) -> Analysis {
    a.diagnostics.sort_by_key(|d| d.line);
    a
}

#[must_use]
pub fn category(code: &str) -> &'static str {
    match code.strip_prefix("SXP-").and_then(|n| n.chars().next()) {
        Some('1') if code == "SXP-101" || code == "SXP-102" => "lexical",
        Some('1') => "grammatical",
        Some('2') => "prosodic",
        Some('3') => "lexicon",
        Some('4') => "semantic",
        Some('5') => "runtime",
        _ => "internal",
    }
}

/// The literary headline of a code; the structured record stays authoritative and the
/// technical message is printed under it. Every code the compiler emits has one (a test scans
/// the sources for codes and fails when one is missing).
#[must_use]
pub fn headline(code: &str) -> Option<&'static str> {
    Some(match code {
        "SXP-101" => "A stray mark lies in the text.",
        "SXP-102" => "A number is ill-formed or beyond reckoning.",
        "SXP-103" => "No rule of the grammar can read this sentence.",
        "SXP-104" => "This sentence can be read in more than one way.",
        "SXP-106" => "An infix word stands where none may.",
        "SXP-107" => "A block is opened or closed amiss.",
        "SXP-108" => "An empty list is not told what it holds.",
        "SXP-109" => "The header of this stanza is malformed.",
        "SXP-201" => "A lone verse has nothing to rhyme with.",
        "SXP-202" => "The verse hath failed to rhyme.",
        "SXP-203" => "No word could be found to end the verse.",
        "SXP-301" => "This word is a stranger to the lexicon.",
        "SXP-302" => "The pronunciation given is not well formed.",
        "SXP-303" => "The lexicon already knows this word and will not be corrected.",
        "SXP-304" => "The lexicon has been altered from its pinned form.",
        "SXP-305" => "The rhythm file cannot be read.",
        "SXP-401" => "A name is declared twice in the same breath.",
        "SXP-402" => "The types do not agree.",
        "SXP-403" => "This name was not made to change.",
        "SXP-404" => "A function ends without giving back what it promised.",
        "SXP-405" => "The opening stanza must not give back a value.",
        "SXP-406" => "A whole number and a fraction may not be mixed unconverted.",
        "SXP-407" => "This name already belongs to a builtin.",
        "SXP-408" => "The program has no stanza, hence no beginning.",
        "SXP-409" => "Two stanzas share one name.",
        "SXP-410" => "An empty list does not say what it holds.",
        "SXP-411" => "This name is unknown here.",
        "SXP-412" => "No such stanza or function can be called.",
        "SXP-413" => "A call is made with the wrong number of arguments.",
        "SXP-501" => "A number has outgrown its bounds.",
        "SXP-502" => "There is no dividing by zero.",
        "SXP-503" => "The index lies outside the list or the text.",
        "SXP-504" => "The calls have nested too deep.",
        "SXP-505" => "There is nothing more to hear.",
        "SXP-506" => "This value cannot be converted.",
        "SXP-507" => "The output cannot be written.",
        "SXP-599" => "An unchecked program reached the evaluator.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{category, headline};

    /// Every `SXP-nnn` the sources mention has a headline, and every headline is a code the
    /// sources mention, so the table cannot drift from the compiler.
    #[test]
    fn every_emitted_code_has_a_headline() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = std::collections::BTreeSet::new();
        for krate in std::fs::read_dir(&crates).expect("crates directory") {
            let src = krate.expect("entry").path().join("src");
            let Ok(files) = std::fs::read_dir(&src) else {
                continue;
            };
            for file in files
                .flatten()
                .filter(|f| f.path().extension().is_some_and(|e| e == "rs"))
            {
                // The headline table itself mentions every code; skip it.
                if file.path().ends_with("pipeline.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(file.path()).expect("source");
                let mut rest = text.as_str();
                while let Some(i) = rest.find("SXP-") {
                    let Some(code) = rest.get(i..i + 7) else {
                        break;
                    };
                    if code[4..].chars().all(|c| c.is_ascii_digit()) {
                        found.insert(code.to_string());
                    }
                    rest = &rest[i + 4..];
                }
            }
        }
        assert!(found.len() > 30, "scan found only {found:?}");
        let missing: Vec<&String> = found.iter().filter(|c| headline(c).is_none()).collect();
        assert!(missing.is_empty(), "codes without a headline: {missing:?}");
        // The table lists nothing the compiler never says.
        for code in (100..600).map(|n| format!("SXP-{n}")) {
            if headline(&code).is_some() {
                assert!(
                    found.contains(&code),
                    "{code} has a headline but is never emitted"
                );
            }
        }
        // And each code lands in a category.
        for code in &found {
            assert_ne!(category(code), "internal", "{code}");
        }
    }
}
