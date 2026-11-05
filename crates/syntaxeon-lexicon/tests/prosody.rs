#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
use syntaxeon_lexicon::lexicon::{Lexicon, PragmaError};
use syntaxeon_lexicon::rhyme::words_rhyme;
use syntaxeon_lexicon::scheme::{StanzaError, check_stanza, schemes};

fn lex() -> Lexicon {
    Lexicon::pinned().expect("pinned lexicon verifies")
}

#[test]
fn word_vectors() {
    let l = lex();
    let cases = [
        ("tonight", "light", true),
        ("ten", "then", true),
        ("love", "dove", true),
        ("eye", "high", true),
        ("night", "knight", false),
        ("eye", "I", false),
        ("cat", "cat", false),
        ("ten", "tonight", false),
        ("men", "amen", false),
        ("again", "men", true),
        ("fire", "higher", true),
    ];
    for (a, b, want) in cases {
        assert_eq!(words_rhyme(&l, a, b), Ok(want), "{a}/{b}");
    }
}

#[test]
fn unknown_word_is_unpronounceable() {
    assert!(words_rhyme(&lex(), "zyzzyva", "men").is_err());
}

#[test]
fn stanza_vectors() {
    let l = lex();
    assert_eq!(check_stanza(&l, &["men", "again", "ten", "amen"]), Ok(()));
    let err = check_stanza(&l, &["men", "hello"]).unwrap_err();
    assert_eq!(err.code(), "SXP-202");
    assert_eq!(
        check_stanza(&l, &["men"]),
        Err(StanzaError::TooShort { count: 1 })
    );
    assert_eq!(
        check_stanza(&l, &["men", "zyzzyva"]),
        Err(StanzaError::Unpronounceable("zyzzyva".into()))
    );
}

#[test]
fn scheme_shapes() {
    assert_eq!(schemes(1).len(), 0);
    assert_eq!(schemes(3), vec![vec![0, 0, 0]]);
    assert_eq!(schemes(5), vec![vec![0, 0, 1, 1, 1]]);
    assert_eq!(schemes(4).len(), 3);
}

#[test]
fn abab_and_abba() {
    let l = lex();
    assert_eq!(check_stanza(&l, &["ten", "light", "men", "night"]), Ok(()));
    assert_eq!(check_stanza(&l, &["ten", "light", "night", "men"]), Ok(()));
}

#[test]
fn pragma_adds_but_never_overrides() {
    let mut l = lex();
    assert_eq!(l.add_pragma("zyzzyva", "Z IH1 Z AH0 V AH0"), Ok(()));
    assert!(l.lookup("zyzzyva").is_some());
    assert_eq!(l.add_pragma("cat", "K AE1 T"), Err(PragmaError::Redefines));
    assert_eq!(l.add_pragma("foo", "AY"), Err(PragmaError::Malformed));
    assert_eq!(l.add_pragma("foo", ""), Err(PragmaError::Malformed));
}

#[test]
fn hyphen_falls_back_to_last_component() {
    let l = lex();
    assert!(l.lookup("moon-light").is_some());
    assert!(l.lookup("twenty-one").is_some());
}

#[test]
fn pin_detects_tampering() {
    use syntaxeon_lexicon::lexicon::verify_pin;
    let lock = include_str!("../lexicon/lexicon.lock");
    let dict = include_str!("../lexicon/cmudict.dict");
    assert_eq!(verify_pin(dict, lock), Ok(()));
    let tampered = verify_pin(&dict.replace("zen Z EH1 N", "zen Z EH1 M"), lock).unwrap_err();
    assert_eq!(tampered.code(), "SXP-304");
    assert!(verify_pin(dict, "no hash here").is_err());
}

#[test]
fn dictionary_comments_are_ignored() {
    let l = Lexicon::parse("aalto AA1 L T OW2 # name, finnish\n").unwrap();
    assert!(l.lookup("aalto").is_some());
}

#[test]
fn rhymes_with_lists_partners_and_excludes_the_word_itself() {
    use syntaxeon_lexicon::rhyme::rhymes_with;
    let l = lex();
    let then = rhymes_with(&l, "then").unwrap();
    assert!(then.contains(&"men") && then.contains(&"ten") && then.contains(&"again"));
    assert!(!then.contains(&"then"), "a word is not its own rhyme");
    // Same onset means identical rhyme, so homophones of the onset are excluded.
    let night = rhymes_with(&l, "night").unwrap();
    assert!(night.contains(&"light") && !night.contains(&"knight"));
    // Deterministic order: shortest first, then alphabetical.
    assert!(
        then.windows(2)
            .all(|w| (w[0].len(), w[0]) <= (w[1].len(), w[1]))
    );
    assert!(rhymes_with(&l, "zyzzyva").is_err());
    // The common-word list drops abbreviations, bare names and tiny tokens.
    assert!(!l.is_common("cnn") && !l.is_common("ren") && !l.is_common("gen"));
    assert!(l.is_common("men") && l.is_common("again") && l.is_common("chimes"));
    assert!(
        then.iter().any(|w| !l.is_common(w)),
        "the raw list has noise to filter"
    );
}

#[test]
fn common_word_pin_detects_tampering() {
    use syntaxeon_lexicon::lexicon::verify_common_pin;
    let lock = include_str!("../lexicon/lexicon.lock");
    let list = include_str!("../lexicon/common-words.txt");
    assert_eq!(verify_common_pin(list, lock), Ok(()));
    assert_eq!(
        verify_common_pin(&format!("{list}extra\n"), lock)
            .unwrap_err()
            .code(),
        "SXP-304"
    );
}
