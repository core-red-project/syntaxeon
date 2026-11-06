#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
use syntaxeon_check::check;
use syntaxeon_lex::lexer::lex;
use syntaxeon_lex::literary::stanzas;
use syntaxeon_syntax::parse_program;

/// Codes from the checker for `src`; asserts lexing and parsing were clean.
fn codes(src: &str) -> Vec<&'static str> {
    let l = lex(src);
    let st = stanzas(&l);
    let (p, d) = parse_program(&l, &st);
    assert!(
        l.diagnostics.is_empty() && d.is_empty(),
        "front-end errors: {d:?} {:?}",
        l.diagnostics
    );
    check(&p).iter().map(|d| d.code).collect()
}

/// Same, for a body placed in the entry stanza.
fn entry(lines: &str) -> Vec<&'static str> {
    codes(&format!("Thus begins the tale:\n{lines}\n"))
}

#[test]
fn the_worked_example_type_checks() {
    let src = "Thus begins the counting:\n\
        Let the counter, a whole number, become zero.\n\
        Do this five times:\n\
        Let the counter now be the sum of the counter and one.\n\
        Proclaim the text of the counter.\n\
        So be it.\n\n\
        To double, taking the number as whole number, giving whole number:\n\
        Give back the product of the number and two.\n";
    assert_eq!(codes(src).len(), 0);
}

#[test]
fn valid_constructs() {
    let ok = [
        "Let s be \"a\" plus \"b\".",
        "Let c be item 0 of \"abc\".",
        "Let xs be a list of 1, 2, and 3.\nLet n be the length of the xs.",
        "Let xs be a list of 1, 2.\nLet ys be the xs with 3 appended.",
        "Let xs, a list of whole number, be an empty list of whole number.",
        "Let b be 1 is less than 2.",
        "Let t be \"a\" is \"b\".",
        "Let f be the whole number of \"12\".",
        "Let g be the fraction of 3.",
        "Let x, a whole number, become 1.\nLet x now be 2.",
        "Let xs, a list of whole number, become a list of 1.\nLet item 0 of the xs now be 9.",
        "Let line be what is heard.\nSpeak the line.",
    ];
    for src in ok {
        assert_eq!(entry(src), Vec::<&str>::new(), "{src}");
    }
}

#[test]
fn unknown_names_and_calls() {
    assert_eq!(entry("Speak the missing."), ["SXP-411"]);
    assert_eq!(entry("Let x now be 1."), ["SXP-411"]);
    assert_eq!(entry("Call nowhere."), ["SXP-412"]);
}

#[test]
fn an_error_reports_once_not_in_a_cascade() {
    assert_eq!(
        entry("Let x be the missing.\nLet y be the x plus 1.\nSpeak the text of the y."),
        ["SXP-411"]
    );
}

#[test]
fn declared_type_must_match() {
    assert_eq!(entry("Let x, a text, be 1."), ["SXP-402"]);
    assert_eq!(entry("Let x, a fraction, be 1."), ["SXP-406"]);
    assert_eq!(entry("Let x, a list of text, be a list of 1."), ["SXP-402"]);
}

#[test]
fn mutability() {
    assert_eq!(entry("Let x be 1.\nLet x now be 2."), ["SXP-403"]);
    assert_eq!(
        entry("Let xs be a list of 1.\nLet item 0 of the xs now be 2."),
        ["SXP-403"]
    );
}

#[test]
fn assignment_types() {
    assert_eq!(
        entry("Let x, a whole number, become 1.\nLet x now be \"s\"."),
        ["SXP-402"]
    );
    assert_eq!(
        entry("Let s, a text, become \"s\".\nLet item 0 of the s now be the letter \"a\"."),
        ["SXP-402"]
    );
}

#[test]
fn no_implicit_numeric_conversion() {
    assert_eq!(entry("Let x be 1 plus 2.5."), ["SXP-406"]);
    assert_eq!(entry("Let x be 1 is less than 2.5."), ["SXP-406"]);
    assert_eq!(entry("Let x be 1.5 modulo 2."), ["SXP-406"]);
}

#[test]
fn operators_reject_wrong_types() {
    assert_eq!(entry("Let x be true plus false."), ["SXP-402"]);
    assert_eq!(entry("Let x be \"a\" minus \"b\"."), ["SXP-402"]);
    assert_eq!(entry("Let x be both 1 and 2."), ["SXP-402"]);
    assert_eq!(entry("Let x be not 1."), ["SXP-402"]);
    assert_eq!(entry("Let x be the negation of \"a\"."), ["SXP-402"]);
    assert_eq!(entry("Let x be 1 is \"a\"."), ["SXP-402"]);
}

#[test]
fn duplicate_declaration_in_one_block() {
    assert_eq!(entry("Let x be 1.\nLet x be 2."), ["SXP-401"]);
    // Shadowing an outer block's name is allowed.
    assert_eq!(
        entry("Let x be 1.\nDo this 2 times:\nLet x be \"inner\".\nSo be it.").len(),
        0
    );
    let dup = "Thus begins t:\nReturn nothing.\n\nTo f, taking x as whole number, x as text:\nReturn nothing.\n";
    assert_eq!(codes(dup), ["SXP-401"]);
}

#[test]
fn conditions_and_counts() {
    assert_eq!(entry("If 1, then:\nSpeak \"x\".\nSo be it."), ["SXP-402"]);
    assert_eq!(
        entry("As long as \"a\", do:\nSpeak \"x\".\nSo be it."),
        ["SXP-402"]
    );
    assert_eq!(
        entry("Do this 1.5 times:\nSpeak \"x\".\nSo be it."),
        ["SXP-406"]
    );
}

#[test]
fn builtin_arguments() {
    assert_eq!(entry("Speak 5."), ["SXP-402"]);
    assert_eq!(entry("Let n be the length of 5."), ["SXP-402"]);
    assert_eq!(entry("Let n be the text of \"s\"."), ["SXP-402"]);
    assert_eq!(
        entry("Let xs be a list of 1.\nLet ys be the xs with \"a\" appended."),
        ["SXP-402"]
    );
    assert_eq!(
        entry("Let n be 3.\nLet ys be the n with 4 appended."),
        ["SXP-402"]
    );
}

#[test]
fn list_elements_agree() {
    assert_eq!(entry("Let xs be a list of 1, \"a\"."), ["SXP-402"]);
    assert_eq!(entry("Let x be item \"a\" of \"abc\"."), ["SXP-402"]);
    assert_eq!(entry("Let x be item 0 of 5."), ["SXP-402"]);
}

#[test]
fn calls_check_arity_and_argument_types() {
    let head = "Thus begins t:\n";
    let f = "\n\nTo twice, taking n as whole number, giving whole number:\nGive back the product of the n and 2.\n";
    assert_eq!(
        codes(&format!("{head}Let x be calling twice with 3.{f}")).len(),
        0
    );
    assert_eq!(
        codes(&format!("{head}Let x be calling twice with 3, 4.{f}")),
        ["SXP-413"]
    );
    assert_eq!(
        codes(&format!("{head}Let x be calling twice.{f}")),
        ["SXP-413"]
    );
    assert_eq!(
        codes(&format!("{head}Let x be calling twice with \"s\".{f}")),
        ["SXP-402"]
    );
    assert_eq!(
        codes(&format!("{head}Let x, a text, be calling twice with 3.{f}")),
        ["SXP-402"]
    );
    // A value-less call cannot be bound.
    let g = "\n\nTo quiet:\nReturn nothing.\n";
    assert_eq!(
        codes(&format!("{head}Let x be calling quiet.{g}")),
        ["SXP-402"]
    );
    assert_eq!(codes(&format!("{head}Call quiet.{g}")).len(), 0);
}

#[test]
fn recursion_and_forward_references() {
    let src = "Thus begins t:\nLet r be calling fact with 5.\n\n\
        To fact, taking n as whole number, giving whole number:\n\
        If the n is 0, then:\nGive back 1.\nOtherwise:\n\
        Give back the product of the n and calling fact with the difference of the n and 1.\nSo be it.\n";
    assert_eq!(codes(src).len(), 0, "{:?}", codes(src));
}

#[test]
fn returns() {
    let head = "Thus begins t:\nReturn nothing.\n\n";
    // Missing on one path.
    assert_eq!(
        codes(&format!(
            "{head}To f, taking n as whole number, giving whole number:\nIf the n is 0, then:\nGive back 1.\nSo be it.\n"
        )),
        ["SXP-404"]
    );
    // No return at all.
    assert_eq!(
        codes(&format!("{head}To f, giving whole number:\nLet x be 1.\n")),
        ["SXP-404"]
    );
    // A loop may run zero times.
    assert_eq!(
        codes(&format!(
            "{head}To f, giving whole number:\nDo this 3 times:\nGive back 1.\nSo be it.\n"
        )),
        ["SXP-404"]
    );
    // Wrong type, missing value, unexpected value.
    assert_eq!(
        codes(&format!(
            "{head}To f, giving whole number:\nGive back \"s\".\n"
        )),
        ["SXP-402"]
    );
    assert_eq!(
        codes(&format!(
            "{head}To f, giving whole number:\nReturn nothing.\n"
        )),
        ["SXP-402"]
    );
    assert_eq!(codes(&format!("{head}To f:\nGive back 1.\n")), ["SXP-402"]);
}

#[test]
fn entry_cannot_return_a_value() {
    assert_eq!(entry("Give back 1."), ["SXP-405"]);
    assert_eq!(entry("Return nothing.").len(), 0);
}

#[test]
fn program_level_errors() {
    let head = "Thus begins t:\nReturn nothing.\n\n";
    assert_eq!(
        codes(&format!(
            "{head}To f:\nReturn nothing.\n\nTo f:\nReturn nothing.\n"
        )),
        ["SXP-409"]
    );
    assert_eq!(
        codes(&format!("{head}To \"speak\":\nReturn nothing.\n")),
        ["SXP-407"]
    );
    assert_eq!(
        check(&syntaxeon_core::Program { stanzas: vec![] })[0].code,
        "SXP-408"
    );
}

#[test]
fn the_primitives_have_the_documented_types() {
    assert_eq!(
        entry("Let the n, a whole number, become the code of the letter \"a\"."),
        Vec::<&str>::new()
    );
    assert_eq!(
        entry("Let the c, a letter, become the letter with code 97."),
        Vec::<&str>::new()
    );
    assert_eq!(
        entry("Let the b, a truth, become there is more to hear."),
        Vec::<&str>::new()
    );
    assert_eq!(
        entry("Let the b, a truth, become \"12\" reads as a whole number."),
        Vec::<&str>::new()
    );
    // Wrong operand types are ordinary type errors.
    assert_eq!(
        entry("Let the n, a whole number, become the code of 5."),
        ["SXP-402"]
    );
    assert_eq!(
        entry("Let the c, a letter, become the letter with code \"a\"."),
        ["SXP-402"]
    );
    assert_eq!(
        entry("Let the b, a truth, become 5 reads as a whole number."),
        ["SXP-402"]
    );
    // They return the types they promise, not others.
    assert_eq!(
        entry("Let the n, a text, become the code of the letter \"a\"."),
        ["SXP-402"]
    );
}
