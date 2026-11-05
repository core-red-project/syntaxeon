//! Help text, completion scripts and the man page.

use crate::style::Style;

pub fn help(s: Style) -> String {
    let h = |t: &str| s.bold(t);
    let c = |t: &str| s.cyan(t);
    let f = |t: &str| s.yellow(t);
    format!(
        "syntaxeon - check and explain programs written in prose that must also rhyme.\n\n\
{usage}\n  syntaxeon <command> [options] <file>\n\n\
{commands}\n\
  {check}        Check syntax, types and rhyme; exit 1 if the program is not valid\n\
  {explain}      Show how each stanza reads, its end-word tails and rhyme scheme\n\
  {ast}          Dump the canonical AST\n\
  {run}          Check, then run the program (input for 'hear' comes from stdin)\n\
  {completions}  Print a completion script (bash, zsh, fish, powershell)\n\
  {help}, {version}\n\n\
{options}\n\
  {h1}, {h2}        Show this help\n\
  {v1}, {v2}  Print the version\n\
  {q1}, {q2}       Suppress non-error output\n\
  {vb1}, {vb2}     Print each phase to stderr\n\
  {j}              Machine-readable output (check only)\n\
  {nc}          Disable color (also: NO_COLOR, or output not a terminal)\n\n\
{examples}\n\
  {ex1}\n\
  {c1}\n\
  {ex3}\n\
  {c3}\n\
  {ex2}\n\
  {c2}\n\n\
Use '-' as the file to read standard input. Exit codes: 0 ok, 1 invalid program or I/O failure, 2 usage error.\n",
        usage = h("USAGE"),
        commands = h("COMMANDS"),
        options = h("OPTIONS"),
        examples = h("EXAMPLES"),
        check = c("check <file>"),
        explain = c("explain <file>"),
        ast = c("ast <file>"),
        run = c("run <file>"),
        completions = c("completions <shell>"),
        help = c("help"),
        version = c("version"),
        h1 = f("-h"),
        h2 = f("--help"),
        v1 = f("-V"),
        v2 = f("--version"),
        q1 = f("-q"),
        q2 = f("--quiet"),
        vb1 = f("-v"),
        vb2 = f("--verbose"),
        j = f("--json"),
        nc = f("--no-color"),
        ex1 = s.dim("# validate a program"),
        c1 = "syntaxeon check tale.sxe",
        ex3 = s.dim("# run it, feeding lines to 'hear'"),
        c3 = "printf 'Ada\\n' | syntaxeon run tale.sxe",
        ex2 = s.dim("# see why a stanza does or does not rhyme"),
        c2 = "syntaxeon explain tale.sxe",
    )
}

pub fn completions(shell: &str) -> Option<&'static str> {
    Some(match shell {
        "bash" => {
            "_syntaxeon() {\n  local cur=\"${COMP_WORDS[COMP_CWORD]}\"\n  COMPREPLY=( $(compgen -W \"check explain ast run completions help version --help --version --quiet --verbose --json --no-color\" -- \"$cur\") )\n}\ncomplete -F _syntaxeon syntaxeon\n"
        }
        "zsh" => {
            "#compdef syntaxeon\n_arguments \\\n  '(-h --help)'{-h,--help}'[show help]' \\\n  '(-V --version)'{-V,--version}'[print version]' \\\n  '(-q --quiet)'{-q,--quiet}'[suppress non-error output]' \\\n  '(-v --verbose)'{-v,--verbose}'[print each phase]' \\\n  '--json[machine-readable output]' \\\n  '--no-color[disable color]' \\\n  '1:command:(check explain ast run completions help version)' \\\n  '*:file:_files'\n"
        }
        "fish" => {
            "complete -c syntaxeon -f -n __fish_use_subcommand -a 'check explain ast run completions help version'\ncomplete -c syntaxeon -s h -l help -d 'show help'\ncomplete -c syntaxeon -s V -l version -d 'print version'\ncomplete -c syntaxeon -s q -l quiet -d 'suppress non-error output'\ncomplete -c syntaxeon -s v -l verbose -d 'print each phase'\ncomplete -c syntaxeon -l json -d 'machine-readable output'\ncomplete -c syntaxeon -l no-color -d 'disable color'\n"
        }
        "powershell" => {
            "Register-ArgumentCompleter -Native -CommandName syntaxeon -ScriptBlock {\n  param($wordToComplete)\n  'check','explain','ast','run','completions','help','version','--help','--version','--quiet','--verbose','--json','--no-color' |\n    Where-Object { $_ -like \"$wordToComplete*\" } |\n    ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }\n}\n"
        }
        _ => return None,
    })
}

pub fn man() -> String {
    let v = env!("CARGO_PKG_VERSION");
    format!(
        ".TH SYNTAXEON 1 \"\" \"syntaxeon {v}\"\n.SH NAME\nsyntaxeon \\- check and explain programs written in prose that must also rhyme\n.SH SYNOPSIS\n.B syntaxeon\n<command> [options] <file>\n.SH COMMANDS\n.TP\n.B check\nCheck syntax, types and rhyme.\n.TP\n.B explain\nShow each stanza's end-word tails and rhyme scheme.\n.TP\n.B ast\nDump the canonical AST.\n.TP\n.B run\nCheck, then run the program.\n.TP\n.B completions\nPrint a shell completion script.\n.SH EXIT STATUS\n0 success, 1 invalid program or I/O failure, 2 usage error.\n"
    )
}
