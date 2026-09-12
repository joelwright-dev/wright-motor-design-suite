use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;
use wmds_units::{Dim, Quantity, UnitError};

use crate::{BinOp, Expr, UnOp, Value};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum EvalError {
    #[error("unknown name `{0}`")]
    Unknown(String),
    #[error("{0}")]
    Unit(#[from] UnitError),
    #[error("type error: {0}")]
    Type(String),
    #[error("unknown function `{0}`")]
    UnknownFunction(String),
    #[error("{0}")]
    Other(String),
}

/// Name resolution for expressions. Hosts implement this over their own model.
pub trait Env {
    /// Resolve a dotted path. Return `None` if the first segment is unknown.
    fn lookup(&self, path: &[String]) -> Option<Value>;
}

/// A simple environment backed by a map, with optional parent for lambda scopes.
pub struct MapEnv<'a> {
    pub vars: HashMap<String, Value>,
    pub parent: Option<&'a dyn Env>,
}

impl<'a> MapEnv<'a> {
    pub fn new() -> Self {
        MapEnv {
            vars: HashMap::new(),
            parent: None,
        }
    }

    pub fn with_parent(parent: &'a dyn Env) -> Self {
        MapEnv {
            vars: HashMap::new(),
            parent: Some(parent),
        }
    }

    pub fn set(&mut self, name: &str, v: Value) {
        self.vars.insert(name.to_string(), v);
    }
}

impl Default for MapEnv<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Env for MapEnv<'_> {
    fn lookup(&self, path: &[String]) -> Option<Value> {
        if let Some(v) = self.vars.get(&path[0]) {
            return walk(v, &path[1..]);
        }
        self.parent.and_then(|p| p.lookup(path))
    }
}

/// Follow record fields along the remaining path.
pub fn walk(v: &Value, rest: &[String]) -> Option<Value> {
    let mut cur = v.clone();
    for seg in rest {
        cur = match &cur {
            Value::Record(_) => cur.field(seg)?.clone(),
            Value::Tuple(t) => match seg.as_str() {
                "x" => t.first()?.clone(),
                "y" => t.get(1)?.clone(),
                "z" => t.get(2)?.clone(),
                _ => return None,
            },
            _ => return None,
        };
    }
    Some(cur)
}

/// Constants every definition file may use without declaring them.
///
/// A name in scope always wins, so a primitive that declares its own `pi` gets its own, wrong as
/// that would be. Only added where a vehicle actually needs them: piston areas, swept volumes
/// and wheel circumferences all want pi, and writing 3.14159 in a definition file is how a
/// rounding error gets into a brake calculation.
fn constant(path: &[String]) -> Option<Value> {
    if path.len() != 1 {
        return None;
    }
    let q = match path[0].as_str() {
        "pi" => std::f64::consts::PI,
        "tau" => std::f64::consts::TAU,
        "e" => std::f64::consts::E,
        _ => return None,
    };
    Some(Value::Num(wmds_units::Quantity::dimensionless(q)))
}

/// Evaluate `e` in `env`.
pub fn eval(e: &Expr, env: &dyn Env) -> Result<Value, EvalError> {
    match e {
        Expr::Num(q) => Ok(Value::Num(*q)),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Path(p) => env
            .lookup(p)
            .or_else(|| constant(p))
            .ok_or_else(|| EvalError::Unknown(p.join("."))),
        Expr::Tuple(items) => Ok(Value::Tuple(
            items
                .iter()
                .map(|x| eval(x, env))
                .collect::<Result<_, _>>()?,
        )),
        Expr::List(items) => Ok(Value::List(
            items
                .iter()
                .map(|x| eval(x, env))
                .collect::<Result<_, _>>()?,
        )),
        Expr::Unary(op, x) => {
            let v = eval(x, env)?;
            match (op, v) {
                (UnOp::Neg, Value::Num(q)) => Ok(Value::Num(-q)),
                (UnOp::Neg, Value::Tuple(t)) => Ok(Value::Tuple(
                    t.into_iter()
                        .map(|x| {
                            x.as_quantity().map(|q| Value::Num(-q)).ok_or_else(|| {
                                EvalError::Type("negating a non-numeric tuple".into())
                            })
                        })
                        .collect::<Result<_, _>>()?,
                )),
                (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                (op, v) => Err(EvalError::Type(format!(
                    "cannot apply {op:?} to {}",
                    v.type_name()
                ))),
            }
        }
        Expr::Binary(op, a, b) => {
            // Short-circuit logic first.
            match op {
                BinOp::And => {
                    let l = expect_bool(eval(a, env)?)?;
                    return if !l {
                        Ok(Value::Bool(false))
                    } else {
                        Ok(Value::Bool(expect_bool(eval(b, env)?)?))
                    };
                }
                BinOp::Or => {
                    let l = expect_bool(eval(a, env)?)?;
                    return if l {
                        Ok(Value::Bool(true))
                    } else {
                        Ok(Value::Bool(expect_bool(eval(b, env)?)?))
                    };
                }
                _ => {}
            }
            let l = eval(a, env)?;
            let r = eval(b, env)?;
            binary(*op, l, r)
        }
        Expr::If(c, a, b) => {
            if expect_bool(eval(c, env)?)? {
                eval(a, env)
            } else {
                eval(b, env)
            }
        }
        Expr::Lambda(p, body) => Ok(Value::Lambda(p.clone(), Arc::new((**body).clone()))),
        Expr::Call(name, args) => {
            let vals = args
                .iter()
                .map(|x| eval(x, env))
                .collect::<Result<Vec<_>, _>>()?;
            call(name, vals, env)
        }
        Expr::TextOr(text, inner) => match eval(inner, env) {
            Err(EvalError::Unknown(_)) if !inner.contains_number() => Ok(Value::Str(text.clone())),
            other => other,
        },
        Expr::Member(base, name) => {
            let v = eval(base, env)?;
            walk(&v, std::slice::from_ref(name))
                .ok_or_else(|| EvalError::Unknown(format!("field `{name}` on {}", v.type_name())))
        }
        Expr::MethodCall(base, name, args) => {
            let v = eval(base, env)?;
            let mut vals = vec![v];
            for a in args {
                vals.push(eval(a, env)?);
            }
            call(name, vals, env)
        }
    }
}

fn expect_bool(v: Value) -> Result<bool, EvalError> {
    v.as_bool()
        .ok_or_else(|| EvalError::Type(format!("expected bool, found {}", v.type_name())))
}

fn expect_num(v: &Value) -> Result<Quantity, EvalError> {
    v.as_quantity()
        .ok_or_else(|| EvalError::Type(format!("expected number, found {}", v.type_name())))
}

fn binary(op: BinOp, l: Value, r: Value) -> Result<Value, EvalError> {
    use BinOp::*;
    match op {
        Add | Sub | Mul | Div | Pow => match (&l, &r) {
            (Value::Num(a), Value::Num(b)) => Ok(Value::Num(arith(op, *a, *b)?)),
            // element-wise tuple arithmetic with a scalar or another tuple of equal length
            (Value::Tuple(t), Value::Num(b)) => Ok(Value::Tuple(
                t.iter()
                    .map(|x| Ok(Value::Num(arith(op, expect_num(x)?, *b)?)))
                    .collect::<Result<_, EvalError>>()?,
            )),
            (Value::Num(a), Value::Tuple(t)) if matches!(op, Mul) => Ok(Value::Tuple(
                t.iter()
                    .map(|x| Ok(Value::Num(arith(op, *a, expect_num(x)?)?)))
                    .collect::<Result<_, EvalError>>()?,
            )),
            (Value::Tuple(t), Value::Tuple(u)) if t.len() == u.len() && matches!(op, Add | Sub) => {
                Ok(Value::Tuple(
                    t.iter()
                        .zip(u)
                        .map(|(x, y)| Ok(Value::Num(arith(op, expect_num(x)?, expect_num(y)?)?)))
                        .collect::<Result<_, EvalError>>()?,
                ))
            }
            (Value::Str(a), Value::Str(b)) if matches!(op, Add) => {
                Ok(Value::Str(format!("{a}{b}")))
            }
            _ => Err(EvalError::Type(format!(
                "cannot apply {op:?} to {} and {}",
                l.type_name(),
                r.type_name()
            ))),
        },
        Eq => Ok(Value::Bool(values_equal(&l, &r))),
        Ne => Ok(Value::Bool(!values_equal(&l, &r))),
        Lt | Le | Gt | Ge => {
            let (a, b) = match (&l, &r) {
                (Value::Num(a), Value::Num(b)) => {
                    a.same_dim(b)?;
                    (a.value, b.value)
                }
                _ => {
                    return Err(EvalError::Type(format!(
                        "cannot compare {} and {}",
                        l.type_name(),
                        r.type_name()
                    )));
                }
            };
            Ok(Value::Bool(match op {
                Lt => a < b,
                Le => a <= b,
                Gt => a > b,
                _ => a >= b,
            }))
        }
        In => match r {
            Value::List(items) => Ok(Value::Bool(items.iter().any(|x| values_equal(x, &l)))),
            _ => Err(EvalError::Type("right side of `in` must be a list".into())),
        },
        And | Or => unreachable!("handled in eval"),
    }
}

fn arith(op: BinOp, a: Quantity, b: Quantity) -> Result<Quantity, EvalError> {
    Ok(match op {
        BinOp::Add => a.try_add(b)?,
        BinOp::Sub => a.try_sub(b)?,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Pow => {
            if !b.dim.is_dimensionless() {
                return Err(EvalError::Type("exponent must be dimensionless".into()));
            }
            if b.value == b.value.trunc() && b.value.abs() <= 127.0 {
                a.powi(b.value as i8)
            } else if a.dim.is_dimensionless() {
                Quantity::dimensionless(a.value.powf(b.value))
            } else {
                return Err(EvalError::Type(
                    "non-integer exponent on a quantity with units".into(),
                ));
            }
        }
        _ => unreachable!(),
    })
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => {
            x.dim == y.dim
                && (x.value - y.value).abs() <= 1e-9 * (1.0 + x.value.abs().max(y.value.abs()))
        }
        (Value::Tuple(x), Value::Tuple(y)) | (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| values_equal(p, q))
        }
        _ => a == b,
    }
}

fn apply_lambda(f: &Value, arg: Value, env: &dyn Env) -> Result<Value, EvalError> {
    match f {
        Value::Lambda(p, body) => {
            let mut scope = MapEnv::with_parent(env);
            scope.set(p, arg);
            eval(body, &scope)
        }
        _ => Err(EvalError::Type(format!(
            "expected lambda, found {}",
            f.type_name()
        ))),
    }
}

fn call(name: &str, args: Vec<Value>, env: &dyn Env) -> Result<Value, EvalError> {
    let n = args.len();
    let arity = |k: usize| -> Result<(), EvalError> {
        if n == k {
            Ok(())
        } else {
            Err(EvalError::Type(format!(
                "{name} expects {k} argument(s), got {n}"
            )))
        }
    };
    match name {
        "min" | "max" => {
            if n == 0 {
                return Err(EvalError::Type(format!(
                    "{name} needs at least one argument"
                )));
            }
            let items: Vec<Value> = if n == 1 {
                match &args[0] {
                    Value::List(v) => v.clone(),
                    _ => args.clone(),
                }
            } else {
                args.clone()
            };
            let mut best = expect_num(&items[0])?;
            for it in &items[1..] {
                let q = expect_num(it)?;
                best.same_dim(&q)?;
                if (name == "min" && q.value < best.value)
                    || (name == "max" && q.value > best.value)
                {
                    best = q;
                }
            }
            Ok(Value::Num(best))
        }
        "abs" => {
            arity(1)?;
            Ok(Value::Num(expect_num(&args[0])?.abs()))
        }
        "sqrt" => {
            arity(1)?;
            Ok(Value::Num(expect_num(&args[0])?.sqrt()?))
        }
        "floor" | "ceil" | "round" => {
            arity(1)?;
            let q = expect_num(&args[0])?;
            let v = match name {
                "floor" => q.value.floor(),
                "ceil" => q.value.ceil(),
                _ => q.value.round(),
            };
            Ok(Value::Num(Quantity::new(v, q.dim)))
        }
        "sin" | "cos" | "tan" => {
            arity(1)?;
            let q = expect_num(&args[0])?;
            if !(q.dim == Dim::ANGLE || q.dim.is_dimensionless()) {
                return Err(EvalError::Type(format!("{name} expects an angle")));
            }
            let v = match name {
                "sin" => q.value.sin(),
                "cos" => q.value.cos(),
                _ => q.value.tan(),
            };
            Ok(Value::num(v))
        }
        "atan2" => {
            arity(2)?;
            let y = expect_num(&args[0])?;
            let x = expect_num(&args[1])?;
            y.same_dim(&x)?;
            Ok(Value::Num(Quantity::new(
                y.value.atan2(x.value),
                Dim::ANGLE,
            )))
        }
        "len" | "count" => {
            arity(1)?;
            match &args[0] {
                Value::List(v) | Value::Tuple(v) => Ok(Value::num(v.len() as f64)),
                Value::Str(s) => Ok(Value::num(s.chars().count() as f64)),
                v => Err(EvalError::Type(format!("{name} of {}", v.type_name()))),
            }
        }
        "sum" => {
            arity(1)?;
            match &args[0] {
                Value::List(v) => {
                    let mut acc: Option<Quantity> = None;
                    for it in v {
                        let q = expect_num(it)?;
                        acc = Some(match acc {
                            None => q,
                            Some(a) => a.try_add(q)?,
                        });
                    }
                    Ok(Value::Num(acc.unwrap_or(Quantity::dimensionless(0.0))))
                }
                v => Err(EvalError::Type(format!("sum of {}", v.type_name()))),
            }
        }
        "all" | "any" | "map" | "filter" => {
            arity(2)?;
            let items = match &args[0] {
                Value::List(v) => v.clone(),
                v => {
                    return Err(EvalError::Type(format!(
                        "{name} expects a list, found {}",
                        v.type_name()
                    )));
                }
            };
            let f = &args[1];
            match name {
                "all" => {
                    for it in items {
                        if !expect_bool(apply_lambda(f, it, env)?)? {
                            return Ok(Value::Bool(false));
                        }
                    }
                    Ok(Value::Bool(true))
                }
                "any" => {
                    for it in items {
                        if expect_bool(apply_lambda(f, it, env)?)? {
                            return Ok(Value::Bool(true));
                        }
                    }
                    Ok(Value::Bool(false))
                }
                "map" => Ok(Value::List(
                    items
                        .into_iter()
                        .map(|it| apply_lambda(f, it, env))
                        .collect::<Result<_, _>>()?,
                )),
                _ => {
                    let mut out = Vec::new();
                    for it in items {
                        if expect_bool(apply_lambda(f, it.clone(), env)?)? {
                            out.push(it);
                        }
                    }
                    Ok(Value::List(out))
                }
            }
        }
        "norm" => {
            arity(1)?;
            let v = args[0]
                .as_vec3()
                .ok_or_else(|| EvalError::Type("norm expects a 3-tuple".into()))?;
            let sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
            Ok(Value::Num(sq.sqrt()?))
        }
        "dot" => {
            arity(2)?;
            let a = args[0]
                .as_vec3()
                .ok_or_else(|| EvalError::Type("dot expects 3-tuples".into()))?;
            let b = args[1]
                .as_vec3()
                .ok_or_else(|| EvalError::Type("dot expects 3-tuples".into()))?;
            Ok(Value::Num(a[0] * b[0] + a[1] * b[1] + a[2] * b[2]))
        }
        "str" => {
            arity(1)?;
            Ok(Value::Str(args[0].to_string()))
        }
        _ => Err(EvalError::UnknownFunction(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn run(src: &str, env: &dyn Env) -> Value {
        eval(&parse(src).unwrap(), env).unwrap_or_else(|e| panic!("{src}: {e}"))
    }

    #[test]
    fn arithmetic_with_units() {
        let mut env = MapEnv::new();
        env.set(
            "track_width",
            Value::Num(Quantity::parse("1500 mm").unwrap()),
        );
        let v = run("2 * track_width - 120 mm", &env);
        assert_eq!(
            v.as_quantity().unwrap().to_unit("mm").unwrap().round(),
            2880.0
        );
        let v = run("(-track_width/2, 0 mm, 0 mm)", &env);
        let t = v.as_vec3().unwrap();
        assert_eq!(t[0].to_unit("mm").unwrap(), -750.0);
    }

    #[test]
    fn mismatched_units_fail() {
        let env = MapEnv::new();
        let r = eval(&parse("1 kg + 2 mm").unwrap(), &env);
        assert!(matches!(r, Err(EvalError::Unit(_))));
    }

    #[test]
    fn logic_and_conditionals() {
        let mut env = MapEnv::new();
        env.set("kind", Value::Str("coil".into()));
        env.set("k", Value::Num(Quantity::parse("25 N/mm").unwrap()));
        let v = run("if kind == \"coil\" then k else 0 N/mm", &env);
        assert_eq!(v.as_quantity().unwrap().to_unit("N/mm").unwrap(), 25.0);
        assert_eq!(run("kind in [\"coil\", \"leaf\"]", &env), Value::Bool(true));
        assert_eq!(run("not (1 > 2) and 3 >= 3", &env), Value::Bool(true));
    }

    #[test]
    fn records_lambdas_and_lists() {
        let mut env = MapEnv::new();
        let lamp = |z: f64| {
            Value::record(vec![(
                "centre".into(),
                Value::Tuple(vec![
                    Value::num(0.0),
                    Value::num(0.0),
                    Value::Num(Quantity::parse(&format!("{z} mm")).unwrap()),
                ]),
            )])
        };
        env.set("lamps", Value::List(vec![lamp(600.0), lamp(700.0)]));
        assert_eq!(
            run(
                "all(lamps, l -> l.centre.z >= 500 mm and l.centre.z <= 1200 mm)",
                &env
            ),
            Value::Bool(true)
        );
        assert_eq!(
            run("count(filter(lamps, l -> l.centre.z > 650 mm))", &env),
            Value::num(1.0)
        );
        assert_eq!(run("min(3, 1, 2)", &env), Value::num(1.0));
        assert_eq!(
            run("norm((3 mm, 4 mm, 0 mm))", &env)
                .as_quantity()
                .unwrap()
                .to_unit("mm")
                .unwrap(),
            5.0
        );
    }
}
