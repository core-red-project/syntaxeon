//! The normalizer's closed rule tables (ADR-005, `spec/PROSE_GRAMMAR.md`).
//!
//! Every verb set, connective and surface-to-AST mapping lives here as data; `grammar.rs` holds
//! only the generic readers that interpret them. Adding a synonym is a one-line change here.

use syntaxeon_core::{BinOp, Type, UnOp};

/// How a block header lowers (`spec/PROSE_GRAMMAR.md` §8).
#[derive(Clone, Copy)]
pub(crate) enum HeaderKind {
    If,
    While,
    Repeat,
}

/// `lead E close`: words before the condition, and the words after it (a comma-led tail
/// such as `, then`, or a bare `times`).
pub(crate) struct Header {
    pub lead: &'static [&'static str],
    pub close: &'static [&'static str],
    pub kind: HeaderKind,
}

pub(crate) const HEADERS: &[Header] = &[
    Header {
        lead: &["if"],
        close: &[",", "then"],
        kind: HeaderKind::If,
    },
    Header {
        lead: &["as", "long", "as"],
        close: &[",", "do"],
        kind: HeaderKind::While,
    },
    Header {
        lead: &["do", "this"],
        close: &["times"],
        kind: HeaderKind::Repeat,
    },
];

/// One element of a declaration or assignment sentence (`spec/PROSE_GRAMMAR.md` §6.1–§6.2).
#[derive(Clone, Copy)]
pub(crate) enum Step {
    /// Fixed words.
    Words(&'static [&'static str]),
    /// A declared name (possibly quoted).
    Name,
    /// An assignment target: a name or `item i of name`.
    Target,
    /// Optional apposition `, a T,`.
    Annot,
    /// `be` (immutable) or `become` (mutable), ADR-012.
    Verb,
    Expr,
}

#[derive(Clone, Copy)]
pub(crate) enum Make {
    Decl,
    Assign,
    /// `Return E`.
    Return,
    /// `Return nothing`.
    Nothing,
}

/// A sentence shape: the steps must cover the whole sentence. Word order is free to differ
/// between forms; all forms with the same `make` lower to the same AST node.
pub(crate) struct Form {
    pub steps: &'static [Step],
    pub make: Make,
}

use Step::{Annot, Expr, Name, Target, Verb, Words};

pub(crate) const FORMS: &[Form] = &[
    // D1: N [", a T,"] shall V E
    Form {
        steps: &[Name, Annot, Words(&["shall"]), Verb, Expr],
        make: Make::Decl,
    },
    // D2: Let N [", a T,"] V E
    Form {
        steps: &[Words(&["let"]), Name, Annot, Verb, Expr],
        make: Make::Decl,
    },
    // D3: E shall N [", a T,"] V
    Form {
        steps: &[Expr, Words(&["shall"]), Name, Annot, Verb],
        make: Make::Decl,
    },
    // A1: X shall now be E
    Form {
        steps: &[Target, Words(&["shall", "now", "be"]), Expr],
        make: Make::Assign,
    },
    // A2: Let X now be E
    Form {
        steps: &[Words(&["let"]), Target, Words(&["now", "be"]), Expr],
        make: Make::Assign,
    },
    // A3: E shall X now be
    Form {
        steps: &[Expr, Words(&["shall"]), Target, Words(&["now", "be"])],
        make: Make::Assign,
    },
    // R1–R2: Give back E / Return E; R3–R4: the same with `nothing` (`PROSE_GRAMMAR.md` §6.5).
    Form {
        steps: &[Words(&["give", "back"]), Expr],
        make: Make::Return,
    },
    Form {
        steps: &[Words(&["return"]), Expr],
        make: Make::Return,
    },
    Form {
        steps: &[Words(&["give", "back", "nothing"])],
        make: Make::Nothing,
    },
    Form {
        steps: &[Words(&["return", "nothing"])],
        make: Make::Nothing,
    },
];

/// Output verbs and the builtin each lowers to.
pub(crate) const SPEAK: &[(&str, &str)] = &[("speak", "speak"), ("proclaim", "speakln")];

/// Type phrases (`spec/PROSE_GRAMMAR.md` §4); `list of T` is built by the reader.
pub(crate) const SIMPLE_TYPES: [(&[&str], Type); 5] = [
    (&["whole", "number"], Type::Integer),
    (&["fraction"], Type::Float),
    (&["truth"], Type::Boolean),
    (&["text"], Type::String),
    (&["letter"], Type::Character),
];

/// `the sum of A and B`.
pub(crate) const PREFIX_ARITH: [(&[&str], BinOp); 5] = [
    (&["the", "sum", "of"], BinOp::Add),
    (&["the", "difference", "of"], BinOp::Sub),
    (&["the", "product", "of"], BinOp::Mul),
    (&["the", "quotient", "of"], BinOp::Div),
    (&["the", "remainder", "of"], BinOp::Mod),
];

/// `the text of X`: lead words and the builtin they call.
pub(crate) const PREFIX_CALL: [(&[&str], &str); 4] = [
    (&["the", "text", "of"], "to_string"),
    (&["the", "whole", "number", "of"], "to_integer"),
    (&["the", "fraction", "of"], "to_float"),
    (&["the", "length", "of"], "length"),
];

pub(crate) const PREFIX_UNARY: [(&[&str], UnOp); 2] = [
    (&["the", "negation", "of"], UnOp::Neg),
    (&["not"], UnOp::Not),
];

/// `A plus B`.
pub(crate) const INFIX: [(&str, BinOp); 5] = [
    ("plus", BinOp::Add),
    ("minus", BinOp::Sub),
    ("times", BinOp::Mul),
    ("over", BinOp::Div),
    ("modulo", BinOp::Mod),
];

/// `A is less than B`.
pub(crate) const COMPARE: [(&[&str], BinOp); 6] = [
    (&["is", "not"], BinOp::Ne),
    (&["is", "less", "than"], BinOp::Lt),
    (&["is", "greater", "than"], BinOp::Gt),
    (&["is", "at", "most"], BinOp::Le),
    (&["is", "at", "least"], BinOp::Ge),
    (&["is"], BinOp::Eq),
];

/// `both A and B`, `either A or B`: opener, separator, operator.
pub(crate) const JOIN: [(&str, &str, BinOp); 2] =
    [("both", "and", BinOp::And), ("either", "or", BinOp::Or)];

pub(crate) const DETS: [&str; 8] = ["the", "this", "that", "a", "an", "my", "thy", "our"];

/// Words that can never appear inside a `Name` (`spec/PROSE_GRAMMAR.md` §5).
pub(crate) const RESERVED: &[&str] = &[
    "the",
    "this",
    "that",
    "a",
    "an",
    "my",
    "thy",
    "our",
    "shall",
    "be",
    "become",
    "now",
    "let",
    "speak",
    "proclaim",
    "give",
    "return",
    "call",
    "calling",
    "if",
    "then",
    "otherwise",
    "as",
    "do",
    "times",
    "so",
    "it",
    "true",
    "false",
    "both",
    "and",
    "either",
    "or",
    "not",
    "is",
    "less",
    "greater",
    "than",
    "at",
    "least",
    "most",
    "plus",
    "minus",
    "over",
    "modulo",
    "item",
    "taking",
    "giving",
    "thus",
    "what",
    "heard",
    "appended",
    "with",
    "nothing",
];

/// After `the`, a name starting with one of these would collide with a prefix form.
pub(crate) const THE_HEADS: &[&str] = &[
    "sum",
    "difference",
    "product",
    "quotient",
    "remainder",
    "text",
    "whole",
    "fraction",
    "length",
    "negation",
    "letter",
];

/// Builtins produced by conversions; their results are not `with … appended` bases.
pub(crate) const CONVERSIONS: &[&str] = &["to_string", "to_integer", "to_float", "length", "hear"];

pub(crate) const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
pub(crate) const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

#[allow(clippy::cast_possible_wrap)] // values are at most 99
pub(crate) fn number_word(w: &str) -> Option<i64> {
    if let Some(n) = ONES.iter().position(|x| *x == w) {
        return Some(n as i64);
    }
    let tens = |s: &str| TENS.iter().position(|x| !x.is_empty() && *x == s);
    if let Some(t) = tens(w) {
        return Some(t as i64 * 10);
    }
    let (t, o) = w.split_once('-')?;
    let ones = ONES
        .iter()
        .position(|x| *x == o)
        .filter(|n| (1..=9).contains(n))?;
    Some((tens(t)? * 10 + ones) as i64)
}
