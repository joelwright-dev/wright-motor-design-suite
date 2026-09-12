use thiserror::Error;
use wmds_units::Quantity;

use crate::lexer::{Tok, lex};
use crate::{BinOp, Expr, UnOp};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    #[error("bad number `{0}`")]
    BadNumber(String),
    #[error("unit error: {0}")]
    Unit(String),
    #[error("unterminated string")]
    UnterminatedString,
    #[error("unexpected character `{0}`")]
    UnexpectedChar(char),
    #[error("unexpected end of expression")]
    UnexpectedEnd,
    #[error("unexpected token {0}")]
    UnexpectedToken(String),
    #[error("expected {0}")]
    Expected(String),
}

/// Parse an expression from text.
pub fn parse(src: &str) -> Result<Expr, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr(0)?;
    if p.pos != p.toks.len() {
        return Err(ParseError::UnexpectedToken(p.describe()));
    }
    Ok(e)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

// Binding powers, lowest to highest.
const BP_LAMBDA: u8 = 1;
const BP_OR: u8 = 2;
const BP_AND: u8 = 3;
const BP_NOT: u8 = 4;
const BP_CMP: u8 = 5;
const BP_ADD: u8 = 6;
const BP_MUL: u8 = 7;
const BP_NEG: u8 = 8;
const BP_POW: u8 = 9;
const BP_POSTFIX: u8 = 10;

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn describe(&self) -> String {
        match self.peek() {
            Some(t) => format!("{t:?}"),
            None => "end".to_string(),
        }
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, op: &str) -> Result<(), ParseError> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(ParseError::Expected(format!(
                "`{op}` but found {}",
                self.describe()
            )))
        }
    }

    fn expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.prefix()?;
        loop {
            let (op, l_bp, r_bp): (BinOp, u8, u8) = match self.peek() {
                Some(Tok::Op("+")) => (BinOp::Add, BP_ADD, BP_ADD + 1),
                Some(Tok::Op("-")) => (BinOp::Sub, BP_ADD, BP_ADD + 1),
                Some(Tok::Op("*")) => (BinOp::Mul, BP_MUL, BP_MUL + 1),
                Some(Tok::Op("/")) => (BinOp::Div, BP_MUL, BP_MUL + 1),
                Some(Tok::Op("^")) => (BinOp::Pow, BP_POW, BP_POW), // right assoc
                Some(Tok::Op("<")) => (BinOp::Lt, BP_CMP, BP_CMP + 1),
                Some(Tok::Op("<=")) => (BinOp::Le, BP_CMP, BP_CMP + 1),
                Some(Tok::Op(">")) => (BinOp::Gt, BP_CMP, BP_CMP + 1),
                Some(Tok::Op(">=")) => (BinOp::Ge, BP_CMP, BP_CMP + 1),
                Some(Tok::Op("==")) => (BinOp::Eq, BP_CMP, BP_CMP + 1),
                Some(Tok::Op("!=")) => (BinOp::Ne, BP_CMP, BP_CMP + 1),
                Some(Tok::Op("and")) => (BinOp::And, BP_AND, BP_AND + 1),
                Some(Tok::Op("or")) => (BinOp::Or, BP_OR, BP_OR + 1),
                Some(Tok::Ident(s)) if s == "and" => (BinOp::And, BP_AND, BP_AND + 1),
                Some(Tok::Ident(s)) if s == "or" => (BinOp::Or, BP_OR, BP_OR + 1),
                Some(Tok::Ident(s)) if s == "in" => (BinOp::In, BP_CMP, BP_CMP + 1),
                Some(Tok::Op(".")) => {
                    if BP_POSTFIX < min_bp {
                        break;
                    }
                    self.pos += 1;
                    let name = match self.next() {
                        Some(Tok::Ident(n)) => n,
                        _ => return Err(ParseError::Expected("member name after `.`".into())),
                    };
                    if self.eat_op("(") {
                        let args = self.args()?;
                        lhs = Expr::MethodCall(Box::new(lhs), name, args);
                    } else {
                        lhs = match lhs {
                            Expr::Path(mut p) => {
                                p.push(name);
                                Expr::Path(p)
                            }
                            other => Expr::Member(Box::new(other), name),
                        };
                    }
                    continue;
                }
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.pos += 1;
            let rhs = self.expr(r_bp)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn prefix(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            None => Err(ParseError::UnexpectedEnd),
            Some(Tok::Num(q)) => Ok(Expr::Num(q)),
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::Op("-")) => {
                let e = self.expr(BP_NEG)?;
                Ok(match e {
                    Expr::Num(q) => Expr::Num(-q),
                    other => Expr::Unary(UnOp::Neg, Box::new(other)),
                })
            }
            Some(Tok::Op("+")) => self.expr(BP_NEG),
            Some(Tok::Op("not")) => Ok(Expr::Unary(UnOp::Not, Box::new(self.expr(BP_NOT)?))),
            Some(Tok::Op("abs")) => {
                let e = self.expr(0)?;
                self.expect_op("abs")?;
                Ok(Expr::Call("abs".into(), vec![e]))
            }
            Some(Tok::Op("(")) => {
                if self.eat_op(")") {
                    return Ok(Expr::Tuple(vec![]));
                }
                let first = self.expr(0)?;
                if self.eat_op(",") {
                    let mut items = vec![first];
                    loop {
                        if self.eat_op(")") {
                            break;
                        }
                        items.push(self.expr(0)?);
                        if !self.eat_op(",") {
                            self.expect_op(")")?;
                            break;
                        }
                    }
                    Ok(Expr::Tuple(items))
                } else {
                    self.expect_op(")")?;
                    Ok(first)
                }
            }
            Some(Tok::Op("[")) => {
                let mut items = Vec::new();
                loop {
                    if self.eat_op("]") {
                        break;
                    }
                    items.push(self.expr(0)?);
                    if !self.eat_op(",") {
                        self.expect_op("]")?;
                        break;
                    }
                }
                Ok(Expr::List(items))
            }
            Some(Tok::Ident(id)) => match id.as_str() {
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                "not" => Ok(Expr::Unary(UnOp::Not, Box::new(self.expr(BP_NOT)?))),
                "if" => {
                    let c = self.expr(0)?;
                    if !self.eat_ident("then") {
                        return Err(ParseError::Expected("`then`".into()));
                    }
                    let a = self.expr(0)?;
                    if !self.eat_ident("else") {
                        return Err(ParseError::Expected("`else`".into()));
                    }
                    let b = self.expr(BP_LAMBDA)?;
                    Ok(Expr::If(Box::new(c), Box::new(a), Box::new(b)))
                }
                _ => {
                    if self.eat_op("->") {
                        let body = self.expr(BP_LAMBDA)?;
                        return Ok(Expr::Lambda(id, Box::new(body)));
                    }
                    if self.eat_op("(") {
                        let args = self.args()?;
                        return Ok(Expr::Call(id, args));
                    }
                    Ok(Expr::Path(vec![id]))
                }
            },
            Some(t) => Err(ParseError::UnexpectedToken(format!("{t:?}"))),
        }
    }

    /// Parses call arguments after the opening `(` has been consumed.
    fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        loop {
            if self.eat_op(")") {
                break;
            }
            args.push(self.expr(0)?);
            if !self.eat_op(",") {
                self.expect_op(")")?;
                break;
            }
        }
        Ok(args)
    }
}

#[allow(dead_code)]
fn q(v: f64) -> Quantity {
    Quantity::dimensionless(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence() {
        let e = parse("1 + 2 * 3 ^ 2").unwrap();
        assert_eq!(
            e,
            Expr::Binary(
                BinOp::Add,
                Box::new(Expr::Num(q(1.0))),
                Box::new(Expr::Binary(
                    BinOp::Mul,
                    Box::new(Expr::Num(q(2.0))),
                    Box::new(Expr::Binary(
                        BinOp::Pow,
                        Box::new(Expr::Num(q(3.0))),
                        Box::new(Expr::Num(q(2.0)))
                    ))
                ))
            )
        );
    }

    #[test]
    fn units_and_paths() {
        let e = parse("2 * track_width - 120 mm").unwrap();
        assert_eq!(e.references(), vec![&["track_width".to_string()][..]]);
        let e = parse("spring.k").unwrap();
        assert_eq!(e, Expr::Path(vec!["spring".into(), "k".into()]));
    }

    #[test]
    fn tuples_calls_lambdas() {
        assert!(matches!(parse("(-span/2, 0 mm, 0 mm)").unwrap(), Expr::Tuple(v) if v.len() == 3));
        assert!(
            matches!(parse("min(a, b)").unwrap(), Expr::Call(n, v) if n == "min" && v.len() == 2)
        );
        assert!(
            matches!(parse("all(lamps, l -> l.z > 500 mm)").unwrap(), Expr::Call(_, v) if matches!(v[1], Expr::Lambda(..)))
        );
        assert!(matches!(
            parse("l.port(\"lens\").centre").unwrap(),
            Expr::Member(..)
        ));
    }

    #[test]
    fn conditionals_and_logic() {
        let e = parse("if kind == \"coil\" then k else 0 N/mm").unwrap();
        assert!(matches!(e, Expr::If(..)));
        let e = parse("a >= 1 and b <= 2 or not c").unwrap();
        assert!(matches!(e, Expr::Binary(BinOp::Or, ..)));
        assert!(matches!(
            parse("x in [1, 2, 3]").unwrap(),
            Expr::Binary(BinOp::In, ..)
        ));
    }

    #[test]
    fn unit_suffix_does_not_swallow_operands() {
        // `120 mm / 2` must be (120 mm) / 2, not 120 (mm/2).
        let e = parse("120 mm / 2").unwrap();
        assert!(matches!(e, Expr::Binary(BinOp::Div, ..)));
    }
}
