//! Lexical layer (`spec/PROSE_GRAMMAR.md` §2).
//!
//! shortcut: input is assumed already NFC-normalized and lowercasing uses
//! `str::to_lowercase`, not Unicode simple folding; revisit when non-ASCII text matters.

use crate::{Diagnostic, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// Folded to lowercase.
    Word(String),
    Int(i64),
    Float {
        value: f64,
        text: String,
    },
    /// Verbatim contents, escapes resolved.
    Str(String),
    /// One of `, ; : . ! ?`.
    Punct(char),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    /// 1-based line of the token's first character.
    pub line: usize,
}

/// `(pronounced "word" as "PH PH")` — see `spec/PROSODY.md` §3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pragma {
    pub word: String,
    pub phones: String,
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub pragmas: Vec<Pragma>,
    /// One entry per physical line: true iff the line is blank and outside any aside.
    pub blank_lines: Vec<bool>,
    pub diagnostics: Vec<Diagnostic>,
}

#[must_use]
pub fn lex(src: &str) -> Lexed {
    let mut lx = Lexer {
        src,
        chars: src.char_indices().collect(),
        i: 0,
        line: 1,
        open_at_line_start: false,
        has_content: false,
        out: Lexed::default(),
    };
    lx.run();
    lx.newline(0); // finalize the last line
    lx.out
}

struct Lexer<'a> {
    src: &'a str,
    chars: Vec<(usize, char)>,
    i: usize,
    line: usize,
    open_at_line_start: bool,
    has_content: bool,
    out: Lexed,
}

impl Lexer<'_> {
    fn pos_at(&self, i: usize) -> usize {
        self.chars.get(i).map_or(self.src.len(), |x| x.0)
    }

    fn peek(&self, off: usize) -> Option<char> {
        self.chars.get(self.i + off).map(|x| x.1)
    }

    fn err(&mut self, code: &'static str, message: &str) {
        self.out.diagnostics.push(Diagnostic {
            code,
            message: message.to_string(),
            line: self.line,
            span: None,
        });
    }

    fn push(&mut self, tok: Tok, start: usize) {
        let span = Span {
            start,
            end: self.pos_at(self.i),
        };
        self.out.tokens.push(Token {
            tok,
            span,
            line: self.line,
        });
    }

    /// Ends the current line; `depth` is the aside depth the next line starts in.
    fn newline(&mut self, depth: usize) {
        let blank = !self.open_at_line_start && !self.has_content;
        self.out.blank_lines.push(blank);
        self.line += 1;
        self.open_at_line_start = depth > 0;
        self.has_content = false;
    }

    fn run(&mut self) {
        while let Some(c) = self.peek(0) {
            let pos = self.pos_at(self.i);
            if !c.is_whitespace() {
                self.has_content = true;
            }
            match c {
                '\n' => {
                    self.newline(0);
                    self.i += 1;
                }
                '\r' | ' ' => self.i += 1,
                '\t' => {
                    self.has_content = true;
                    self.err("SXP-101", "tab character");
                    self.i += 1;
                }
                '(' => self.aside(),
                '"' => self.string(pos),
                '0'..='9' => self.number(pos),
                c if c.is_alphabetic() => self.word(pos),
                ',' | ';' | ':' | '.' | '!' | '?' => {
                    self.i += 1;
                    self.push(Tok::Punct(c), pos);
                }
                '_' => {
                    self.err(
                        "SXP-101",
                        "unknown token '_': names are words made of letters, hyphens and apostrophes; write two words instead",
                    );
                    self.i += 1;
                }
                _ => {
                    self.err("SXP-101", &format!("unknown token {c:?}"));
                    self.i += 1;
                }
            }
        }
    }

    fn word(&mut self, start: usize) {
        while let Some(c) = self.peek(0) {
            let joiner = (c == '\'' || c == '-') && self.peek(1).is_some_and(char::is_alphabetic);
            if c.is_alphabetic() || joiner {
                self.i += 1;
            } else {
                break;
            }
        }
        let text = self.src[start..self.pos_at(self.i)].to_lowercase();
        self.push(Tok::Word(text), start);
    }

    fn number(&mut self, start: usize) {
        while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        let mut is_float = false;
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.i += 1;
            while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if self.peek(0).is_some_and(char::is_alphanumeric) {
            while self.peek(0).is_some_and(char::is_alphanumeric) {
                self.i += 1;
            }
            self.err("SXP-102", "invalid number literal");
            return;
        }
        let text = self.src[start..self.pos_at(self.i)].to_string();
        if is_float {
            match text.parse::<f64>() {
                Ok(value) => self.push(Tok::Float { value, text }, start),
                Err(_) => self.err("SXP-102", "invalid float literal"),
            }
        } else {
            match text.parse::<i64>() {
                Ok(n) => self.push(Tok::Int(n), start),
                Err(_) => self.err("SXP-102", "integer literal out of range"),
            }
        }
    }

    fn string(&mut self, start: usize) {
        self.i += 1;
        let mut s = String::new();
        loop {
            match self.peek(0) {
                None | Some('\n') => {
                    self.err("SXP-101", "unterminated string");
                    return;
                }
                Some('"') => {
                    self.i += 1;
                    self.push(Tok::Str(s), start);
                    return;
                }
                Some('\\') => {
                    let esc = self.peek(1);
                    self.i += if esc.is_some() { 2 } else { 1 };
                    match esc {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some('n') => s.push('\n'),
                        _ => self.err("SXP-101", "invalid escape in string"),
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.i += 1;
                }
            }
        }
    }

    /// shortcut: parentheses inside a pragma's quoted strings are not supported;
    /// asides count raw `(`/`)` regardless of quotes.
    fn aside(&mut self) {
        let start_line = self.line;
        self.i += 1;
        let mut depth = 1;
        let mut inner = String::new();
        while depth > 0 {
            let Some(c) = self.peek(0) else {
                self.out.diagnostics.push(Diagnostic {
                    code: "SXP-101",
                    message: "unterminated aside".to_string(),
                    line: start_line,
                    span: None,
                });
                return;
            };
            self.i += 1;
            if !c.is_whitespace() {
                self.has_content = true;
            }
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if c == '\n' {
                self.newline(depth);
            }
            if depth > 0 {
                inner.push(c);
            }
        }
        match parse_pragma(&inner) {
            Aside::Comment => {}
            Aside::Pragma(word, phones) => {
                self.out.pragmas.push(Pragma {
                    word,
                    phones,
                    line: start_line,
                });
            }
            Aside::BadPragma => self.out.diagnostics.push(Diagnostic {
                code: "SXP-302",
                message: "malformed pronunciation pragma".to_string(),
                line: start_line,
                span: None,
            }),
        }
    }
}

fn take_quoted(s: &str) -> Option<(String, &str)> {
    let rest = s.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

enum Aside {
    Comment,
    Pragma(String, String),
    BadPragma,
}

fn parse_pragma(inner: &str) -> Aside {
    let t = inner.trim_start();
    let Some(first) = t.split_whitespace().next() else {
        return Aside::Comment;
    };
    if !first.eq_ignore_ascii_case("pronounced") {
        return Aside::Comment;
    }
    let parsed = (|| {
        let (word, rest) = take_quoted(&t[first.len()..])?;
        let rest = rest.trim_start();
        let rest = rest
            .get(..2)
            .filter(|a| a.eq_ignore_ascii_case("as"))
            .map(|_| &rest[2..])?;
        let (phones, rest) = take_quoted(rest)?;
        rest.trim().is_empty().then_some((word, phones))
    })();
    parsed.map_or(Aside::BadPragma, |(w, p)| Aside::Pragma(w, p))
}
