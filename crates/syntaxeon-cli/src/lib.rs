//! The `syntaxeon` command line. All I/O is injected so the whole CLI is testable.
//!
//! Exit codes: 0 success, 1 invalid program or I/O failure, 2 usage error.
//! Phases run in ADR-009 order and stop at the first failing one, so a program that is both
//! wrong and unmetrical reports its meaning error first.
//!
//! shortcut: `build`, `fmt`, `repl` and `rhyme` are not implemented (`explain` covers rhyme
//! analysis); `--json` is only supported by `check`.

mod args;
mod help;
mod pipeline;
mod render;
mod rhymes;
mod style;

use std::io::{Read, Write};

use syntaxeon_lex::Diagnostic;

use args::{Cmd, Opts, parse};
use pipeline::analyze;
use style::Style;

/// Terminal facts the caller supplies.
pub struct Env {
    pub stdout_tty: bool,
    pub stderr_tty: bool,
    pub no_color_env: bool,
}

fn read_source(path: &str, stdin: &mut dyn Read) -> Result<String, String> {
    let mut s = String::new();
    if path == "-" {
        stdin
            .read_to_string(&mut s)
            .map_err(|e| format!("cannot read standard input: {e}"))?;
    } else {
        s = std::fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    }
    Ok(s)
}

#[must_use]
pub fn run(
    argv: &[String],
    env: &Env,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let (opts, cmd) = match parse(argv) {
        Ok(x) => x,
        Err(args::Usage(msg)) => {
            let _ = writeln!(err, "error: {msg}\n  try 'syntaxeon --help'");
            return 2;
        }
    };
    let color = !opts.no_color && !env.no_color_env;
    let so = Style(color && env.stdout_tty);
    let se = Style(color && env.stderr_tty);
    match cmd {
        Cmd::Help => {
            let _ = write!(out, "{}", help::help(so));
            0
        }
        Cmd::Version => {
            let _ = writeln!(out, "syntaxeon {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Cmd::Man => {
            let _ = write!(out, "{}", help::man());
            0
        }
        Cmd::Rhymes(word) => rhymes::run(&word, opts, out, err),
        Cmd::Completions(shell) => {
            if let Some(script) = help::completions(&shell) {
                let _ = write!(out, "{script}");
                0
            } else {
                let _ = writeln!(
                    err,
                    "error: unknown shell '{shell}'\n  use one of: bash, zsh, fish, powershell"
                );
                2
            }
        }
        Cmd::Check(ref f) | Cmd::Explain(ref f) | Cmd::Ast(ref f) | Cmd::Run(ref f) => {
            if opts.json && !matches!(cmd, Cmd::Check(_)) {
                let _ = writeln!(
                    err,
                    "error: --json is only supported by 'check'\n  try 'syntaxeon --help'"
                );
                return 2;
            }
            if matches!(cmd, Cmd::Run(_)) && f == "-" {
                let _ = writeln!(
                    err,
                    "error: 'run' cannot read the program from standard input\n  standard input is what the program 'hear's; pass a file path"
                );
                return 2;
            }
            file_command(&cmd, f, opts, (so, se), stdin, out, err)
        }
    }
}

fn file_command(
    cmd: &Cmd,
    file: &str,
    opts: Opts,
    (so, se): (Style, Style),
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let src = match read_source(file, stdin) {
        Ok(s) => s,
        Err(msg) => {
            let _ = writeln!(
                err,
                "error: {msg}\n  check the path, or use '-' for standard input"
            );
            return 1;
        }
    };
    let verbose = opts.verbose;
    let mut note = |p: &str| {
        if verbose {
            let _ = writeln!(err, "note: {p}");
        }
    };
    let a = analyze(&src, &mut note);
    if opts.json {
        let _ = write!(out, "{}", render::json(file, &a));
        return i32::from(!a.diagnostics.is_empty());
    }
    if !a.diagnostics.is_empty() {
        for d in &a.diagnostics {
            let _ = write!(err, "{}", render::human(d, file, se));
        }
        let phase = a.failed_phase.unwrap_or("analysis");
        let n = a.diagnostics.len();
        let _ = writeln!(
            err,
            "{}: {file}: {n} error(s) in {phase}\n  fix the first one and run it again",
            se.red("error")
        );
        return 1;
    }
    match cmd {
        Cmd::Explain(_) => {
            let _ = write!(out, "{}", render::explain(&a, so));
        }
        Cmd::Ast(_) => {
            let _ = writeln!(out, "{:#?}", a.program);
        }
        Cmd::Run(_) => {
            let mut input = std::io::BufReader::new(stdin);
            if let Err(e) = syntaxeon_eval::run(&a.program, &mut input, out) {
                let _ = write!(
                    err,
                    "{}",
                    render::human(
                        &Diagnostic {
                            code: e.code,
                            message: e.message,
                            line: e.line
                        },
                        file,
                        se
                    )
                );
                let _ = writeln!(
                    err,
                    "{}: {file}: the program stopped\n  its output above is what it said before failing",
                    se.red("error")
                );
                return 1;
            }
        }
        _ if !opts.quiet => {
            let _ = writeln!(out, "ok: {file} ({} stanza(s))", a.program.stanzas.len());
        }
        _ => {}
    }
    0
}
