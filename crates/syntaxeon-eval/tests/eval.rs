#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
#![allow(clippy::many_single_char_names)] // short names for lexed/parsed stages in a test helper

use syntaxeon_check::check;
use syntaxeon_eval::{MAX_FRAMES, RuntimeError, run, run_vm};
use syntaxeon_lex::lexer::lex;
use syntaxeon_lex::literary::stanzas;
use syntaxeon_syntax::parse_program;

/// Parses, type-checks and runs `src` on a big-stack thread with both engines, which must
/// agree; returns the evaluator's (result, stdout).
fn exec(src: &str, input: &str) -> (Result<(), RuntimeError>, String) {
    let (src, input) = (src.to_string(), input.to_string());
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let l = lex(&src);
            let st = stanzas(&l);
            let (p, d) = parse_program(&l, &st);
            assert!(l.diagnostics.is_empty() && d.is_empty(), "front-end: {d:?}");
            let c = check(&p);
            assert_eq!(c.len(), 0, "checker: {c:?}");
            let mut out = Vec::new();
            let r = run(&p, &mut input.as_bytes(), &mut out);
            // The VM must be indistinguishable from the tree evaluator: same output, same
            // error with the same line and span (ADR-021).
            let mut vm_out = Vec::new();
            let vm = run_vm(&p, &mut input.as_bytes(), &mut vm_out);
            assert_eq!(vm, r, "VM and evaluator disagree on the result of:\n{src}");
            assert_eq!(
                vm_out, out,
                "VM and evaluator disagree on the output of:\n{src}"
            );
            (r, String::from_utf8(out).unwrap())
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Runs `lines` as the entry body and returns stdout, asserting success.
fn say(lines: &str) -> String {
    let (r, out) = exec(&format!("Thus begins the tale:\n{lines}\n"), "");
    assert_eq!(r, Ok(()), "{lines}");
    out
}

fn fail(lines: &str) -> &'static str {
    exec(&format!("Thus begins the tale:\n{lines}\n"), "")
        .0
        .unwrap_err()
        .code
}

#[test]
fn counting_loop() {
    let src = "Thus begins the counting:\n\
        Let the counter, a whole number, become zero.\n\
        Do this five times:\n\
        Let the counter now be the sum of the counter and one.\n\
        Proclaim the text of the counter.\n\
        So be it.\n";
    assert_eq!(exec(src, "").1, "1\n2\n3\n4\n5\n");
}

#[test]
fn output_has_no_implicit_newline() {
    assert_eq!(say("Speak \"a\".\nSpeak \"b\".\nProclaim \"c\"."), "abc\n");
}

#[test]
fn integer_arithmetic_and_edge_cases() {
    assert_eq!(say("Proclaim the text of the quotient of 7 and 2."), "3\n");
    assert_eq!(
        say("Proclaim the text of the quotient of the negation of 7 and 2."),
        "-3\n"
    );
    assert_eq!(
        say("Proclaim the text of the remainder of the negation of 7 and 2."),
        "-1\n"
    );
    assert_eq!(
        say("Proclaim the text of the remainder of 7 and the negation of 2."),
        "1\n"
    );
    assert_eq!(say("Proclaim the text of the product of 6 and 7."), "42\n");
    assert_eq!(
        say("Proclaim the text of the difference of 3 and 5."),
        "-2\n"
    );
}

#[test]
fn integer_overflow_and_division_by_zero_are_errors() {
    assert_eq!(fail("Let x be 9223372036854775807 plus 1."), "SXP-501");
    assert_eq!(
        fail("Let x be the product of 9223372036854775807 and 2."),
        "SXP-501"
    );
    assert_eq!(
        fail(
            "Let x be the negation of 9223372036854775807.\nLet y be the difference of the x and 2."
        ),
        "SXP-501"
    );
    assert_eq!(fail("Let x be the quotient of 1 and 0."), "SXP-502");
    assert_eq!(fail("Let x be the remainder of 1 and 0."), "SXP-502");
    // The most negative integer divided by -1 overflows; its remainder is 0.
    let min = "Let m be the difference of the negation of 9223372036854775807 and 1.\nLet n be the negation of 1.";
    assert_eq!(
        fail(&format!("{min}\nLet q be the quotient of the m and the n.")),
        "SXP-501"
    );
    assert_eq!(
        say(&format!(
            "{min}\nProclaim the text of the remainder of the m and the n."
        )),
        "0\n"
    );
}

#[test]
fn floats_follow_ieee() {
    assert_eq!(
        say("Proclaim the text of the quotient of 1.0 and 0.0."),
        "inf\n"
    );
    assert_eq!(say("Proclaim the text of the sum of 1.5 and 1.0."), "2.5\n");
    assert_eq!(say("Proclaim the text of 1.0."), "1.0\n");
    assert_eq!(
        say(
            "Let z be the quotient of 0.0 and 0.0.\nLet b be the z is the z.\nProclaim the text of the b."
        ),
        "false\n"
    );
    assert_eq!(say("Proclaim the text of the fraction of 3."), "3.0\n");
}

#[test]
fn conversions() {
    assert_eq!(
        say("Proclaim the text of the whole number of \"12\"."),
        "12\n"
    );
    assert_eq!(
        say("Proclaim the text of the whole number of the negation of 2.7."),
        "-2\n"
    );
    assert_eq!(fail("Let n be the whole number of \"x\"."), "SXP-506");
    assert_eq!(
        fail("Let n be the whole number of the quotient of 1.0 and 0.0."),
        "SXP-506"
    );
    assert_eq!(
        fail("Let n be the whole number of 99999999999999999999.0."),
        "SXP-506"
    );
}

#[test]
fn strings_are_sequences_of_scalar_values() {
    assert_eq!(say("Proclaim the text of the length of \"héllo\"."), "5\n");
    assert_eq!(
        say("Let c be item 1 of \"héllo\".\nProclaim the text of the c."),
        "é\n"
    );
    assert_eq!(say("Proclaim \"ab\" plus \"cd\"."), "abcd\n");
    assert_eq!(
        say("Let b be \"apple\" is less than \"banana\".\nProclaim the text of the b."),
        "true\n"
    );
    assert_eq!(fail("Let c be item 9 of \"abc\"."), "SXP-503");
    assert_eq!(
        fail("Let c be item the negation of 1 of \"abc\"."),
        "SXP-503"
    );
}

#[test]
fn lists_have_value_semantics() {
    let src = "Let xs, a list of whole number, become a list of 1, 2, and 3.\n\
        Let ys be the xs.\n\
        Let item 0 of the xs now be 9.\n\
        Proclaim the text of item 0 of the xs.\n\
        Proclaim the text of item 0 of the ys.\n\
        Let zs be the xs with 4 appended.\n\
        Proclaim the text of the length of the zs.\n\
        Proclaim the text of the length of the xs.";
    assert_eq!(say(src), "9\n1\n4\n3\n");
    assert_eq!(
        fail("Let xs be a list of 1.\nLet y be item 1 of the xs."),
        "SXP-503"
    );
    assert_eq!(
        fail("Let xs, a list of whole number, become a list of 1.\nLet item 5 of the xs now be 2."),
        "SXP-503"
    );
}

#[test]
fn logic_short_circuits() {
    // The right operand would divide by zero if it were evaluated.
    assert_eq!(
        say("Let b be both false and the quotient of 1 and 0 is 5.\nProclaim the text of the b."),
        "false\n"
    );
    assert_eq!(
        say("Let b be either true or the quotient of 1 and 0 is 5.\nProclaim the text of the b."),
        "true\n"
    );
    assert_eq!(
        fail("Let b be both true and the quotient of 1 and 0 is 5."),
        "SXP-502"
    );
}

#[test]
fn branches_and_loops() {
    let src = "Let n, a whole number, become 0.\n\
        As long as the n is less than 3, do:\n\
        If the n is 1, then:\nSpeak \"one \".\nOtherwise:\nSpeak \"other \".\nSo be it.\n\
        Let n now be the sum of the n and 1.\nSo be it.";
    assert_eq!(say(src), "other one other ");
    assert_eq!(say("Do this 0 times:\nSpeak \"x\".\nSo be it."), "");
    assert_eq!(
        say("Do this the negation of 3 times:\nSpeak \"x\".\nSo be it."),
        ""
    );
}

#[test]
fn recursion_and_return_from_nested_blocks() {
    let src = "Thus begins t:\nProclaim the text of calling fact with 10.\nProclaim the text of calling bigger with 100.\n\n\
        To fact, taking n as whole number, giving whole number:\n\
        If the n is 0, then:\nGive back 1.\nSo be it.\n\
        Give back the product of the n and calling fact with the difference of the n and 1.\n\n\
        To bigger, taking limit as whole number, giving whole number:\n\
        Let i, a whole number, become 0.\n\
        As long as true, do:\nDo this 1 times:\nIf the i is greater than the limit, then:\nGive back the i.\nSo be it.\nSo be it.\n\
        Let i now be the sum of the i and 7.\nSo be it.\nGive back 0.\n";
    assert_eq!(exec(src, "").1, "3628800\n105\n");
}

fn down(n: usize) -> (Result<(), RuntimeError>, String) {
    let src = format!(
        "Thus begins t:\nProclaim the text of calling down with {n}.\n\n\
         To down, taking n as whole number, giving whole number:\n\
         If the n is 0, then:\nGive back 0.\nOtherwise:\n\
         Give back calling down with the difference of the n and 1.\nSo be it.\n"
    );
    exec(&src, "")
}

#[test]
fn call_depth_limit_is_exact() {
    // Entry is frame 1, then down(n) .. down(0) are n + 1 more.
    let ok = MAX_FRAMES - 2;
    assert_eq!(down(ok), (Ok(()), "0\n".to_string()));
    let err = down(ok + 1).0.unwrap_err();
    assert_eq!(err.code, "SXP-504");
    let spin = "Thus begins t:\nCall spin.\n\nTo spin:\nCall spin.\n";
    assert_eq!(exec(spin, "").0.unwrap_err().code, "SXP-504");
}

#[test]
fn hear_reads_lines_then_hits_eof() {
    let src = "Thus begins t:\nLet first be what is heard.\nLet second be what is heard.\nProclaim first plus second.\nLet third be what is heard.\n";
    let (r, out) = exec(src, "foo\r\nbar\n");
    assert_eq!(out, "foobar\n");
    assert_eq!(r.unwrap_err().code, "SXP-505");
}

#[test]
fn errors_report_the_line_of_the_statement() {
    let e = exec(
        "Thus begins t:\nSpeak \"fine\".\nLet x be the quotient of 1 and 0.\n",
        "",
    )
    .0
    .unwrap_err();
    assert_eq!((e.code, e.line), ("SXP-502", 3));
}

#[test]
fn output_is_kept_when_the_program_later_fails() {
    let (r, out) = exec(
        "Thus begins t:\nProclaim \"before\".\nLet x be the quotient of 1 and 0.\n",
        "",
    );
    assert_eq!(out, "before\n");
    assert!(r.is_err());
}

// ---- the three primitives (ADR-018) --------------------------------------------------------

#[test]
fn a_letter_and_its_code_convert_both_ways() {
    assert_eq!(
        say(
            "Proclaim the text of the code of the letter \"a\".\nProclaim the text of the code of the letter \"é\"."
        ),
        "97\n233\n"
    );
    assert_eq!(
        say(
            "Proclaim the text of the letter with code 98.\nProclaim the text of the letter with code 233."
        ),
        "b\né\n"
    );
    // Upper case without a table: the codes of `a` and `A` differ by 32.
    assert_eq!(
        say(
            "Let the c, a letter, become the letter \"q\".\nProclaim the text of the letter with code the difference of the code of the c and 32."
        ),
        "Q\n"
    );
}

#[test]
fn a_code_that_is_no_letter_is_a_conversion_error() {
    assert_eq!(
        fail("Proclaim the text of the letter with code the negation of 1."),
        "SXP-506"
    );
    assert_eq!(
        fail("Proclaim the text of the letter with code 55296."),
        "SXP-506"
    ); // a surrogate
    assert_eq!(
        fail("Proclaim the text of the letter with code 1114112."),
        "SXP-506"
    ); // past U+10FFFF
}

#[test]
fn more_to_hear_asks_without_consuming() {
    let src = "Thus begins the tale:\nProclaim the text of there is more to hear.\nProclaim what is heard.\nProclaim the text of there is more to hear.\nProclaim what is heard.\nProclaim the text of there is more to hear.\n";
    assert_eq!(
        exec(src, "a\nb\n"),
        (Ok(()), "true\na\ntrue\nb\nfalse\n".to_string())
    );
    // Nothing at all, and a last line with no terminator.
    assert_eq!(exec(src, "").0.unwrap_err().code, "SXP-505");
    let (r, out) = exec(
        "Thus begins the tale:\nProclaim the text of there is more to hear.\nProclaim what is heard.\nProclaim the text of there is more to hear.\n",
        "tail",
    );
    assert_eq!((r, out.as_str()), (Ok(()), "true\ntail\nfalse\n"));
}

#[test]
fn a_filter_reads_until_the_input_ends() {
    let src = "Thus begins the tale:\nLet the total, a whole number, become zero.\nAs long as there is more to hear, do:\nLet the line, a text, become what is heard.\nIf the line reads as a whole number, then:\nLet the total now be the sum of the total and the whole number of the line.\nSo be it.\nSo be it.\nProclaim the text of the total.\n";
    assert_eq!(
        exec(src, "5\nabc\n-2\n 7\n+4\n"),
        (Ok(()), "7\n".to_string())
    );
}

#[test]
fn reads_as_a_whole_number_follows_the_conversion_rule() {
    let cases = [
        ("42", "true"),
        ("-7", "true"),
        ("+3", "true"),
        ("007", "true"),
        ("", "false"),
        ("+", "false"),
        ("1.5", "false"),
        (" 5", "false"),
        ("5 ", "false"),
        ("9223372036854775807", "true"),
        ("9223372036854775808", "false"),
        ("-9223372036854775808", "true"),
        ("abc", "false"),
    ];
    for (text, want) in cases {
        let src = format!(
            "Let the t, a text, become \"{text}\".\nLet the b, a truth, become the t reads as a whole number.\nProclaim the text of the b."
        );
        assert_eq!(say(&src), format!("{want}\n"), "{text:?}");
    }
}
