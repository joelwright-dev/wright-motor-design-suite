//! The WMDS expression language.
//!
//! A small, pure language used in primitive parameters, port frames, rule checks and cost
//! models. It has quantities with units, booleans, strings, tuples, lists and lambdas; no loops,
//! no side effects, no I/O.
//!
//! ```text
//! length = 2 * track_width - 120 mm
//! rate   = if spring.kind == "coil" then spring.k else 0 N/mm
//! ok     = headlamp.centre.z >= 500 mm and headlamp.centre.z <= 1200 mm
//! at     = (-span/2, 0 mm, 0 mm)
//! all(lamps, l -> l.height > 500 mm)
//! ```

mod eval;
mod lexer;
mod parser;
mod value;

pub use eval::{Env, EvalError, MapEnv, eval, walk};
pub use parser::{ParseError, parse};
pub use value::Value;

/// Abstract syntax tree of an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(wmds_units::Quantity),
    Bool(bool),
    Str(String),
    /// Dotted path such as `spring.k` or a bare identifier.
    Path(Vec<String>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Lambda(String, Box<Expr>),
    /// Member access on a computed value, e.g. `l.port("lens").centre`.
    Member(Box<Expr>, String),
    /// Text from a definition file that also parses as an expression.
    ///
    /// Definition files write both values and expressions as quoted strings, so `"M12"` and
    /// `"span / 2"` look alike. A few literals parse as expressions by accident:
    /// `"amphenol-surlok"` is a subtraction of two names that do not exist. When such an
    /// expression cannot be evaluated *and* contains no numbers, it was a word all along and
    /// evaluates to the text. Anything containing a number stays an error, so a misspelled
    /// parameter in `"(spann / 2, 0, 0)"` is still caught.
    TextOr(String, Box<Expr>),
    /// Method call on a computed value, e.g. `l.port("lens")`.
    MethodCall(Box<Expr>, String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    In,
}

impl Expr {
    /// Does this expression contain a numeric literal anywhere?
    ///
    /// Used to decide whether an unevaluatable expression was meant as text. A word has no
    /// numbers in it; a mistyped formula usually does.
    pub fn contains_number(&self) -> bool {
        match self {
            Expr::Num(_) => true,
            Expr::Bool(_) | Expr::Str(_) | Expr::Path(_) => false,
            Expr::Tuple(v) | Expr::List(v) => v.iter().any(|e| e.contains_number()),
            Expr::Unary(_, e) | Expr::Lambda(_, e) | Expr::Member(e, _) | Expr::TextOr(_, e) => {
                e.contains_number()
            }
            Expr::Binary(_, a, b) => a.contains_number() || b.contains_number(),
            Expr::If(c, a, b) => c.contains_number() || a.contains_number() || b.contains_number(),
            // A function call is never a stray word, so treat it as formula-like.
            Expr::Call(..) | Expr::MethodCall(..) => true,
        }
    }

    /// Every path referenced by the expression, used for dependency ordering of parameters.
    pub fn references(&self) -> Vec<&[String]> {
        let mut out = Vec::new();
        self.collect_refs(&mut out);
        out
    }

    fn collect_refs<'a>(&'a self, out: &mut Vec<&'a [String]>) {
        match self {
            Expr::Path(p) => out.push(p.as_slice()),
            Expr::Num(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::Tuple(v) | Expr::List(v) => v.iter().for_each(|e| e.collect_refs(out)),
            Expr::Unary(_, e) => e.collect_refs(out),
            Expr::Binary(_, a, b) => {
                a.collect_refs(out);
                b.collect_refs(out);
            }
            Expr::If(c, a, b) => {
                c.collect_refs(out);
                a.collect_refs(out);
                b.collect_refs(out);
            }
            Expr::Call(_, args) => args.iter().for_each(|e| e.collect_refs(out)),
            Expr::Lambda(_, body) => body.collect_refs(out),
            Expr::Member(e, _) => e.collect_refs(out),
            Expr::TextOr(_, e) => e.collect_refs(out),
            Expr::MethodCall(e, _, args) => {
                e.collect_refs(out);
                args.iter().for_each(|e| e.collect_refs(out));
            }
        }
    }
}
