//! Hand-rolled argument parsing (no dependencies); global flags work before or after the command.

#[derive(Clone, Copy, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // one bool per independent global flag
pub struct Opts {
    pub quiet: bool,
    pub verbose: bool,
    pub json: bool,
    pub no_color: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Help,
    Version,
    Check(String),
    Explain(String),
    Ast(String),
    Run(String),
    Completions(String),
    Man,
}

pub const COMMANDS: [&str; 7] = [
    "check",
    "explain",
    "ast",
    "completions",
    "help",
    "version",
    "man",
];

/// A usage error: message plus the hint line.
pub struct Usage(pub String);

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push(
                (prev[j] + usize::from(ca != cb))
                    .min(prev[j + 1] + 1)
                    .min(cur[j] + 1),
            );
        }
        prev = cur;
    }
    prev[b.len()]
}

fn suggest(word: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .copied()
        .filter(|c| distance(word, c) <= 2)
        .min_by_key(|c| distance(word, c))
}

pub fn parse(args: &[String]) -> Result<(Opts, Cmd), Usage> {
    let mut opts = Opts::default();
    let mut positional: Vec<&str> = Vec::new();
    let (mut help, mut version) = (false, false);
    for a in args {
        match a.as_str() {
            "-h" | "--help" => help = true,
            "-V" | "--version" => version = true,
            "-q" | "--quiet" => opts.quiet = true,
            "-v" | "--verbose" => opts.verbose = true,
            "--json" => opts.json = true,
            "--no-color" => opts.no_color = true,
            f if f.starts_with('-') && f != "-" => {
                return Err(Usage(format!("unknown option '{f}'")));
            }
            p => positional.push(p),
        }
    }
    if help {
        return Ok((opts, Cmd::Help));
    }
    if version {
        return Ok((opts, Cmd::Version));
    }
    let Some((&command, rest)) = positional.split_first() else {
        return Err(Usage("no command given".to_string()));
    };
    let one = |rest: &[&str], what: &str| match rest {
        [x] => Ok((*x).to_string()),
        [] => Err(Usage(format!("'{command}' needs {what}"))),
        _ => Err(Usage(format!("'{command}' takes exactly one {what}"))),
    };
    let cmd = match command {
        "help" => Cmd::Help,
        "version" => Cmd::Version,
        "man" => Cmd::Man,
        "check" => Cmd::Check(one(rest, "a file (or '-' for stdin)")?),
        "explain" => Cmd::Explain(one(rest, "a file (or '-' for stdin)")?),
        "run" => Cmd::Run(one(rest, "a file")?),
        "ast" => Cmd::Ast(one(rest, "a file (or '-' for stdin)")?),
        "completions" => Cmd::Completions(one(rest, "a shell")?),
        other => {
            return Err(Usage(match suggest(other) {
                Some(s) => format!("unknown command '{other}'; did you mean '{s}'?"),
                None => format!("unknown command '{other}'"),
            }));
        }
    };
    Ok((opts, cmd))
}
