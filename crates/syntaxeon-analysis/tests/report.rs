#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
use syntaxeon_analysis::report::{StanzaInfo, lexicon_for, verse_report};
use syntaxeon_lex::lexer::lex;
use syntaxeon_lex::literary::stanzas;
use syntaxeon_lexicon::lexicon::RhymeRules;
use syntaxeon_lexicon::rhyme::RhymeKind;

fn report(src: &str, rules: RhymeRules) -> Vec<StanzaInfo> {
    let lexed = lex(src);
    let verses = stanzas(&lexed);
    let lex = lexicon_for(&lexed, rules).unwrap();
    verse_report(&verses, &lex)
}

const GOOD: &str = "Thus begins the tale of men:\nSpeak \"Hello again\".\nProclaim the text of ten.\nSpeak \"Amen\".\n";

#[test]
fn a_valid_stanza_reports_its_scheme_sounds_and_relations() {
    let r = report(GOOD, RhymeRules::default());
    assert_eq!(r.len(), 1);
    let s = &r[0];
    assert!(s.valid);
    assert_eq!(s.scheme.as_deref(), Some("AABB"));
    let v = &s.verses;
    assert_eq!(v.len(), 4);
    assert_eq!(v[0].end_word.as_deref(), Some("men"));
    assert_eq!(v[0].prons[0], "M EH1 N");
    assert_eq!(v[0].tails[0], "EH N");
    assert_eq!(v[0].group, Some('A'));
    assert_eq!(v[0].leader, None);
    assert_eq!(v[1].group, Some('A'));
    assert_eq!(v[1].leader, Some(0));
    assert_eq!(v[1].kind, Some(RhymeKind::Perfect));
    assert_eq!(v[1].rhymes, Some(true));
    // `again` has two pronunciations; both are listed, the first is the one to recite.
    assert_eq!(v[1].prons.len(), 2);
    assert_eq!(v[3].group, Some('B'));
    assert_eq!(v[3].leader, Some(2));
    assert_eq!(v[3].kind, Some(RhymeKind::Perfect));
}

#[test]
fn structural_verses_have_no_sound_and_no_group() {
    let src = "Thus begins the tale of ten:\nIf true, then:\nSpeak \"men\".\nSo be it.\n";
    let s = &report(src, RhymeRules::default())[0];
    assert!(s.verses[3].structural && s.verses[3].group.is_none() && s.verses[3].prons.is_empty());
    // Three counting verses make one group of three.
    assert_eq!(s.scheme.as_deref(), Some("AAA"));
}

#[test]
fn assonance_is_reported_as_such_and_follows_the_rules() {
    let src = "Thus begins the tale of men:\nSpeak \"Go to bed\".\n";
    let loose = &report(src, RhymeRules::default())[0];
    assert!(loose.valid);
    assert_eq!(loose.verses[1].kind, Some(RhymeKind::Assonant));
    assert_eq!(loose.verses[1].rhymes, Some(true));
    let strict = &report(src, RhymeRules::perfect())[0];
    assert!(!strict.valid);
    // The relation is the same; whether it is accepted is the rules' business.
    assert_eq!(strict.verses[1].kind, Some(RhymeKind::Assonant));
    assert_eq!(strict.verses[1].rhymes, Some(false));
    // The scheme is still shown, so the editor can say which verses disagree.
    assert_eq!(strict.scheme.as_deref(), Some("AA"));
}

#[test]
fn a_word_the_lexicon_does_not_know_has_no_sound() {
    let s = &report(
        "Thus begins the tale of men:\nSpeak \"zyzzyva\".\n",
        RhymeRules::default(),
    )[0];
    assert!(s.verses[1].prons.is_empty() && s.verses[1].tails.is_empty());
    assert!(!s.valid && s.scheme.is_none());
    // A pragma gives it one.
    let src = "Thus begins the tale of men:\nSpeak \"zyzzyva\".\n(pronounced \"zyzzyva\" as \"Z IH1 Z AH0 V AH0\")\n";
    let s = &report(src, RhymeRules::default())[0];
    assert_eq!(s.verses[1].prons, ["Z IH1 Z AH0 V AH0"]);
}
