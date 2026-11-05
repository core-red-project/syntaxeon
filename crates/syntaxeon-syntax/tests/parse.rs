#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
use syntaxeon_core::{BinOp, Expr, Program, StmtKind, Target, Type, UnOp};
use syntaxeon_lex::lexer::lex;
use syntaxeon_lex::literary::stanzas;
use syntaxeon_syntax::parse_program;

fn parse(src: &str) -> (Program, Vec<&'static str>) {
    let l = lex(src);
    let st = stanzas(&l);
    let (p, d) = parse_program(&l, &st);
    let codes = l.diagnostics.iter().chain(&d).map(|d| d.code).collect();
    (p, codes)
}

/// Body of a program whose entry stanza contains `lines`; asserts there are no diagnostics.
fn body(lines: &str) -> Vec<StmtKind> {
    let (p, codes) = parse(&format!("Thus begins the tale:\n{lines}\n"));
    assert_eq!(
        codes.len(),
        0,
        "unexpected diagnostics {codes:?} for {lines:?}"
    );
    p.stanzas[0].body.iter().map(|s| s.kind.clone()).collect()
}

fn codes(lines: &str) -> Vec<&'static str> {
    parse(&format!("Thus begins the tale:\n{lines}\n")).1
}

fn v(n: &str) -> Expr {
    Expr::Var(n.to_string())
}
fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}
fn decl(name: &str, ty: Option<Type>, mutable: bool, init: Expr) -> StmtKind {
    StmtKind::Decl {
        name: name.to_string(),
        ty,
        mutable,
        init,
    }
}
fn call(f: &str, args: Vec<Expr>) -> Expr {
    Expr::Call(f.to_string(), args)
}

#[test]
fn worked_example_from_the_grammar() {
    let src = "Thus begins the counting:\n\
        Let the counter, a whole number, become zero.\n\
        Do this five times:\n\
        Let the counter now be the sum of the counter and one.\n\
        Proclaim the text of the counter.\n\
        So be it.\n\n\
        To double, taking the number as whole number, giving whole number:\n\
        Give back the product of the number and two.\n";
    let (p, c) = parse(src);
    assert_eq!(c.len(), 0, "{c:?}");
    assert_eq!(p.stanzas.len(), 2);
    let e = &p.stanzas[0];
    assert_eq!(e.title, "counting");
    assert_eq!(e.ret, Type::Unit);
    assert_eq!(
        e.body[0].kind,
        decl("counter", Some(Type::Integer), true, Expr::Int(0))
    );
    let StmtKind::Repeat { count, body } = &e.body[1].kind else {
        panic!("repeat")
    };
    assert_eq!(*count, Expr::Int(5));
    assert_eq!(
        body[0].kind,
        StmtKind::Assign {
            target: Target::Name("counter".into()),
            value: bin(BinOp::Add, v("counter"), Expr::Int(1)),
        }
    );
    assert_eq!(
        body[1].kind,
        StmtKind::Expr(call("speakln", vec![call("to_string", vec![v("counter")])]))
    );
    let f = &p.stanzas[1];
    assert_eq!(f.title, "double");
    assert_eq!(f.params[0].name, "number");
    assert_eq!(f.params[0].ty, Type::Integer);
    assert_eq!(f.ret, Type::Integer);
    assert_eq!(
        f.body[0].kind,
        StmtKind::Return(Some(bin(BinOp::Mul, v("number"), Expr::Int(2))))
    );
}

#[test]
fn declaration_surface_forms_are_one_ast() {
    let want = vec![decl("counter", Some(Type::Integer), true, Expr::Int(1))];
    for s in [
        "The counter, a whole number, shall become one.",
        "Let the counter, a whole number, become one.",
        "One shall the counter, a whole number, become.",
    ] {
        assert_eq!(body(s), want, "{s}");
    }
    assert_eq!(
        body("Let the total be 7."),
        vec![decl("total", None, false, Expr::Int(7))]
    );
    assert_eq!(
        body("The total shall be seven."),
        vec![decl("total", None, false, Expr::Int(7))]
    );
}

#[test]
fn assignment_surface_forms_are_one_ast() {
    let want = vec![StmtKind::Assign {
        target: Target::Name("counter".into()),
        value: Expr::Int(2),
    }];
    for s in [
        "The counter shall now be two.",
        "Let the counter now be two.",
        "Two shall the counter now be.",
    ] {
        assert_eq!(body(s), want, "{s}");
    }
    assert_eq!(
        body("Let item 0 of the xs now be 9."),
        vec![StmtKind::Assign {
            target: Target::Index("xs".into(), Expr::Int(0)),
            value: Expr::Int(9),
        }]
    );
}

#[test]
fn arity_nesting_needs_no_precedence() {
    assert_eq!(
        body("Speak the sum of the product of 2 and 3 and 4.")[0],
        StmtKind::Expr(call(
            "speak",
            vec![bin(
                BinOp::Add,
                bin(BinOp::Mul, Expr::Int(2), Expr::Int(3)),
                Expr::Int(4)
            )]
        ))
    );
    assert_eq!(
        body("Let x be 2 plus 3.")[0],
        decl(
            "x",
            None,
            false,
            bin(BinOp::Add, Expr::Int(2), Expr::Int(3))
        )
    );
}

#[test]
fn chained_infix_is_106() {
    assert_eq!(codes("Let x be 1 plus 2 plus 3."), ["SXP-106"]);
    assert_eq!(codes("Let x be 1 plus 2 times 3."), ["SXP-106"]);
}

#[test]
fn comparisons_and_logic() {
    let b = body("Let ok be both the x is at least 3 and not the y.");
    assert_eq!(
        b[0],
        decl(
            "ok",
            None,
            false,
            bin(
                BinOp::And,
                bin(BinOp::Ge, v("x"), Expr::Int(3)),
                Expr::Unary(UnOp::Not, Box::new(v("y")))
            )
        )
    );
    // `is not` is Ne, never Eq(a, Not b).
    assert_eq!(
        body("Let d be the p is not the q.")[0],
        decl("d", None, false, bin(BinOp::Ne, v("p"), v("q")))
    );
}

#[test]
fn control_flow_nests() {
    let b = body(
        "As long as the n is greater than 0, do:\n\
         If the n is 5, then:\n\
         Speak \"five\".\n\
         Otherwise:\n\
         Speak \"other\".\n\
         So be it.\n\
         Let the n now be the difference of the n and 1.\n\
         So be it.",
    );
    let StmtKind::While { body, .. } = &b[0] else {
        panic!("while")
    };
    assert_eq!(body.len(), 2);
    let StmtKind::If {
        then, otherwise, ..
    } = &body[0].kind
    else {
        panic!("if")
    };
    assert_eq!(then.len(), 1);
    assert_eq!(otherwise.as_ref().map(Vec::len), Some(1));
}

#[test]
fn block_errors_are_107() {
    assert_eq!(codes("So be it."), ["SXP-107"]);
    assert_eq!(codes("Otherwise:"), ["SXP-107"]);
    assert_eq!(codes("If the x is 1, then:\nSpeak \"a\"."), ["SXP-107"]);
    assert_eq!(
        codes("If the x is 1, then:\nOtherwise:\nOtherwise:\nSo be it."),
        ["SXP-107"]
    );
}

#[test]
fn a_comma_goes_to_the_innermost_open_call_or_list() {
    // `g` takes both arguments; `f` takes one.
    assert_eq!(
        body("Let z be calling f with calling g with 1, 2.")[0],
        decl(
            "z",
            None,
            false,
            call("f", vec![call("g", vec![Expr::Int(1), Expr::Int(2)])])
        )
    );
    // A list literal in an argument list takes the rest of the list.
    assert_eq!(
        body("Call f with a list of 1, 2.")[0],
        StmtKind::Expr(call(
            "f",
            vec![Expr::List(vec![Expr::Int(1), Expr::Int(2)])]
        ))
    );
}

#[test]
fn an_open_construct_swallows_a_following_infix_word() {
    // gcd(71 plus 12), not (gcd 71) plus 12.
    assert_eq!(
        body("Give back calling gcd with 71 plus 12.")[0],
        StmtKind::Return(Some(call(
            "gcd",
            vec![bin(BinOp::Add, Expr::Int(71), Expr::Int(12))]
        )))
    );
    // `calling f with x with y appended` is f(x with y appended).
    assert_eq!(
        body("Let z be calling f with x with y appended.")[0],
        decl(
            "z",
            None,
            false,
            call("f", vec![call("append", vec![v("x"), v("y")])])
        )
    );
}

#[test]
fn prefix_forms_hold_plain_operands() {
    // The infix word ends the prefix form: (the sum of 1 and 2) plus 3.
    assert_eq!(
        body("Speak the sum of 1 and 2 plus 3.")[0],
        StmtKind::Expr(call(
            "speak",
            vec![bin(
                BinOp::Add,
                bin(BinOp::Add, Expr::Int(1), Expr::Int(2)),
                Expr::Int(3)
            )]
        ))
    );
    // An infix word inside one is refused with the infix code, not guessed.
    assert_eq!(codes("Speak the sum of 1 plus 2 and 3."), ["SXP-106"]);
}

#[test]
fn variable_names_cannot_contain_of() {
    assert_eq!(codes("Let the tale of men be 1."), ["SXP-103"]);
    // `item … of …` therefore reads one way only.
    assert_eq!(
        body("Let x be item i of xs.")[0],
        decl(
            "x",
            None,
            false,
            Expr::Index(Box::new(v("xs")), Box::new(v("i")))
        )
    );
}

#[test]
fn no_rule_is_103() {
    assert_eq!(codes("Dance wildly."), ["SXP-103"]);
    assert_eq!(codes("Let x be."), ["SXP-103"]);
    assert_eq!(codes("the sum of 1 and 2."), ["SXP-103"]);
    assert_eq!(codes("Speak \"unterminated sentence\""), ["SXP-103"]);
}

#[test]
fn headers() {
    let (_, c) = parse("To greet:\nSpeak \"hi\".\n");
    assert_eq!(c, ["SXP-109"], "first stanza must be the entry");
    let (_, c) = parse("Thus begins alpha:\nSpeak \"a\".\n\nThus begins beta:\nSpeak \"b\".\n");
    assert_eq!(c, ["SXP-109"], "'Thus begins' only on the first stanza");
    let (p, c) = parse(
        "Thus begins alpha:\nSpeak \"a\".\n\n\
         To add, taking x as whole number, y as whole number, and z as fraction, giving fraction:\nGive back z.\n",
    );
    assert_eq!(c.len(), 0, "{c:?}");
    assert_eq!(p.stanzas[1].params.len(), 3);
    assert_eq!(p.stanzas[1].params[2].ty, Type::Float);
    let (p, _) = parse(
        "Thus begins alpha:\nReturn nothing.\n\nTo f, taking xs as list of list of text:\nReturn nothing.\n",
    );
    assert_eq!(
        p.stanzas[1].params[0].ty,
        Type::List(Box::new(Type::List(Box::new(Type::String))))
    );
}

#[test]
fn empty_list_needs_an_annotated_decl() {
    assert_eq!(
        body("Let xs, a list of whole number, be an empty list of whole number.")[0],
        decl(
            "xs",
            Some(Type::List(Box::new(Type::Integer))),
            false,
            Expr::List(vec![])
        )
    );
    assert_eq!(
        codes("Let xs be an empty list of whole number."),
        ["SXP-108"]
    );
    assert_eq!(
        codes("Speak the length of an empty list of text."),
        ["SXP-108"]
    );
}

#[test]
fn literals_and_names() {
    assert_eq!(
        body("Let n be twenty-one.")[0],
        decl("n", None, false, Expr::Int(21))
    );
    assert_eq!(
        body("Let f be 3.5.")[0],
        decl("f", None, false, Expr::Float(3.5))
    );
    assert_eq!(
        body("Let c be the letter \"x\".")[0],
        decl("c", None, false, Expr::Char('x'))
    );
    assert_eq!(
        body("Let \"the end\" be true.")[0],
        decl("the end", None, false, Expr::Bool(true))
    );
    assert_eq!(
        body("Let line be what is heard.")[0],
        decl("line", None, false, call("hear", vec![]))
    );
    // Determiner and case are dropped: the same name.
    assert_eq!(
        body("Let This Counter be 1.")[0],
        decl("counter", None, false, Expr::Int(1))
    );
}

#[test]
fn lists_calls_and_append() {
    assert_eq!(
        body("Let xs be a list of one, two, and three.")[0],
        decl(
            "xs",
            None,
            false,
            Expr::List(vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)])
        )
    );
    assert_eq!(
        body("Call greet with \"a\", 2.")[0],
        StmtKind::Expr(call("greet", vec![Expr::Str("a".into()), Expr::Int(2)]))
    );
    assert_eq!(
        body("Let ys be the xs with 4 appended.")[0],
        decl(
            "ys",
            None,
            false,
            call("append", vec![v("xs"), Expr::Int(4)])
        )
    );
    assert_eq!(
        body("Let n be the length of the xs.")[0],
        decl("n", None, false, call("length", vec![v("xs")]))
    );
}

#[test]
fn return_forms() {
    assert_eq!(body("Give back nothing.")[0], StmtKind::Return(None));
    assert_eq!(body("Return 3.")[0], StmtKind::Return(Some(Expr::Int(3))));
}
