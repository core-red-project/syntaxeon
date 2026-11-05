//! Static semantics. Normative reference: `spec/SEMANTIC_CORE.md` §5 and §9.
//!
//! After an error the offending expression has no type (`None`) and silently poisons its
//! users, so one mistake reports once instead of cascading.
//!
//! shortcut: diagnostics carry the statement's line only; expression spans do not exist yet.

use std::collections::HashMap;

use syntaxeon_core::{BinOp, Expr, Program, Stanza, Stmt, StmtKind, Target, Type, UnOp};
use syntaxeon_lex::Diagnostic;

const BUILTINS: [&str; 8] = [
    "speak",
    "speakln",
    "hear",
    "length",
    "append",
    "to_string",
    "to_integer",
    "to_float",
];

/// A binding's type (`None` once poisoned by an earlier error) and mutability.
type Binding = (Option<Type>, bool);

struct Cx<'a> {
    sigs: HashMap<&'a str, (Vec<Type>, Type)>,
    scopes: Vec<HashMap<String, Binding>>,
    diags: Vec<Diagnostic>,
    line: usize,
    ret: Type,
    is_entry: bool,
}

/// Checks a whole program and returns every static error found.
#[must_use]
pub fn check(program: &Program) -> Vec<Diagnostic> {
    let mut cx = Cx {
        sigs: HashMap::new(),
        scopes: Vec::new(),
        diags: Vec::new(),
        line: 0,
        ret: Type::Unit,
        is_entry: false,
    };
    if program.stanzas.is_empty() {
        cx.err(
            "SXP-408",
            "the program has no stanzas, hence no entry point",
        );
        return cx.diags;
    }
    for s in &program.stanzas {
        cx.line = s.line;
        if BUILTINS.contains(&s.title.as_str()) {
            cx.err(
                "SXP-407",
                &format!("{:?} is a builtin and cannot be redefined", s.title),
            );
        } else if cx.sigs.contains_key(s.title.as_str()) {
            cx.err(
                "SXP-409",
                &format!("stanza name {:?} is defined twice", s.title),
            );
        } else {
            let params = s.params.iter().map(|p| p.ty.clone()).collect();
            cx.sigs.insert(&s.title, (params, s.ret.clone()));
        }
    }
    for s in &program.stanzas {
        cx.stanza(s);
    }
    cx.diags
}

fn numeric(t: &Type) -> bool {
    matches!(t, Type::Integer | Type::Float)
}

fn int_float_mix(a: &Type, b: &Type) -> bool {
    numeric(a) && numeric(b) && a != b
}

/// Does every path through `block` end in a `Return`? Loops may run zero times.
fn always_returns(block: &[Stmt]) -> bool {
    block.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If {
            then,
            otherwise: Some(other),
            ..
        } => always_returns(then) && always_returns(other),
        _ => false,
    })
}

impl Cx<'_> {
    fn err(&mut self, code: &'static str, message: &str) {
        self.diags.push(Diagnostic {
            code,
            message: message.to_string(),
            line: self.line,
        });
    }

    fn mismatch(&mut self, what: &str, want: &Type, got: &Type) {
        if int_float_mix(want, got) {
            self.err(
                "SXP-406",
                &format!("{what}: {got:?} where {want:?} is required; convert explicitly"),
            );
        } else {
            self.err(
                "SXP-402",
                &format!("{what}: expected {want:?}, found {got:?}"),
            );
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn stanza(&mut self, s: &Stanza) {
        self.line = s.line;
        self.ret = s.ret.clone();
        self.is_entry = s.id == 0;
        let mut scope: HashMap<String, Binding> = HashMap::new();
        for p in &s.params {
            if scope
                .insert(p.name.clone(), (Some(p.ty.clone()), false))
                .is_some()
            {
                self.err(
                    "SXP-401",
                    &format!("parameter {:?} is declared twice", p.name),
                );
            }
        }
        // Parameters and the top level of the body share one scope.
        self.scopes = vec![scope];
        for st in &s.body {
            self.stmt(st);
        }
        self.scopes.clear();
        if s.ret != Type::Unit && !always_returns(&s.body) {
            self.line = s.line;
            self.err(
                "SXP-404",
                &format!("{:?} does not return a value on every path", s.title),
            );
        }
    }

    fn block(&mut self, stmts: &[Stmt]) {
        self.scopes.push(HashMap::new());
        for s in stmts {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    /// Checks `e` against `want`, reporting a mismatch.
    fn expect(&mut self, e: &Expr, want: &Type, what: &str) {
        if let Some(got) = self.infer(e, Some(want))
            && got != *want
        {
            self.mismatch(what, want, &got);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        self.line = s.line;
        match &s.kind {
            StmtKind::Decl {
                name,
                ty,
                init,
                mutable,
            } => self.decl(name, ty.as_ref(), init, *mutable),
            StmtKind::Assign { target, value } => self.assign(target, value),
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expect(cond, &Type::Boolean, "condition");
                self.block(then);
                if let Some(o) = otherwise {
                    self.block(o);
                }
            }
            StmtKind::While { cond, body } => {
                self.expect(cond, &Type::Boolean, "condition");
                self.block(body);
            }
            StmtKind::Repeat { count, body } => {
                self.expect(count, &Type::Integer, "repeat count");
                self.block(body);
            }
            StmtKind::Return(value) => self.ret_stmt(value.as_ref()),
            StmtKind::Expr(e) => {
                self.infer(e, None);
            }
        }
    }

    fn ret_stmt(&mut self, value: Option<&Expr>) {
        let ret = self.ret.clone();
        match value {
            Some(e) if self.is_entry => {
                self.infer(e, None);
                self.err("SXP-405", "the entry stanza cannot return a value");
            }
            Some(e) if ret == Type::Unit => {
                self.infer(e, None);
                self.err(
                    "SXP-402",
                    "this stanza declares no result, so it cannot return a value",
                );
            }
            Some(e) => self.expect(e, &ret, "returned value"),
            None if ret != Type::Unit => {
                self.err("SXP-402", &format!("a {ret:?} result is required here"));
            }
            None => {}
        }
    }

    fn decl(&mut self, name: &str, ty: Option<&Type>, init: &Expr, mutable: bool) {
        let got = self.infer(init, ty);
        let bound = match (ty, got) {
            (Some(want), Some(got)) => {
                if *want != got {
                    self.mismatch(&format!("declaration of {name:?}"), want, &got);
                }
                Some(want.clone())
            }
            (Some(want), None) => Some(want.clone()),
            (None, Some(Type::Unit)) => {
                self.err(
                    "SXP-402",
                    &format!("{name:?} cannot be bound to a value-less call"),
                );
                None
            }
            (None, got) => got,
        };
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if scope.contains_key(name) {
            self.err(
                "SXP-401",
                &format!("{name:?} is already declared in this block"),
            );
        } else {
            scope.insert(name.to_string(), (bound, mutable));
        }
    }

    fn assign(&mut self, target: &Target, value: &Expr) {
        let (name, idx) = match target {
            Target::Name(n) => (n, None),
            Target::Index(n, i) => (n, Some(i)),
        };
        let Some((ty, mutable)) = self.lookup(name).cloned() else {
            self.err("SXP-411", &format!("unknown name {name:?}"));
            self.infer(value, None);
            return;
        };
        if !mutable {
            self.err(
                "SXP-403",
                &format!("{name:?} is not mutable; declare it with 'become'"),
            );
        }
        let want = match (idx, ty) {
            (None, t) => t,
            (Some(i), Some(Type::List(elem))) => {
                self.expect(i, &Type::Integer, "index");
                Some(*elem)
            }
            (Some(i), t) => {
                self.infer(i, None);
                if let Some(t) = t {
                    self.err(
                        "SXP-402",
                        &format!("only a list element can be assigned, not into {t:?}"),
                    );
                }
                None
            }
        };
        match want {
            Some(w) => self.expect(value, &w, "assignment"),
            None => {
                self.infer(value, None);
            }
        }
    }

    fn infer(&mut self, e: &Expr, want: Option<&Type>) -> Option<Type> {
        match e {
            Expr::Int(_) => Some(Type::Integer),
            Expr::Float(_) => Some(Type::Float),
            Expr::Bool(_) => Some(Type::Boolean),
            Expr::Str(_) => Some(Type::String),
            Expr::Char(_) => Some(Type::Character),
            Expr::EmptyList(t) => Some(Type::List(Box::new(t.clone()))),
            Expr::Var(n) => {
                if let Some((t, _)) = self.lookup(n) {
                    t.clone()
                } else {
                    self.err("SXP-411", &format!("unknown name {n:?}"));
                    None
                }
            }
            Expr::Unary(op, a) => self.unary(*op, a),
            Expr::Binary(op, a, b) => self.binary(*op, a, b),
            Expr::List(xs) => self.list(xs, want),
            Expr::Index(l, i) => {
                let tl = self.infer(l, None);
                self.expect(i, &Type::Integer, "index");
                match tl? {
                    Type::List(t) => Some(*t),
                    Type::String => Some(Type::Character),
                    other => {
                        self.err("SXP-402", &format!("cannot index into {other:?}"));
                        None
                    }
                }
            }
            Expr::Call(n, args) => self.call(n, args),
        }
    }

    fn unary(&mut self, op: UnOp, a: &Expr) -> Option<Type> {
        let t = self.infer(a, None)?;
        match (op, &t) {
            (UnOp::Neg, t) if numeric(t) => Some(t.clone()),
            (UnOp::Not, Type::Boolean) => Some(Type::Boolean),
            _ => {
                self.err("SXP-402", &format!("{op:?} cannot be applied to {t:?}"));
                None
            }
        }
    }

    fn binary(&mut self, op: BinOp, a: &Expr, b: &Expr) -> Option<Type> {
        let ta = self.infer(a, None);
        let tb = self.infer(b, None);
        let (ta, tb) = (ta?, tb?);
        if ta != tb {
            self.mismatch(&format!("{op:?}"), &ta, &tb);
            return None;
        }
        let ok = match op {
            BinOp::Add => numeric(&ta) || ta == Type::String,
            BinOp::Sub | BinOp::Mul | BinOp::Div => numeric(&ta),
            BinOp::Mod => ta == Type::Integer,
            BinOp::Eq | BinOp::Ne => true,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                numeric(&ta) || matches!(ta, Type::Character | Type::String)
            }
            BinOp::And | BinOp::Or => ta == Type::Boolean,
        };
        if !ok {
            self.err("SXP-402", &format!("{op:?} cannot be applied to {ta:?}"));
            return None;
        }
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => Some(ta),
            _ => Some(Type::Boolean),
        }
    }

    fn list(&mut self, xs: &[Expr], want: Option<&Type>) -> Option<Type> {
        let hint = match want {
            Some(Type::List(t)) => Some(&**t),
            _ => None,
        };
        let Some((first, rest)) = xs.split_first() else {
            return if let Some(t) = hint {
                Some(Type::List(Box::new(t.clone())))
            } else {
                self.err("SXP-410", "cannot infer the element type of an empty list");
                None
            };
        };
        let elem = self.infer(first, hint);
        for x in rest {
            match &elem {
                Some(t) => self.expect(x, t, "list element"),
                None => {
                    self.infer(x, None);
                }
            }
        }
        elem.map(|t| Type::List(Box::new(t)))
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Option<Type> {
        if BUILTINS.contains(&name) {
            return self.builtin(name, args);
        }
        let Some((params, ret)) = self.sigs.get(name).cloned() else {
            self.err("SXP-412", &format!("unknown stanza or function {name:?}"));
            for a in args {
                self.infer(a, None);
            }
            return None;
        };
        if params.len() != args.len() {
            self.err(
                "SXP-413",
                &format!(
                    "{name:?} takes {} argument(s), {} given",
                    params.len(),
                    args.len()
                ),
            );
            return Some(ret);
        }
        for (a, p) in args.iter().zip(&params) {
            self.expect(a, p, &format!("argument to {name:?}"));
        }
        Some(ret)
    }

    fn builtin(&mut self, name: &str, args: &[Expr]) -> Option<Type> {
        let arity = match name {
            "hear" => 0,
            "append" => 2,
            _ => 1,
        };
        if args.len() != arity {
            self.err(
                "SXP-413",
                &format!("{name:?} takes {arity} argument(s), {} given", args.len()),
            );
            return None;
        }
        if name == "hear" {
            return Some(Type::String);
        }
        if name == "append" {
            let Type::List(elem) = self.infer(&args[0], None)? else {
                self.err("SXP-402", "'appended' needs a list on the left");
                return None;
            };
            self.expect(&args[1], &elem, "appended element");
            return Some(Type::List(elem));
        }
        let t = self.infer(&args[0], None)?;
        let (ok, out) = match (name, &t) {
            ("speak" | "speakln", Type::String) => (true, Type::Unit),
            ("length", Type::List(_) | Type::String)
            | ("to_integer", Type::Float | Type::String) => (true, Type::Integer),
            ("to_string", Type::Integer | Type::Float | Type::Boolean | Type::Character) => {
                (true, Type::String)
            }
            ("to_float", Type::Integer) => (true, Type::Float),
            _ => (false, Type::Unit),
        };
        if ok {
            Some(out)
        } else {
            self.err("SXP-402", &format!("{name:?} cannot take {t:?}"));
            None
        }
    }
}
