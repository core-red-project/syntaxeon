#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
#![allow(clippy::struct_field_names, clippy::similar_names)] // test helper mirrors stdout/stderr

use syntaxeon_cli::{Env, run};

const GOOD: &str = "Thus begins the tale of men:\nSpeak \"Hello again\".\nProclaim the text of ten.\nSpeak \"Amen\".\n";
const BAD_RHYME: &str = "Thus begins the tale of men:\nSpeak \"Hello\".\n";
const BAD_TYPE_AND_RHYME: &str = "Thus begins the tale of men:\nSpeak the missing.\n";

struct Out {
    code: i32,
    out: String,
    err: String,
}

fn exec_env(args: &[&str], stdin: &str, env: &Env) -> Out {
    let argv: Vec<String> = args.iter().map(ToString::to_string).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&argv, env, &mut stdin.as_bytes(), &mut out, &mut err);
    Out {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

fn exec(args: &[&str], stdin: &str) -> Out {
    exec_env(
        args,
        stdin,
        &Env {
            stdout_tty: false,
            stderr_tty: false,
            no_color_env: false,
        },
    )
}

#[test]
fn version_help_and_usage_errors() {
    let o = exec(&["--version"], "");
    assert_eq!(
        (o.code, o.out.as_str()),
        (0, concat!("syntaxeon ", env!("CARGO_PKG_VERSION"), "\n"))
    );
    assert_eq!(exec(&["version"], "").out, o.out);
    let h = exec(&["--help"], "");
    assert_eq!(h.code, 0);
    for section in ["USAGE", "COMMANDS", "OPTIONS", "EXAMPLES"] {
        assert!(h.out.contains(section), "{section}");
    }
    assert_eq!(h.err.len(), 0);
    // `-v` is verbose, never version: with no command it is a usage error.
    let v = exec(&["-v"], "");
    assert_eq!(v.code, 2);
    assert!(v.out.is_empty() && v.err.contains("no command given") && v.err.contains("--help"));
    assert_eq!(exec(&[], "").code, 2);
    assert_eq!(exec(&["--bogus", "check", "-"], "").code, 2);
    assert_eq!(exec(&["check"], "").code, 2);
    assert_eq!(exec(&["check", "a", "b"], "").code, 2);
}

#[test]
fn unknown_command_suggests_the_closest() {
    let o = exec(&["chek", "-"], "");
    assert_eq!(o.code, 2);
    assert!(o.err.contains("did you mean 'check'"), "{}", o.err);
    assert!(!exec(&["zzzzzz"], "").err.contains("did you mean"));
}

#[test]
fn check_accepts_a_valid_program() {
    let o = exec(&["check", "-"], GOOD);
    assert_eq!(o.code, 0);
    assert_eq!(o.out, "ok: - (1 stanza(s))\n");
    assert_eq!(o.err.len(), 0);
    // Global flags work after the command; --quiet silences success.
    let q = exec(&["check", "-", "--quiet"], GOOD);
    assert_eq!((q.code, q.out.as_str(), q.err.as_str()), (0, "", ""));
}

#[test]
fn check_reports_a_rhyme_failure_on_stderr() {
    let o = exec(&["check", "-"], BAD_RHYME);
    assert_eq!(o.code, 1);
    assert_eq!(o.out.len(), 0, "stdout carries data only");
    assert!(o.err.contains("error[SXP-202]"));
    assert!(o.err.contains("The verse hath failed to rhyme."));
    assert!(o.err.contains("-:2") || o.err.contains("-:1"));
    assert!(o.err.contains("in prosody"));
}

#[test]
fn phases_stop_at_the_first_failure() {
    // Meaning is reported before rhyme (ADR-009).
    let o = exec(&["check", "-"], BAD_TYPE_AND_RHYME);
    assert_eq!(o.code, 1);
    assert!(
        o.err.contains("SXP-411") && !o.err.contains("SXP-20"),
        "{}",
        o.err
    );
    let syn = exec(
        &["check", "-"],
        "Thus begins the tale of men:\nDance wildly.\n",
    );
    assert!(syn.err.contains("SXP-103") && syn.err.contains("in syntax"));
}

#[test]
fn json_output() {
    let bad = exec(&["check", "--json", "-"], BAD_RHYME);
    assert_eq!(bad.code, 1);
    assert_eq!(bad.err.len(), 0);
    assert!(
        bad.out
            .starts_with("{\"file\":\"-\",\"ok\":false,\"phase\":\"prosody\"")
    );
    assert!(
        bad.out.contains("\"code\":\"SXP-202\"") && bad.out.contains("\"category\":\"prosodic\"")
    );
    assert!(!bad.out.contains('\x1b'));
    let good = exec(&["check", "--json", "-"], GOOD);
    assert_eq!(good.code, 0);
    assert_eq!(
        good.out,
        "{\"file\":\"-\",\"ok\":true,\"phase\":null,\"diagnostics\":[]}\n"
    );
    // Only `check` has a JSON form.
    assert_eq!(exec(&["ast", "--json", "-"], GOOD).code, 2);
}

#[test]
fn explain_shows_tails_groups_and_verdict() {
    let o = exec(&["explain", "-"], GOOD);
    assert_eq!(o.code, 0);
    assert!(o.out.contains("tail /EH N/"), "{}", o.out);
    assert!(o.out.contains("scheme AABB: satisfied"), "{}", o.out);
    assert!(o.out.contains("stanza 1") && o.out.contains("entry"));
}

#[test]
fn explain_marks_the_verse_that_breaks_the_rhyme() {
    // A rhyme failure makes the program invalid, so `explain` reports it like `check`.
    let o = exec(&["explain", "-"], BAD_RHYME);
    assert_eq!(o.code, 1);
    assert!(o.err.contains("SXP-202"));
}

#[test]
fn structural_verses_are_exempt_in_explain() {
    let src = "Thus begins the tale of men:\nIf true, then:\nSpeak \"ten\".\nSo be it.\n";
    let o = exec(&["explain", "-"], src);
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("structural verse, exempt"));
}

#[test]
fn ast_dumps_the_canonical_tree() {
    let o = exec(&["ast", "-"], GOOD);
    assert_eq!(o.code, 0);
    assert!(o.out.contains("Stanza") && o.out.contains("speak"));
}

#[test]
fn missing_file_is_a_runtime_failure() {
    let o = exec(&["check", "/no/such/file.sxe"], "");
    assert_eq!(o.code, 1);
    assert!(o.err.starts_with("error: cannot read"));
}

#[test]
fn verbose_prints_phases_to_stderr() {
    let o = exec(&["check", "-", "-v"], GOOD);
    assert_eq!(o.code, 0);
    assert!(o.err.contains("note: prosodic validation"));
    assert!(!o.out.contains("note:"));
}

#[test]
fn color_follows_tty_no_color_flag_and_env() {
    let tty = Env {
        stdout_tty: true,
        stderr_tty: true,
        no_color_env: false,
    };
    assert!(exec_env(&["--help"], "", &tty).out.contains('\x1b'));
    assert!(
        exec_env(&["check", "-"], BAD_RHYME, &tty)
            .err
            .contains('\x1b')
    );
    assert!(
        !exec_env(&["--help", "--no-color"], "", &tty)
            .out
            .contains('\x1b')
    );
    let env_off = Env {
        no_color_env: true,
        ..tty
    };
    assert!(
        !exec_env(&["check", "-"], BAD_RHYME, &env_off)
            .err
            .contains('\x1b')
    );
    assert!(!exec(&["--help"], "").out.contains('\x1b'));
}

#[test]
fn completions_and_man() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let o = exec(&["completions", shell], "");
        assert_eq!(o.code, 0, "{shell}");
        assert!(o.out.contains("explain"), "{shell}");
    }
    assert_eq!(exec(&["completions", "tcsh"], "").code, 2);
    assert!(exec(&["man"], "").out.starts_with(".TH SYNTAXEON"));
}

#[test]
fn the_binary_starts() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_syntaxeon"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("syntaxeon {}\n", env!("CARGO_PKG_VERSION"))
    );
}

fn temp_program(name: &str, src: &str) -> String {
    let path = std::env::temp_dir().join(format!("syntaxeon-{}-{name}.sxe", std::process::id()));
    std::fs::write(&path, src).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn run_executes_a_valid_program() {
    let path = temp_program("good", GOOD);
    let o = exec(&["run", &path], "");
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.out, "Hello again10\nAmen");
    assert_eq!(o.err.len(), 0);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn run_refuses_a_program_that_does_not_rhyme() {
    let path = temp_program("bad", BAD_RHYME);
    let o = exec(&["run", &path], "");
    assert_eq!(o.code, 1);
    assert_eq!(o.out.len(), 0, "nothing runs when the program is invalid");
    assert!(o.err.contains("SXP-202"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn run_reports_runtime_errors_after_the_output() {
    // men/again and zero/hero rhyme; the third statement divides by zero.
    let src = "Thus begins the tale of men:\nSpeak \"Hello again\".\nLet n be the quotient of 1 and 0.\nSpeak \"hero\".\n";
    let path = temp_program("boom", src);
    let o = exec(&["run", &path], "");
    assert_eq!(o.code, 1, "{}", o.err);
    assert_eq!(o.out, "Hello again", "output before the failure is kept");
    assert!(
        o.err.contains("error[SXP-502] (runtime)") && o.err.contains(":3"),
        "{}",
        o.err
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn run_feeds_stdin_to_hear_and_reports_eof() {
    let src = "Thus begins the tale of absurd:\nProclaim what is heard.\nProclaim what is heard.\nSpeak \"bird\".\n";
    let path = temp_program("hear", src);
    let ok = exec(&["run", &path], "one\ntwo\n");
    assert_eq!(
        (ok.code, ok.out.as_str()),
        (0, "one\ntwo\nbird"),
        "{}",
        ok.err
    );
    let eof = exec(&["run", &path], "one\n");
    assert_eq!(eof.code, 1);
    assert_eq!(eof.out, "one\n");
    assert!(eof.err.contains("SXP-505"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn run_usage_errors() {
    assert_eq!(exec(&["run", "-"], GOOD).code, 2);
    assert_eq!(exec(&["run", "--json", "x.sxe"], "").code, 2);
    assert_eq!(exec(&["run"], "").code, 2);
}

fn programs_in(dir: &str) -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(dir);
    let mut found: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sxe"))
        .collect();
    found.sort();
    found
}

#[test]
fn every_shipped_program_passes_check() {
    let programs: Vec<_> = programs_in("examples")
        .into_iter()
        .chain(programs_in("benches"))
        .collect();
    assert!(
        programs.len() >= 5,
        "examples and the bench fixture are expected"
    );
    for path in programs {
        let o = exec(&["check", &path.to_string_lossy()], "");
        assert_eq!(o.code, 0, "{}: {}", path.display(), o.err);
    }
}

#[test]
fn shipped_programs_print_what_they_promise() {
    let fizz = "1\n2\nFizz again\n4\nBuzz, say when\nFizz again\n7\n8\nFizz again\nBuzz, say when\n11\nFizz again\n13\n14\nFizzBuzz, amen\nSing it, in rhymes\n";
    let cases = [
        ("hello", "", "Hello again10\nAmen"),
        ("counting", "", "dingsing\ndingsing\ndingsing\n"),
        ("twice", "", "20\n"),
        ("echo", "one\ntwo\n", "one\ntwo\nbird"),
        (
            "powers",
            "",
            "4\n8\n16\n32\n64\n128\nRest a while\n100\nAmen",
        ),
        ("fizzbuzz", "", fizz),
        ("gcd", "", "6\n25\nStill alive"),
        (
            "greeter",
            "Ada\nLovelace\n",
            "Hello, AdaLovelace\nEach letter is a hero\nA\nd\na\nRest, absurd",
        ),
    ];
    for (name, input, want) in cases {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../examples/{name}.sxe"));
        let o = exec(&["run", &path.to_string_lossy()], input);
        assert_eq!(o.code, 0, "{name}: {}", o.err);
        assert_eq!(o.out, want, "{name}");
    }
}

#[test]
fn rhymes_lists_partners_one_per_line() {
    let o = exec(&["rhymes", "THEN", "--limit", "5"], "");
    assert_eq!(o.code, 0, "{}", o.err);
    let words: Vec<&str> = o.out.lines().collect();
    assert_eq!(words.len(), 5);
    assert!(!words.contains(&"then"), "a word is not its own rhyme");
    assert!(o.err.contains("showing 5 of"), "{}", o.err);
    // -q silences the note; the default list skips abbreviations, --all keeps them.
    assert_eq!(exec(&["rhymes", "then", "--limit", "5", "-q"], "").err, "");
    assert!(
        !exec(&["rhymes", "then", "--limit", "0"], "")
            .out
            .lines()
            .any(|w| w == "cnn")
    );
    assert!(
        exec(&["rhymes", "then", "--all", "--limit=0"], "")
            .out
            .lines()
            .any(|w| w == "cnn")
    );
}

#[test]
fn rhymes_json_and_errors() {
    let o = exec(&["rhymes", "light", "--json", "--limit", "3"], "");
    assert_eq!(o.code, 0);
    assert!(
        o.out
            .starts_with("{\"word\":\"light\",\"tail\":\"AY T\",\"total\":"),
        "{}",
        o.out
    );
    assert!(
        o.out.contains("\"shown\":3") && o.out.contains("\"rhymes\":[\""),
        "{}",
        o.out
    );
    assert_eq!(o.err, "");
    let unknown = exec(&["rhymes", "zyzzyva"], "");
    assert_eq!(unknown.code, 1);
    assert!(unknown.err.contains("not in the pronunciation lexicon") && unknown.out.is_empty());
    assert_eq!(exec(&["rhymes", "then", "--limit", "x"], "").code, 2);
    assert_eq!(exec(&["rhymes", "then", "--limit"], "").code, 2);
    assert_eq!(exec(&["rhymes"], "").code, 2);
}

#[test]
fn rhymes_default_list_is_common_words_only() {
    let times = exec(&["rhymes", "times", "--limit", "0"], "").out;
    let words: Vec<&str> = times.lines().collect();
    assert!(words.contains(&"chimes") && words.contains(&"rhymes") && words.contains(&"dimes"));
    assert!(!words.contains(&"imes"), "bare tokens are filtered out");
    let then = exec(&["rhymes", "then", "--limit", "0"], "").out;
    assert!(then.lines().any(|w| w == "men") && !then.lines().any(|w| w == "ren"));
}

// ---- fmt -----------------------------------------------------------------------------------

/// The AST dump without source positions, to compare meaning across formatting.
fn ast_of(src: &str) -> String {
    let o = exec(&["ast", "-"], src);
    assert_eq!(o.code, 0, "{}", o.err);
    o.out
        .lines()
        .filter(|l| !l.contains("line:"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn fmt_keeps_examples_valid_equal_in_meaning_and_stable() {
    let mut seen = 0;
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples")).unwrap()
    {
        let path = entry.unwrap().path();
        let src = std::fs::read_to_string(&path).unwrap();
        let once = exec(&["fmt", "-"], &src);
        assert_eq!(once.code, 0, "{path:?}: {}", once.err);
        assert_eq!(
            exec(&["check", "-q", "-"], &once.out).code,
            0,
            "{path:?} stopped rhyming"
        );
        assert_eq!(ast_of(&once.out), ast_of(&src), "{path:?} changed meaning");
        assert_eq!(
            exec(&["fmt", "-"], &once.out).out,
            once.out,
            "{path:?} not stable"
        );
        seen += 1;
    }
    assert!(seen >= 8);
}

#[test]
fn fmt_prefers_the_declarative_spelling_when_the_rhyme_survives() {
    let src = "Thus begins the tale of men:\nSpeak \"Hello again\".\nLet the pile, a whole number, become ten.\nSpeak \"Amen\".\n";
    assert_eq!(exec(&["check", "-q", "-"], src).code, 0);
    let o = exec(&["fmt", "-"], src);
    assert_eq!(
        o.out,
        "Thus begins the tale of men:\nSpeak \"Hello again\".\nThe pile, a whole number, shall become ten.\nSpeak \"Amen\".\n"
    );
}

#[test]
fn fmt_refuses_a_rewrite_that_would_break_the_rhyme() {
    // "Ten shall the pile … become." ends in `become`, which rhymes with `thumb`; the
    // preferred spelling would end in `ten` and no longer rhyme.
    let src = "Thus begins the tale of thumb:\nTen shall the pile, a whole number, become.\n";
    assert_eq!(exec(&["check", "-q", "-"], src).code, 0);
    assert_eq!(exec(&["fmt", "-"], src).out, src);
}

#[test]
fn fmt_normalizes_whitespace_and_accepts_unrhymed_programs() {
    let o = exec(
        &["fmt", "-"],
        "\n\nThus begins the tale of men:   \nSpeak \"Hello\".  \n\n\n\n",
    );
    assert_eq!(
        (o.code, o.out.as_str()),
        (0, "Thus begins the tale of men:\nSpeak \"Hello\".\n")
    );
}

#[test]
fn fmt_only_formats_valid_programs() {
    let o = exec(&["fmt", "-"], BAD_TYPE_AND_RHYME);
    assert_eq!(o.code, 1);
    assert_eq!(o.out, "");
    assert!(o.err.contains("SXP-411"), "{}", o.err);
}

#[test]
fn fmt_check_and_write() {
    let dir = std::env::temp_dir().join(format!("syntaxeon-fmt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tale.sxe");
    let path = file.to_str().unwrap();
    std::fs::write(&file, "Thus begins the tale of men:  \nSpeak \"Hello again\".\nProclaim the text of ten.\nSpeak \"Amen\".\n").unwrap();
    let check = exec(&["fmt", "--check", path], "");
    assert_eq!((check.code, check.out.as_str()), (1, ""));
    assert!(check.err.contains("not formatted"), "{}", check.err);
    assert_eq!(exec(&["fmt", "--write", path], "").code, 0);
    assert!(!std::fs::read_to_string(&file).unwrap().contains("  \n"));
    assert_eq!(exec(&["fmt", "--check", path], "").code, 0);
    assert_eq!(exec(&["fmt", "--write", "-"], "").code, 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fmt_respells_a_sentence_split_over_two_lines_when_the_rhyme_survives() {
    // The split sentence gives verses `hen` and `ten`; joined, the stanza still rhymes.
    let src = "Thus begins the tale of men:\nSpeak \"Hello again\".\nLet the hen,\na whole number, become ten.\nSpeak \"Amen\".\n";
    assert_eq!(exec(&["check", "-q", "-"], src).code, 0);
    assert_eq!(
        exec(&["fmt", "-"], src).out,
        "Thus begins the tale of men:\nSpeak \"Hello again\".\nThe hen, a whole number, shall become ten.\nSpeak \"Amen\".\n"
    );
}

#[test]
fn fmt_keeps_a_split_sentence_whose_join_would_break_the_rhyme() {
    // Split, the stanza is AABB (men/hen, ten/amen); joined it is AAA and `men`/`amen` share
    // an onset, so the file must stay as written.
    let src = "Thus begins the tale of men:\nLet the hen,\na whole number, become ten.\nSpeak \"Amen\".\n";
    assert_eq!(exec(&["check", "-q", "-"], src).code, 0);
    assert_eq!(exec(&["fmt", "-"], src).out, src);
}

// ---- expression spans ----------------------------------------------------------------------

#[test]
fn type_errors_underline_the_offending_expression() {
    let src = "Thus begins the tale of men:\nLet the pile, a whole number, become the sum of one and \"ten\".\n";
    let o = exec(&["check", "-"], src);
    assert_eq!(o.code, 1);
    assert!(o.err.contains("-:2:57"), "{}", o.err);
    assert!(
        o.err.contains(
            "2 | Let the pile, a whole number, become the sum of one and \"ten\".\n  |                                                         ^^^^^\n"
        ),
        "{}",
        o.err
    );
}

#[test]
fn json_diagnostics_carry_column_and_span_or_null() {
    let src = "Thus begins the tale of men:\nLet the pile, a whole number, become the sum of one and \"ten\".\nSpeak the missing.\n";
    let o = exec(&["--json", "check", "-"], src);
    assert!(
        o.out
            .contains("\"line\":2,\"column\":57,\"span\":{\"start\":"),
        "{}",
        o.out
    );
    // A statement-level error has no expression to point at.
    let o = exec(&["--json", "check", "-"], BAD_RHYME);
    assert!(o.out.contains("\"column\":null,\"span\":null"), "{}", o.out);
}

#[test]
fn runtime_errors_underline_the_failing_expression() {
    let dir = std::env::temp_dir().join(format!("syntaxeon-span-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tale.sxe");
    std::fs::write(
        &file,
        "Thus begins the tale of thumb:\nZero shall the hen, a whole number, become.\nLet the den, a whole number, become the quotient of ten and the hen.\nSpeak \"Amen\".\n",
    )
    .unwrap();
    let path = file.to_str().unwrap();
    assert_eq!(exec(&["check", "-q", path], "").code, 0);
    let o = exec(&["run", path], "");
    assert_eq!(o.code, 1);
    assert!(o.err.contains("SXP-502"), "{}", o.err);
    assert!(o.err.contains(":3:37"), "{}", o.err);
    assert!(
        o.err
            .contains(&"^".repeat("the quotient of ten and the hen".len())),
        "{}",
        o.err
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
