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
        // Feminine (multi-syllable) rhyme: the tail runs from the stressed vowel to the end.
        ("ending", "bending", true),
        ("tender", "fender", true),
        ("flower", "power", true),
        ("ending", "tender", false),
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
    // Deterministic order: most common first, then shortest, then alphabetical.
    assert!(
        then.windows(2)
            .all(|w| (l.tier(w[0]), w[0].len(), w[0]) <= (l.tier(w[1]), w[1].len(), w[1]))
    );
    assert!(
        l.tier("men") == 10
            && l.tier("wren") == 20
            && l.tier("cnn") == 99
            && l.tier("chimes") == 35
    );
    let common: Vec<&str> = then.iter().copied().filter(|w| l.is_common(w)).collect();
    assert!(
        common.iter().position(|w| *w == "men") < common.iter().position(|w| *w == "wren"),
        "a tier-10 word comes before a tier-20 one"
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

#[test]
fn word_tier_pin_detects_tampering() {
    use syntaxeon_lexicon::lexicon::verify_tiers_pin;
    let lock = include_str!("../lexicon/lexicon.lock");
    let list = include_str!("../lexicon/word-tiers.txt");
    assert_eq!(verify_tiers_pin(list, lock), Ok(()));
    assert_eq!(
        verify_tiers_pin(&format!("{list}extra 10\n"), lock)
            .unwrap_err()
            .code(),
        "SXP-304"
    );
}

#[test]
fn assonance_vectors() {
    use syntaxeon_lexicon::lexicon::RhymeRules;
    use syntaxeon_lexicon::rhyme::near_rhymes_with;
    let strict = {
        let mut l = lex();
        l.set_rhyme_rules(RhymeRules::perfect());
        l
    };
    let loose = lex();
    assert!(loose.rhyme_rules().assonance, "assonance is on by default");
    // (a, b, perfect, assonant): same vowels from the stressed vowel on, consonants free.
    let cases = [
        ("men", "bed", false, true),
        ("cat", "cap", false, true),
        ("tender", "member", false, true),
        ("tonight", "light", true, true),
        ("ten", "tonight", false, false),
        ("night", "knight", false, false),
        ("men", "amen", false, false),
        // Same stressed vowel but a different number of syllables after it.
        ("cat", "capital", false, false),
        ("cat", "cat", false, false),
    ];
    for (a, b, perfect, assonant) in cases {
        assert_eq!(words_rhyme(&strict, a, b), Ok(perfect), "perfect {a}/{b}");
        assert_eq!(words_rhyme(&loose, a, b), Ok(assonant), "assonant {a}/{b}");
    }
    let near = near_rhymes_with(&loose, "men").unwrap();
    assert!(
        near.contains(&"bed") && !near.contains(&"ten"),
        "perfect rhymes are not near"
    );
}

/// Every pair in a group must rhyme (ADR-016), so the verdict does not depend on which verse
/// leads. `men` and `amen` are an identical rhyme, so no group may hold both (it used to pass
/// when `ten` led).
#[test]
fn a_group_is_valid_only_if_every_pair_rhymes() {
    use syntaxeon_lexicon::lexicon::RhymeRules;
    use syntaxeon_lexicon::scheme::StanzaError;
    let mut l = lex();
    l.set_rhyme_rules(RhymeRules::perfect());
    assert_eq!(words_rhyme(&l, "men", "amen"), Ok(false));
    for order in [
        ["ten", "men", "amen"],
        ["men", "ten", "amen"],
        ["amen", "men", "ten"],
        ["men", "amen", "ten"],
    ] {
        assert!(check_stanza(&l, &order).is_err(), "{order:?}");
    }
    // The verse reported is the later one of the failing pair.
    assert_eq!(
        check_stanza(&l, &["ten", "men", "amen"]),
        Err(StanzaError::NoScheme {
            scheme: vec![0, 0, 0],
            failing: vec![2]
        })
    );
    // A triple whose pairs all rhyme is fine in any order.
    assert_eq!(check_stanza(&l, &["men", "ten", "pen"]), Ok(()));
    assert_eq!(check_stanza(&l, &["pen", "men", "ten"]), Ok(()));
}

#[test]
fn rhyme_kind_names_the_relation_whatever_the_rules() {
    use syntaxeon_lexicon::lexicon::RhymeRules;
    use syntaxeon_lexicon::rhyme::{RhymeKind::*, rhyme_kind};
    let cases = [
        ("tonight", "light", Perfect),
        ("fire", "higher", Perfect),
        ("men", "bed", Assonant),
        ("cat", "cap", Assonant),
        ("night", "knight", Identical),
        ("men", "amen", Identical),
        ("cat", "cat", Identical),
        ("ten", "tonight", None),
        ("cat", "capital", None),
    ];
    for strict in [false, true] {
        let mut l = lex();
        l.set_rhyme_rules(RhymeRules { assonance: !strict });
        for (a, b, want) in cases {
            assert_eq!(rhyme_kind(&l, a, b), Ok(want), "{a}/{b} (strict {strict})");
            assert_eq!(rhyme_kind(&l, b, a), Ok(want), "symmetry {a}/{b}");
        }
    }
    assert_eq!(Perfect.name(), "perfect");
    assert!(rhyme_kind(&lex(), "zyzzyva", "men").is_err());
}
