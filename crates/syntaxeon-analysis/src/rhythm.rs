//! `rhythm.toml`: the project's own rhyme rules (`spec/PROSODY.md` §12, ADR-015).
//!
//! The file is found like `rustfmt.toml`: the nearest one in the source file's directory or any
//! parent. Without one the defaults apply. Only the subset of TOML the file needs is read:
//! `[rhyme]` and boolean keys; anything else is an error, so a typo cannot silently relax or
//! tighten the rules.
//!
//! ```toml
//! [rhyme]
//! assonance = false   # only perfect rhyme is valid
//! ```

use std::path::{Path, PathBuf};

use syntaxeon_lexicon::lexicon::RhymeRules;

pub const FILE_NAME: &str = "rhythm.toml";

/// The rules in force and the file they came from, if any.
pub struct Rhythm {
    pub rules: RhymeRules,
    pub path: Option<PathBuf>,
}

/// Reads `text` as a `rhythm.toml`; the error is `(line, message)`.
pub fn parse(text: &str) -> Result<RhymeRules, (usize, String)> {
    let mut rules = RhymeRules::default();
    let mut in_rhyme = false;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let n = i + 1;
        if line.is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if section.trim() != "rhyme" {
                return Err((
                    n,
                    format!("unknown section [{}]; only [rhyme] exists", section.trim()),
                ));
            }
            in_rhyme = true;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err((n, format!("expected 'key = value', found {line:?}")));
        };
        let (key, value) = (key.trim(), value.trim());
        if !in_rhyme {
            return Err((n, format!("'{key}' must come after the [rhyme] header")));
        }
        let flag = match value {
            "true" => true,
            "false" => false,
            _ => return Err((n, format!("'{key}' needs true or false, found {value:?}"))),
        };
        match key {
            "assonance" => rules.assonance = flag,
            _ => return Err((n, format!("unknown key '{key}'; only 'assonance' exists"))),
        }
    }
    Ok(rules)
}

fn find(file: &str) -> Option<PathBuf> {
    let start = if file == "-" {
        std::env::current_dir().ok()?
    } else {
        let dir = Path::new(file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty());
        let dir = dir.unwrap_or_else(|| Path::new("."));
        dir.canonicalize().ok()?
    };
    start
        .ancestors()
        .map(|d| d.join(FILE_NAME))
        .find(|p| p.is_file())
}

/// The rules for the program in `file` (`-` means the current directory). The error is a
/// ready-to-print message for an unreadable or malformed `rhythm.toml` (`SXP-305`).
pub fn load(file: &str) -> Result<Rhythm, String> {
    let Some(path) = find(file) else {
        return Ok(Rhythm {
            rules: RhymeRules::default(),
            path: None,
        });
    };
    let shown = path.display();
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!("error[SXP-305]: cannot read {shown}: {e}\n  fix its permissions or remove it")
    })?;
    let rules = parse(&text).map_err(|(line, msg)| {
        format!("error[SXP-305]: {shown}:{line}: {msg}\n  fix rhythm.toml (see spec/PROSODY.md §12) or remove it")
    })?;
    Ok(Rhythm {
        rules,
        path: Some(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_allow_assonance() {
        assert_eq!(parse("").unwrap(), RhymeRules::default());
        assert!(RhymeRules::default().assonance);
    }

    #[test]
    fn reads_the_one_key_with_comments() {
        let r = parse("# mine\n[rhyme]\nassonance = false # strict\n").unwrap();
        assert_eq!(r, RhymeRules::perfect());
    }

    #[test]
    fn typos_are_errors_with_their_line() {
        assert_eq!(parse("[rhyme]\nassonanse = false\n").unwrap_err().0, 2);
        assert_eq!(parse("[rime]\n").unwrap_err().0, 1);
        assert_eq!(parse("assonance = false\n").unwrap_err().0, 1);
        assert_eq!(parse("[rhyme]\nassonance = no\n").unwrap_err().0, 2);
        assert_eq!(parse("[rhyme]\nassonance\n").unwrap_err().0, 2);
    }
}
