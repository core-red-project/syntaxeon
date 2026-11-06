//! Diagnostic rendering (human and JSON) and the `explain` report.

use std::collections::HashSet;
use std::fmt::Write as _;

use syntaxeon_lex::Diagnostic;
use syntaxeon_lexicon::rhyme::tail_string;
use syntaxeon_lexicon::scheme::{StanzaError, find_scheme};

use crate::style::Style;
use syntaxeon_analysis::{Analysis, category, headline};

/// 1-based (line, column) of byte offset `at` in `src`, with the text of that line.
fn locate(src: &str, at: usize) -> Option<(usize, usize, &str)> {
    let before = src.get(..at)?;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[line_start..]
        .find('\n')
        .map_or(src.len(), |i| line_start + i);
    let line = before.matches('\n').count() + 1;
    let col = before[line_start..].chars().count() + 1;
    Some((line, col, src[line_start..line_end].trim_end_matches('\r')))
}

pub fn human(d: &Diagnostic, file: &str, src: &str, s: Style) -> String {
    let head = headline(d.code).unwrap_or(&d.message);
    let mut out = format!(
        "{}{} {}\n",
        s.red(&format!("error[{}]", d.code)),
        s.dim(&format!(" ({})", category(d.code))),
        head
    );
    let spot = d
        .span
        .and_then(|sp| locate(src, sp.start).map(|(l, c, text)| (sp, l, c, text)));
    if let Some((sp, line, col, text)) = spot {
        // Underline the part of the span that lies on its first line.
        let width = src[sp.start..sp.end.min(src.len())]
            .chars()
            .take_while(|c| *c != '\n')
            .count()
            .max(1);
        let gutter = " ".repeat(line.to_string().len());
        let _ = writeln!(out, "  {} {file}:{line}:{col}", s.blue("-->"));
        let _ = writeln!(out, "{gutter} {}", s.blue("|"));
        let _ = writeln!(out, "{line} {} {text}", s.blue("|"));
        let _ = writeln!(
            out,
            "{gutter} {} {}{}",
            s.blue("|"),
            " ".repeat(col - 1),
            s.red(&"^".repeat(width))
        );
    } else if d.line > 0 {
        let _ = writeln!(out, "  {} {file}:{}", s.blue("-->"), d.line);
    }
    if headline(d.code).is_some() {
        let _ = writeln!(out, "   = {}", d.message);
    }
    out
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `{"file", "ok", "phase", "diagnostics":[{"code","category","severity","line","column","span","message"}]}`;
/// `column` and `span` (`{"start","end"}` byte offsets) are `null` unless the error is about one expression.
pub fn json(file: &str, src: &str, a: &Analysis) -> String {
    let items: Vec<String> = a
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{{\"code\":{},\"category\":{},\"severity\":\"error\",\"line\":{},\"column\":{},\"span\":{},\"message\":{}}}",
                json_str(d.code),
                json_str(category(d.code)),
                d.line,
                d.span
                    .and_then(|sp| locate(src, sp.start))
                    .map_or("null".to_string(), |(_, c, _)| c.to_string()),
                d.span.map_or("null".to_string(), |sp| format!(
                    "{{\"start\":{},\"end\":{}}}",
                    sp.start, sp.end
                )),
                json_str(&d.message)
            )
        })
        .collect();
    let phase = a.failed_phase.map_or("null".to_string(), json_str);
    format!(
        "{{\"file\":{},\"ok\":{},\"phase\":{},\"diagnostics\":[{}]}}\n",
        json_str(file),
        a.diagnostics.is_empty(),
        phase,
        items.join(",")
    )
}

fn letter(group: usize) -> char {
    char::from(b'A' + u8::try_from(group % 26).unwrap_or(0))
}

/// Stanza-by-stanza report. Requires that prosody ran (`a.lexicon` is set).
pub fn explain(a: &Analysis, s: Style) -> String {
    let Some(lex) = a.lexicon.as_ref() else {
        return String::new();
    };
    let mut out = String::new();
    for (n, st) in a.program.stanzas.iter().enumerate() {
        let verses = &a.verses[st.id];
        let role = if st.id == 0 { "entry" } else { "function" };
        let _ = writeln!(
            out,
            "{}",
            s.bold(&format!(
                "stanza {} {:?} ({role}, line {})",
                n + 1,
                st.title,
                st.line
            ))
        );
        for p in &st.params {
            let _ = writeln!(out, "  takes {:?}: {:?}", p.name, p.ty);
        }
        let _ = writeln!(
            out,
            "  {} top-level statement(s), returns {:?}",
            st.body.len(),
            st.ret
        );

        let counting: Vec<_> = verses.verses.iter().filter(|v| !v.structural).collect();
        let words: Vec<&str> = counting
            .iter()
            .filter_map(|v| v.end_word.as_deref().ok())
            .collect();
        let result = (words.len() == counting.len()).then(|| find_scheme(lex, &words));
        let scheme = match &result {
            Some(Ok(sc) | Err(StanzaError::NoScheme { scheme: sc, .. })) => sc.clone(),
            _ => vec![],
        };
        let failing: HashSet<usize> = match &result {
            Some(Err(StanzaError::NoScheme { failing, .. })) => failing.iter().copied().collect(),
            _ => HashSet::new(),
        };
        let mut k = 0;
        for v in &verses.verses {
            if v.structural {
                let _ = writeln!(
                    out,
                    "  line {:<3} {}",
                    v.line,
                    s.dim("(structural verse, exempt)")
                );
                continue;
            }
            let word = v.end_word.as_deref().unwrap_or("?");
            let tail = lex
                .lookup(word)
                .and_then(|prons| prons.first())
                .and_then(|p| tail_string(p))
                .map_or(String::from("?"), |t| format!("/{t}/"));
            let group = scheme.get(k).map_or('-', |&g| letter(g));
            let mark = if failing.contains(&k) {
                s.red("  <- breaks the rhyme")
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  line {:<3} end word {word:<12} tail {tail:<10} group {group}{mark}",
                v.line
            );
            k += 1;
        }
        let letters: String = scheme.iter().map(|&g| letter(g)).collect();
        let verdict = match &result {
            Some(Ok(_)) => format!("scheme {letters}: satisfied"),
            Some(Err(e @ StanzaError::NoScheme { .. })) => {
                format!("closest scheme {letters}: NOT satisfied ({})", e.code())
            }
            Some(Err(e)) => format!("not checkable ({})", e.code()),
            None => "not checkable (a verse has no end word)".to_string(),
        };
        let _ = writeln!(out, "  {verdict}\n");
    }
    out
}
