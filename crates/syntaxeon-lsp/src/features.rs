//! What the server answers, as plain functions over one analyzed document.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CompletionItem, CompletionItemKind,
    CompletionTextEdit, Diagnostic, DiagnosticSeverity, Hover, HoverContents, Location,
    MarkupContent, MarkupKind, NumberOrString, Position, PrepareRenameResponse, Range,
    SemanticToken, SemanticTokens, TextEdit, Uri, WorkspaceEdit,
};
use serde_json::{Value, json};
use syntaxeon_analysis::report::{StanzaInfo, VerseInfo, lexicon_for, verse_report};
use syntaxeon_analysis::{analyze, headline, rhythm};
use syntaxeon_core::Span;
use syntaxeon_lex::Diagnostic as Found;
use syntaxeon_lexicon::lexicon::{Lexicon, RhymeRules};
use syntaxeon_lexicon::rhyme::{RhymeKind, rhyme_kind, rhymes_with};

use crate::names::{self, DefKind};
use crate::position::LineIndex;

/// Rhyme groups are shown with one of these token types, repeating after the last.
pub const GROUP_TOKENS: [&str; 6] = ["rhymeA", "rhymeB", "rhymeC", "rhymeD", "rhymeE", "rhymeF"];
const GROUP_COLORS: usize = GROUP_TOKENS.len();

/// One document, analyzed.
pub struct Doc {
    pub text: String,
    pub rules: RhymeRules,
    diagnostics: Vec<Found>,
    pub report: Vec<StanzaInfo>,
    index: names::Index,
    path: Option<PathBuf>,
}

impl Doc {
    /// Analyzes `text`, with the rhyme rules of the `rhythm.toml` near `path` (or the current
    /// directory when the document has no path). A broken `rhythm.toml` becomes a diagnostic on
    /// the first line, and the defaults apply meanwhile.
    #[must_use]
    pub fn new(path: Option<&Path>, text: String) -> Doc {
        let file = path.and_then(Path::to_str).unwrap_or("-");
        let (rules, rhythm_error) = match rhythm::load(file) {
            Ok(r) => (r.rules, None),
            Err(message) => (RhymeRules::default(), Some(message)),
        };
        let a = analyze(&text, rules, &mut |_| {});
        let mut diagnostics = a.diagnostics.clone();
        if let Some(message) = rhythm_error {
            diagnostics.insert(
                0,
                Found {
                    code: "SXP-305",
                    message: message.lines().next().unwrap_or_default().to_string(),
                    line: 1,
                    span: None,
                },
            );
        }
        let report = lexicon_for(&a.lexed, rules)
            .map(|lex| verse_report(&a.verses, &lex))
            .unwrap_or_default();
        let index = names::build(&text, &a.program);
        Doc {
            text,
            rules,
            diagnostics,
            report,
            index,
            path: path.map(Path::to_path_buf),
        }
    }

    fn verses(&self) -> impl Iterator<Item = (usize, usize, &VerseInfo)> {
        self.report
            .iter()
            .enumerate()
            .flat_map(|(s, st)| st.verses.iter().enumerate().map(move |(v, vi)| (s, v, vi)))
    }
}

fn range_of(ix: &LineIndex, d: &Found) -> Range {
    match d.span {
        Some(span) => ix.range(span),
        None => ix.line_range(d.line.max(1)),
    }
}

pub fn diagnostics(doc: &Doc) -> Vec<Diagnostic> {
    let ix = LineIndex::new(&doc.text);
    doc.diagnostics
        .iter()
        .map(|d| Diagnostic {
            range: range_of(&ix, d),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(d.code.to_string())),
            source: Some("syntaxeon".to_string()),
            message: match headline(d.code) {
                Some(h) => format!("{h}\n{}", d.message),
                None => d.message.clone(),
            },
            ..Diagnostic::default()
        })
        .collect()
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

/// How a verse sounds against its leader, in words.
fn relation(doc: &Doc, v: &VerseInfo, stanza: &StanzaInfo) -> Option<String> {
    let lead = &stanza.verses[v.leader?];
    let (kind, accepted) = (v.kind?, v.rhymes?);
    let with = format!(
        "line {} (`{}`)",
        lead.line,
        lead.end_word.as_deref().unwrap_or("?")
    );
    Some(match (kind, accepted) {
        (RhymeKind::Perfect, _) => format!("Perfect rhyme with {with}."),
        (RhymeKind::Assonant, true) => format!("Assonance with {with}, accepted."),
        (RhymeKind::Assonant, false) => format!(
            "Assonance only with {with}; `rhythm.toml` accepts perfect rhyme only{}.",
            if doc.rules.assonance {
                ""
            } else {
                " (assonance = false)"
            }
        ),
        (RhymeKind::Identical, _) => format!("Identical to {with}: the same sound is not a rhyme."),
        (RhymeKind::None, _) => format!("Does not rhyme with {with}."),
    })
}

pub fn hover(doc: &Doc, at: Position) -> Option<Hover> {
    let ix = LineIndex::new(&doc.text);
    let offset = ix.offset(at);
    let (s, _, v) = doc
        .verses()
        .find(|(_, _, v)| v.end_span.is_some_and(|sp| contains(sp, offset)) && !v.structural)?;
    let word = v.end_word.as_deref()?;
    let mut parts = vec![format!("**{word}**")];
    match v.prons.first() {
        Some(p) => {
            let mut line = format!("**{word}** — `{p}`");
            if v.prons.len() > 1 {
                line = format!("{line} · also `{}`", v.prons[1..].join("`, `"));
            }
            parts[0] = line;
            if let Some(t) = v.tails.first() {
                parts.push(format!("rhyme tail `{t}`"));
            }
        }
        None => parts.push("not in the pronunciation lexicon".to_string()),
    }
    if let Some(g) = v.group {
        parts.push(format!("group **{g}**"));
    }
    if let Some(r) = relation(doc, v, &doc.report[s]) {
        parts.push(r);
    }
    let md = parts.join("\n\n");
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: v.end_span.map(|sp| ix.range(sp)),
    })
}

/// End words coloured by rhyme group; a verse that breaks its group is marked `broken`.
pub fn semantic_tokens(doc: &Doc) -> SemanticTokens {
    let ix = LineIndex::new(&doc.text);
    let mut found: Vec<(Position, u32, u32, u32)> = doc
        .verses()
        .filter(|(_, _, v)| !v.structural)
        .filter_map(|(_, _, v)| {
            let g = v.group?;
            let sp = v.end_span?;
            let (start, end) = (ix.position(sp.start), ix.position(sp.end));
            if start.line != end.line {
                return None;
            }
            let color = (g as u32 - 'A' as u32) % u32::try_from(GROUP_COLORS).unwrap_or(1);
            let broken = u32::from(v.rhymes == Some(false));
            Some((start, end.character - start.character, color, broken))
        })
        .collect();
    found.sort_by_key(|(p, ..)| (p.line, p.character));
    let (mut line, mut col) = (0, 0);
    let data = found
        .into_iter()
        .map(|(p, length, token_type, modifiers)| {
            let delta_line = p.line - line;
            let delta_start = if delta_line == 0 {
                p.character - col
            } else {
                p.character
            };
            (line, col) = (p.line, p.character);
            SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: modifiers,
            }
        })
        .collect();
    SemanticTokens {
        result_id: None,
        data,
    }
}

/// The whole document replaced by its formatting, when that differs.
pub fn formatting(doc: &Doc) -> Option<Vec<TextEdit>> {
    let formatted = syntaxeon_analysis::format::format(&doc.text, doc.rules).ok()?;
    if formatted == doc.text {
        return Some(Vec::new());
    }
    let ix = LineIndex::new(&doc.text);
    Some(vec![TextEdit {
        range: Range::new(Position::new(0, 0), ix.position(doc.text.len())),
        new_text: formatted,
    }])
}

/// The span of the last word of the verse's end token, which a replacement can overwrite: the
/// whole token when it is that word, or the last word inside a quoted text.
fn replaceable(text: &str, v: &VerseInfo) -> Option<Span> {
    let word = v.end_word.as_deref()?;
    let sp = v.end_span?;
    let token = text.get(sp.start..sp.end)?;
    if token.eq_ignore_ascii_case(word) {
        return Some(sp);
    }
    let inner = token.strip_prefix('"')?.strip_suffix('"')?;
    let lower = inner.to_lowercase();
    let at = lower.rfind(word)?;
    (at + word.len() == lower.len() && inner.is_char_boundary(at)).then(|| Span {
        start: sp.start + 1 + at,
        end: sp.start + 1 + at + word.len(),
    })
}

const PARTNERS: usize = 6;

/// Quick fixes for a verse that fails to rhyme: swap its end word for a common word that does.
pub fn code_actions(doc: &Doc, uri: &Uri, range: Range) -> Vec<CodeActionOrCommand> {
    let ix = LineIndex::new(&doc.text);
    let (lo, hi) = (ix.offset(range.start), ix.offset(range.end));
    let Some(lex) = Lexicon::pinned().ok() else {
        return Vec::new();
    };
    let mut lex = lex;
    lex.set_rhyme_rules(doc.rules);
    let mut out = Vec::new();
    for (s, _, v) in doc.verses() {
        let (Some(sp), Some(false)) = (v.end_span, v.rhymes) else {
            continue;
        };
        if sp.end < lo || sp.start > hi {
            continue;
        }
        let stanza = &doc.report[s];
        let (Some(lead), Some(old), Some(at)) = (
            v.leader.map(|l| &stanza.verses[l]),
            v.end_word.as_deref(),
            replaceable(&doc.text, v),
        ) else {
            continue;
        };
        let Some(lead_word) = lead.end_word.as_deref() else {
            continue;
        };
        let Ok(partners) = rhymes_with(&lex, lead_word) else {
            continue;
        };
        for new in partners
            .into_iter()
            .filter(|w| lex.is_common(w) && *w != old)
            .take(PARTNERS)
        {
            let edit = TextEdit {
                range: ix.range(at),
                new_text: new.to_string(),
            };
            out.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Rhyme with \"{lead_word}\": replace \"{old}\" with \"{new}\""),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), vec![edit])])),
                    ..WorkspaceEdit::default()
                }),
                ..CodeAction::default()
            }));
        }
    }
    out
}

fn verse_json(ix: &LineIndex, v: &VerseInfo) -> Value {
    json!({
        "line": v.line - 1,
        "range": v.span_range(ix),
        "structural": v.structural,
        "endWord": v.end_word,
        "endRange": v.end_span.map(|sp| ix.range(sp)),
        "pronunciations": v.prons,
        "tails": v.tails,
        "group": v.group.map(String::from),
        "leader": v.leader,
        "kind": v.kind.map(RhymeKind::name),
        "rhymes": v.rhymes,
    })
}

trait SpanRange {
    fn span_range(&self, ix: &LineIndex) -> Range;
}

impl SpanRange for VerseInfo {
    fn span_range(&self, ix: &LineIndex) -> Range {
        ix.range(self.span)
    }
}

/// The phonetic report for tools that make sound: one entry per stanza, one per verse.
pub fn verses_json(doc: &Doc) -> Value {
    let ix = LineIndex::new(&doc.text);
    Value::Array(
        doc.report
            .iter()
            .map(|st| {
                json!({
                    "scheme": st.scheme,
                    "valid": st.valid,
                    "verses": st.verses.iter().map(|v| verse_json(&ix, v)).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Two words side by side: how they sound against each other and each one's pronunciations.
pub fn compare_json(rules: RhymeRules, a: &str, b: &str) -> Result<Value, String> {
    let mut lex = Lexicon::pinned().map_err(|e| e.to_string())?;
    lex.set_rhyme_rules(rules);
    let kind = rhyme_kind(&lex, a, b).map_err(|e| format!("{:?} is not in the lexicon", e.0))?;
    let accepted = syntaxeon_lexicon::rhyme::words_rhyme(&lex, a, b).unwrap_or(false);
    let side = |w: &str| {
        let prons = lex.lookup(w).unwrap_or_default();
        json!({
            "word": w,
            "pronunciations": prons.iter().map(|p| {
                p.iter().map(|x| match x.stress {
                    Some(d) => format!("{}{d}", x.base),
                    None => x.base.to_string(),
                }).collect::<Vec<_>>().join(" ")
            }).collect::<Vec<_>>(),
            "tails": prons.iter().filter_map(|p| syntaxeon_lexicon::rhyme::tail_string(p)).collect::<Vec<_>>(),
        })
    };
    Ok(json!({ "kind": kind.name(), "rhymes": accepted, "a": side(a), "b": side(b) }))
}

/// Where the name under the cursor is defined: a function, a parameter or a declaration.
pub fn definition(doc: &Doc, uri: &Uri, at: Position) -> Option<Location> {
    let ix = LineIndex::new(&doc.text);
    let def = doc.index.definition_at(ix.offset(at))?;
    Some(Location {
        uri: uri.clone(),
        range: ix.range(def.span),
    })
}

/// Names the cursor may use, and, on the end word of a verse that must rhyme, words that rhyme.
pub fn completion(doc: &Doc, at: Position) -> Vec<CompletionItem> {
    let ix = LineIndex::new(&doc.text);
    let offset = ix.offset(at);
    let mut items = Vec::new();
    let stanza = doc.index.stanza_at(offset);
    // shortcut: a name declared inside a block is offered to the whole stanza after its
    // declaration, not only inside the block; upgrade when scopes carry their extent.
    for d in &doc.index.defs {
        let visible = match d.kind {
            DefKind::Function => true,
            DefKind::Param | DefKind::Variable => {
                Some(d.stanza) == stanza && (d.kind == DefKind::Param || d.span.end <= offset)
            }
        };
        if visible {
            items.push(CompletionItem {
                label: d.name.clone(),
                kind: Some(match d.kind {
                    DefKind::Function => CompletionItemKind::FUNCTION,
                    DefKind::Param | DefKind::Variable => CompletionItemKind::VARIABLE,
                }),
                detail: Some(
                    match d.kind {
                        DefKind::Function => "function",
                        DefKind::Param => "parameter",
                        DefKind::Variable => "variable",
                    }
                    .to_string(),
                ),
                sort_text: Some(format!("1{}", d.name)),
                ..CompletionItem::default()
            });
        }
    }
    // Rhyming words go first, as their sort text says too.
    let mut out = rhyme_completions(doc, &ix, offset);
    out.extend(items);
    out
}

const RHYME_WORDS: usize = 24;

fn rhyme_completions(doc: &Doc, ix: &LineIndex, offset: usize) -> Vec<CompletionItem> {
    let Some((s, _, v)) = doc
        .verses()
        .find(|(_, _, v)| !v.structural && v.span.start <= offset && offset <= v.span.end)
    else {
        return Vec::new();
    };
    let (Some(lead), Some(old), Some(at)) = (
        v.leader.map(|l| &doc.report[s].verses[l]),
        v.end_word.as_deref(),
        replaceable(&doc.text, v),
    ) else {
        return Vec::new();
    };
    // Only on the end word itself, where the rhyme is decided.
    if offset < at.start || offset > at.end {
        return Vec::new();
    }
    let (Some(lead_word), Ok(mut lex)) = (lead.end_word.as_deref(), Lexicon::pinned()) else {
        return Vec::new();
    };
    lex.set_rhyme_rules(doc.rules);
    let Ok(partners) = rhymes_with(&lex, lead_word) else {
        return Vec::new();
    };
    partners
        .into_iter()
        .filter(|w| lex.is_common(w) && *w != old)
        .take(RHYME_WORDS)
        .enumerate()
        .map(|(rank, w)| CompletionItem {
            label: w.to_string(),
            kind: Some(CompletionItemKind::TEXT),
            detail: Some(format!("rhymes with \"{lead_word}\"")),
            sort_text: Some(format!("0{rank:03}")),
            filter_text: Some(w.to_string()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: ix.range(at),
                new_text: w.to_string(),
            })),
            ..CompletionItem::default()
        })
        .collect()
}

/// Every place the name under the cursor is written.
pub fn references(doc: &Doc, uri: &Uri, at: Position, declaration: bool) -> Vec<Location> {
    let ix = LineIndex::new(&doc.text);
    let Some((def, _)) = doc.index.at(ix.offset(at)) else {
        return Vec::new();
    };
    doc.index
        .mentions(def)
        .into_iter()
        .skip(usize::from(!declaration))
        .map(|sp| Location {
            uri: uri.clone(),
            range: ix.range(sp),
        })
        .collect()
}

pub fn prepare_rename(doc: &Doc, at: Position) -> Option<PrepareRenameResponse> {
    let ix = LineIndex::new(&doc.text);
    let (_, span) = doc.index.at(ix.offset(at))?;
    Some(PrepareRenameResponse::Range(ix.range(span)))
}

/// Renames the name under the cursor everywhere it is written. The renamed program is analysed
/// again and the rename is refused when that finds more errors than before: a reserved word, a
/// name already taken, or an end word that no longer rhymes.
pub fn rename(doc: &Doc, uri: &Uri, at: Position, new_name: &str) -> Result<WorkspaceEdit, String> {
    let new_name = new_name.trim();
    if new_name.is_empty()
        || !new_name
            .chars()
            .all(|c| c.is_alphabetic() || c == ' ' || c == '-' || c == '\'')
    {
        return Err(format!(
            "{new_name:?} is not a name: use words made of letters"
        ));
    }
    let ix = LineIndex::new(&doc.text);
    let (def, _) = doc
        .index
        .at(ix.offset(at))
        .ok_or("there is no name to rename here")?;
    let mut spans = doc.index.mentions(def);
    spans.sort_by_key(|s| std::cmp::Reverse(s.start));
    let mut text = doc.text.clone();
    for sp in &spans {
        text.replace_range(sp.start..sp.end, new_name);
    }
    let renamed = Doc::new(doc.path.as_deref(), text);
    if renamed.diagnostics.len() > doc.diagnostics.len() {
        let first = renamed
            .diagnostics
            .iter()
            .map(|d| format!("{} (line {}): {}", d.code, d.line, d.message))
            .next()
            .unwrap_or_default();
        return Err(format!(
            "renaming to {new_name:?} would break the program: {first}"
        ));
    }
    let edits = spans
        .iter()
        .map(|sp| TextEdit {
            range: ix.range(*sp),
            new_text: new_name.to_string(),
        })
        .collect();
    Ok(WorkspaceEdit {
        changes: Some(HashMap::from([(uri.clone(), edits)])),
        ..WorkspaceEdit::default()
    })
}
