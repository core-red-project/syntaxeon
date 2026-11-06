//! `syntaxeon rhymes <word>`: words that rhyme with `word` under `spec/PROSODY.md` §4.

use std::io::Write;

use syntaxeon_lexicon::lexicon::Lexicon;
use syntaxeon_lexicon::rhyme::{near_rhymes_with, rhymes_with, tail_string};

use crate::args::Opts;
use syntaxeon_analysis::rhythm;

const DEFAULT_LIMIT: usize = 40;

fn json_list(words: &[&str]) -> String {
    let items: Vec<String> = words.iter().map(|w| format!("\"{w}\"")).collect();
    format!("[{}]", items.join(","))
}

pub fn run(word: &str, opts: Opts, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let lex = match Lexicon::pinned() {
        Ok(l) => l,
        Err(e) => {
            let _ = writeln!(err, "error[{}] {e}\n  reinstall syntaxeon", e.code());
            return 1;
        }
    };
    let key = word.to_lowercase();
    let Ok(all) = rhymes_with(&lex, &key) else {
        let _ = writeln!(
            err,
            "error: '{word}' is not in the pronunciation lexicon\n  a program can add it with (pronounced \"{key}\" as \"PHONEMES\")"
        );
        return 1;
    };
    let candidates: Vec<&str> = if opts.all {
        all
    } else {
        all.into_iter().filter(|w| lex.is_common(w)).collect()
    };
    let limit = match opts.limit {
        Some(0) => usize::MAX,
        Some(n) => n,
        None => DEFAULT_LIMIT,
    };
    let shown = &candidates[..candidates.len().min(limit)];
    // Near (assonant) rhymes come after the perfect ones, and only if the project allows them.
    let near_allowed = match rhythm::load("-") {
        Ok(r) => r.rules.assonance,
        Err(msg) => {
            let _ = writeln!(err, "{msg}");
            return 1;
        }
    };
    let near_all: Vec<&str> = if opts.near && near_allowed {
        near_rhymes_with(&lex, &key)
            .unwrap_or_default()
            .into_iter()
            .filter(|w| opts.all || lex.is_common(w))
            .collect()
    } else {
        Vec::new()
    };
    let near = &near_all[..near_all.len().min(limit)];
    if opts.near && !near_allowed && !opts.quiet {
        let _ = writeln!(
            err,
            "note: assonance is off in rhythm.toml, so there are no near rhymes"
        );
    }
    if opts.json {
        let tail = lex
            .lookup(&key)
            .and_then(|p| p.first())
            .and_then(|p| tail_string(p))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "{{\"word\":\"{key}\",\"tail\":\"{tail}\",\"total\":{},\"shown\":{},\"rhymes\":{},\"near\":{}}}",
            candidates.len(),
            shown.len(),
            json_list(shown),
            json_list(near)
        );
        return 0;
    }
    for w in shown.iter().chain(near) {
        let _ = writeln!(out, "{w}");
    }
    if !near.is_empty() && !opts.quiet {
        let _ = writeln!(
            err,
            "note: the last {} word(s) rhyme only by assonance",
            near.len()
        );
    }
    if shown.len() < candidates.len() && !opts.quiet {
        let _ = writeln!(
            err,
            "note: showing {} of {} rhymes; use --limit 0 for all{}",
            shown.len(),
            candidates.len(),
            if opts.all {
                ""
            } else {
                " (--all also lists words outside the common-word list)"
            }
        );
    }
    if shown.is_empty() && near.is_empty() && !opts.quiet {
        let _ = writeln!(err, "note: no rhymes found for '{word}'; try --all");
    }
    0
}
