use std::fmt;
use std::sync::Arc;

use wmds_units::Quantity;

use crate::Expr;

/// Runtime value of an expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(Quantity),
    Bool(bool),
    Str(String),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    /// A record of named fields, produced by the host (e.g. a port, an instance).
    Record(Arc<Vec<(String, Value)>>),
    Lambda(String, Arc<Expr>),
}

impl Value {
    pub fn num(v: f64) -> Value {
        Value::Num(Quantity::dimensionless(v))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "number",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Tuple(_) => "tuple",
            Value::List(_) => "list",
            Value::Record(_) => "record",
            Value::Lambda(..) => "lambda",
        }
    }

    pub fn as_quantity(&self) -> Option<Quantity> {
        match self {
            Value::Num(q) => Some(*q),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A tuple of three quantities with the same dimension, as `[Quantity; 3]`.
    pub fn as_vec3(&self) -> Option<[Quantity; 3]> {
        match self {
            Value::Tuple(v) if v.len() == 3 => {
                let a = v[0].as_quantity()?;
                let b = v[1].as_quantity()?;
                let c = v[2].as_quantity()?;
                Some([a, b, c])
            }
            _ => None,
        }
    }

    pub fn field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Record(fields) => fields.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn record(fields: Vec<(String, Value)>) -> Value {
        Value::Record(Arc::new(fields))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Num(q) => write!(f, "{q}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "\"{s}\""),
            Value::Tuple(v) => {
                write!(f, "(")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }
            Value::List(v) => {
                write!(f, "[")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Value::Record(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Lambda(p, _) => write!(f, "<lambda {p}>"),
        }
    }
}
