#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
use syntaxeon_lex::lexer::{Tok, lex};
use syntaxeon_lex::literary::{cardinal_last_word, stanzas};
use syntaxeon_lex::prosody::check;
use syntaxeon_lexicon::lexicon::Lexicon;

fn lexicon() -> Lexicon {
    Lexicon::pinned().expect("pinned lexicon verifies")
}

fn codes(src: &str) -> Vec<&'static str> {
    let lexed = lex(src);
    let st = stanzas(&lexed);
    let mut lx = lexicon();
    let mut d: Vec<_> = lexed.diagnostics.iter().map(|d| d.code).collect();
    d.extend(check(&lexed, &st, &mut lx).iter().map(|d| d.code));
    d
}

#[test]
fn tokens_and_folding() {
    let l = lex("Let the Counter, a whole number, become 3.14 and 5.\n");
    let toks: Vec<_> = l.tokens.iter().map(|t| t.tok.clone()).collect();
    assert_eq!(toks[0], Tok::Word("let".into()));
    assert_eq!(toks[2], Tok::Word("counter".into()));
    assert!(matches!(toks[toks.len() - 4], Tok::Float { .. }));
    assert_eq!(toks[toks.len() - 2], Tok::Int(5));
    assert_eq!(toks[toks.len() - 1], Tok::Punct('.'));
    assert_eq!(l.diagnostics.len(), 0);
}

#[test]
fn lexical_errors() {
    assert_eq!(lex("a\tb").diagnostics[0].code, "SXP-101");
    assert_eq!(lex("say \"oops").diagnostics[0].code, "SXP-101");
    assert_eq!(lex("(never closed").diagnostics[0].code, "SXP-101");
    assert_eq!(lex("12abc").diagnostics[0].code, "SXP-102");
    assert_eq!(lex("99999999999999999999").diagnostics[0].code, "SXP-102");
    assert_eq!(lex("a # b").diagnostics[0].code, "SXP-101");
}

#[test]
fn string_escapes() {
    let l = lex(r#""a\"b\\c\n""#);
    assert_eq!(l.tokens[0].tok, Tok::Str("a\"b\\c\n".into()));
}

#[test]
fn asides_are_dropped_and_nest() {
    let l = lex("one (a (nested) note\nover lines) two\n");
    let words: Vec<_> = l.tokens.iter().map(|t| &t.tok).collect();
    assert_eq!(words, [&Tok::Word("one".into()), &Tok::Word("two".into())]);
    assert_eq!(l.diagnostics.len(), 0);
}

#[test]
fn blank_lines_split_stanzas_but_aside_lines_do_not() {
    let src = "men.\nten.\n(just a remark)\nthen.\n\n\nmen.\nten.\n";
    let st = stanzas(&lex(src));
    assert_eq!(st.len(), 2);
    assert_eq!(st[0].verses.len(), 3);
    // A blank line *inside* an aside is not a separator.
    let st = stanzas(&lex("men.\n(remark\n\nmore)\nten.\n"));
    assert_eq!(st.len(), 1);
}

#[test]
fn end_words() {
    let st = stanzas(&lex(
        "Speak \"Hello again\".\nProclaim ten.\nLet x be 10.\nLet y be 3.14.\n\"...\".\n",
    ));
    let ends: Vec<_> = st[0].verses.iter().map(|v| v.end_word.clone()).collect();
    assert_eq!(ends[0], Ok("again".to_string()));
    assert_eq!(ends[1], Ok("ten".to_string()));
    assert_eq!(ends[2], Ok("ten".to_string()));
    assert_eq!(ends[3], Ok("four".to_string()));
    assert!(ends[4].is_err());
}

#[test]
fn hyphenated_words_rhyme_on_their_last_component() {
    let ends = |src: &str| stanzas(&lex(src))[0].verses[0].end_word.clone();
    assert_eq!(ends("Speak twenty-one.\n"), Ok("one".to_string()));
    assert_eq!(ends("Let x be 75.\n"), Ok("five".to_string()));
    assert_eq!(
        ends("Speak \"a well-known man-of-war\".\n"),
        Ok("war".to_string())
    );
    // Runs of hyphens and apostrophes inside a word are kept apart from the split.
    assert_eq!(ends("Speak \"o'er\".\n"), Ok("o'er".to_string()));
}

#[test]
fn cardinals() {
    let cases = [
        (0, "zero"),
        (10, "ten"),
        (21, "twenty-one"),
        (30, "thirty"),
        (100, "hundred"),
        (1000, "thousand"),
        (1025, "twenty-five"),
        (1_200, "hundred"),
        (2_000_000, "million"),
        (3_000_000_000, "billion"),
        (3_000_000_000_000_000_000, "quintillion"),
    ];
    for (n, w) in cases {
        assert_eq!(cardinal_last_word(n), w, "{n}");
    }
}

#[test]
fn structural_verses_are_exempt() {
    let src = "Thus begins the tale of men:\nIf the sky is ten, then:\nOtherwise:\nSpeak \"amen\".\nSo be it.\n";
    let st = stanzas(&lex(src));
    let flags: Vec<_> = st[0].verses.iter().map(|v| v.structural).collect();
    assert_eq!(flags, [false, false, true, false, true]);
}

#[test]
fn valid_program_passes() {
    let src = "Thus begins the tale of men:\nSpeak \"Hello again\".\nProclaim the text of ten.\nSpeak \"Amen\".\n";
    assert_eq!(codes(src).len(), 0);
}

#[test]
fn bad_rhyme_is_202() {
    assert_eq!(
        codes("Thus begins the tale of men:\nSpeak \"Hello\".\n"),
        ["SXP-202"]
    );
}

#[test]
fn single_verse_is_201_and_unknown_word_301() {
    assert_eq!(codes("men.\n"), ["SXP-201"]);
    assert_eq!(codes("men.\nzyzzyva.\n"), ["SXP-301"]);
}

#[test]
fn pragma_makes_a_word_pronounceable() {
    let src = "(pronounced \"blorn\" as \"B L AO1 R N\")\nThus begins the tale of born:\nSpeak \"blorn\".\n";
    let lexed = lex(src);
    let st = stanzas(&lexed);
    let mut lx = lexicon();
    assert!(lx.lookup("blorn").is_none());
    assert_eq!(lexed.diagnostics.len(), 0);
    assert_eq!(check(&lexed, &st, &mut lx).len(), 0);
    // Same program without the pragma: blorn is unknown.
    assert_eq!(
        codes("Thus begins the tale of born:\nSpeak \"blorn\".\n"),
        ["SXP-301"]
    );
}

#[test]
fn pragma_errors() {
    assert_eq!(codes("(pronounced zen)\nmen.\nten.\n")[0], "SXP-302");
    assert_eq!(
        codes("(pronounced \"cat\" as \"K AE1 T\")\nmen.\nten.\n"),
        ["SXP-303"]
    );
    // An ordinary aside is a comment, not a pragma.
    assert_eq!(codes("(a comment)\nmen.\nten.\n").len(), 0);
}

#[test]
fn headers_rhyme_before_their_closer() {
    // (header, end word): a block header rhymes on its condition or count, a function header on
    // its title, a stanza header on its title; anything else on its last word.
    let cases = [
        ("If the pile is ten, then:", "ten"),
        ("As long as the pile is less than the hen, do:", "hen"),
        ("Do this three times:", "three"),
        ("Do this the sum of 99999 and 1 times:", "one"),
        (
            "To double, taking n as whole number, giving whole number:",
            "double",
        ),
        ("To the tale of two, taking n as whole number:", "two"),
        ("To \"big sum\", taking n as whole number:", "sum"),
        ("Thus begins the tale of men:", "men"),
        ("Speak \"then, and do\".", "do"),
        ("Give back the men.", "men"),
    ];
    for (src, want) in cases {
        let l = lex(&format!("{src}\n"));
        let verses = stanzas(&l);
        assert_eq!(verses[0].verses[0].end_word.as_deref(), Ok(want), "{src}");
    }
}
