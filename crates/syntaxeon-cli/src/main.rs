use std::io::{IsTerminal, stderr, stdin, stdout};

use syntaxeon_cli::{Env, run};

/// The evaluator recurses on the native stack and the language allows 10 000 call frames
/// (`spec/SEMANTIC_CORE.md` §6), so everything runs on a thread with a generous stack.
const STACK_BYTES: usize = 256 << 20;

fn main() {
    let spawned = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(real_main);
    let code = match spawned {
        Ok(handle) => handle.join().unwrap_or(1),
        Err(e) => {
            eprintln!("error: cannot start the worker thread: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = Env {
        stdout_tty: stdout().is_terminal(),
        stderr_tty: stderr().is_terminal(),
        no_color_env: std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()),
    };
    run(
        &args,
        &env,
        &mut stdin().lock(),
        &mut stdout().lock(),
        &mut stderr().lock(),
    )
}
