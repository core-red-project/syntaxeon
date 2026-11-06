//! `syntaxeon-lsp`: the language server over standard input and output.
//!
//! Exit codes: 0 after a clean shutdown, 1 on a protocol or I/O failure, 2 on a usage error.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("syntaxeon-lsp {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!(
                    "syntaxeon-lsp - the Syntaxeon language server\n\n\
USAGE\n  syntaxeon-lsp [--stdio]\n\n\
It speaks the Language Server Protocol over standard input and output; an editor \
starts it. Options: -h/--help, -V/--version. --stdio is accepted and is the default."
                );
                return ExitCode::SUCCESS;
            }
            "--stdio" => {}
            other => {
                eprintln!("error: unknown option '{other}'\n  try 'syntaxeon-lsp --help'");
                return ExitCode::from(2);
            }
        }
    }
    let (connection, threads) = lsp_server::Connection::stdio();
    let result = syntaxeon_lsp::run(&connection);
    // The writer thread must finish so the last message is flushed.
    let joined = threads.join();
    match (result, joined) {
        (Ok(()), Ok(())) => ExitCode::SUCCESS,
        (Err(e), _) => {
            eprintln!("error: {e}\n  the editor and the server disagree; restart the server");
            ExitCode::FAILURE
        }
        (_, Err(e)) => {
            eprintln!("error: {e}\n  could not flush the last message");
            ExitCode::FAILURE
        }
    }
}
