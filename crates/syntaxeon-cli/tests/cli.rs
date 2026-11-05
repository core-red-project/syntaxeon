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
    assert!(h.err.is_empty());
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
    assert!(o.err.is_empty());
    // Global flags work after the command; --quiet silences success.
    let q = exec(&["check", "-", "--quiet"], GOOD);
    assert_eq!((q.code, q.out.as_str(), q.err.as_str()), (0, "", ""));
}

#[test]
fn check_reports_a_rhyme_failure_on_stderr() {
    let o = exec(&["check", "-"], BAD_RHYME);
    assert_eq!(o.code, 1);
    assert!(o.out.is_empty(), "stdout carries data only");
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
    assert!(bad.err.is_empty());
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
    assert!(o.err.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn run_refuses_a_program_that_does_not_rhyme() {
    let path = temp_program("bad", BAD_RHYME);
    let o = exec(&["run", &path], "");
    assert_eq!(o.code, 1);
    assert!(o.out.is_empty(), "nothing runs when the program is invalid");
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
