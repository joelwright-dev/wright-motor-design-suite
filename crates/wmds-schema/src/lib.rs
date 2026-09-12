//! Parsers and validators for WMDS definition files.
//!
//! Definition files are KDL v2. This crate turns them into typed definitions
//! ([`PrimitiveDef`] and friends) with every expression parsed but not yet evaluated.
//! Evaluation happens in `wmds-model`.
//!
//! Value conventions inside definition files:
//! * A number is a dimensionless literal, or, for a parameter with a declared `unit`, a value
//!   in that unit.
//! * A quoted string is parsed as an expression (`"2 * span - 10 mm"`, `"(0 mm, reach, 0 mm)"`).
//!   If it is not a valid expression it is kept as a literal string (`"1:8"`, `"1..1000"`).
//!   A bare word that resolves to no known name is also treated as a literal string at
//!   evaluation time (`bolt="M12"`).
//! * Axes may be written as `"x"`, `"-y"`, `"z"` or as a 3-tuple expression.

mod assembly;
mod chassis;
mod material;
mod ports;
mod primitive;
mod rules;

pub use assembly::*;
pub use chassis::*;
pub use material::*;
pub use ports::*;
pub use primitive::*;
pub use rules::*;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;
use wmds_expr::Expr;

/// A single problem found in a definition file, with its location.
#[derive(Error, Debug, Diagnostic, Clone)]
#[error("{msg}")]
pub struct SchemaError {
    pub msg: String,
    #[label("here")]
    pub span: Option<SourceSpan>,
}

/// All problems found in one file.
#[derive(Error, Debug, Diagnostic)]
#[error("{} error(s) in {}", errors.len(), name)]
pub struct SchemaErrors {
    pub name: String,
    #[source_code]
    pub src: NamedSource<String>,
    #[related]
    pub errors: Vec<SchemaError>,
}

impl SchemaErrors {
    pub(crate) fn new(name: &str, src: &str, errors: Vec<SchemaError>) -> Self {
        SchemaErrors {
            name: name.to_string(),
            src: NamedSource::new(name, src.to_string()),
            errors,
        }
    }
}

/// Collects errors while walking a document so that several can be reported at once.
pub(crate) struct Ctx {
    errors: Vec<SchemaError>,
}

impl Ctx {
    fn err(&mut self, node: &KdlNode, msg: impl Into<String>) {
        self.errors.push(SchemaError {
            msg: msg.into(),
            span: Some(node.span()),
        });
    }

    fn err_entry(&mut self, entry: &KdlEntry, msg: impl Into<String>) {
        self.errors.push(SchemaError {
            msg: msg.into(),
            span: Some(entry.span()),
        });
    }
}

/// Parse a `.prim.kdl` file.
pub fn parse_primitive(name: &str, src: &str) -> Result<PrimitiveDef, SchemaErrors> {
    let doc: KdlDocument = match src.parse() {
        Ok(d) => d,
        Err(e) => {
            let kdl_err: kdl::KdlError = e;
            let errors = kdl_err
                .diagnostics
                .iter()
                .map(|d| SchemaError {
                    msg: d.to_string(),
                    span: Some(d.span),
                })
                .collect();
            return Err(SchemaErrors::new(name, src, errors));
        }
    };
    let mut ctx = Ctx { errors: Vec::new() };
    let roots: Vec<&KdlNode> = doc
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "primitive")
        .collect();
    let def = match roots.as_slice() {
        [one] => primitive::parse_primitive_node(&mut ctx, one),
        [] => {
            ctx.errors.push(SchemaError {
                msg: "file has no `primitive` node".into(),
                span: None,
            });
            None
        }
        _ => {
            ctx.err(roots[1], "only one `primitive` node is allowed per file");
            None
        }
    };
    match def {
        Some(d) if ctx.errors.is_empty() => Ok(d),
        _ => Err(SchemaErrors::new(name, src, ctx.errors)),
    }
}

// ---------- KDL helpers ----------

/// Convert a KDL value to an expression using the conventions described in the crate docs.
pub(crate) fn value_to_expr(v: &KdlValue) -> Expr {
    match v {
        KdlValue::Integer(i) => Expr::Num(wmds_units::Quantity::dimensionless(*i as f64)),
        KdlValue::Float(f) => Expr::Num(wmds_units::Quantity::dimensionless(*f)),
        KdlValue::Bool(b) => Expr::Bool(*b),
        KdlValue::Null => Expr::Str(String::new()),
        KdlValue::String(s) => match wmds_expr::parse(s) {
            // Keep the original text alongside the parse: see `Expr::TextOr`.
            Ok(Expr::Str(t)) => Expr::Str(t),
            Ok(e) => Expr::TextOr(s.clone(), Box::new(e)),
            Err(_) => Expr::Str(s.clone()),
        },
    }
}

pub(crate) fn value_to_string(v: &KdlValue) -> String {
    match v {
        KdlValue::String(s) => s.clone(),
        KdlValue::Integer(i) => i.to_string(),
        KdlValue::Float(f) => f.to_string(),
        KdlValue::Bool(b) => b.to_string(),
        KdlValue::Null => String::new(),
    }
}

pub(crate) fn prop<'a>(node: &'a KdlNode, key: &str) -> Option<&'a KdlValue> {
    node.entries()
        .iter()
        .find(|e| e.name().map(|n| n.value() == key).unwrap_or(false))
        .map(|e| e.value())
}

pub(crate) fn prop_expr(node: &KdlNode, key: &str) -> Option<Expr> {
    prop(node, key).map(value_to_expr)
}

pub(crate) fn prop_string(node: &KdlNode, key: &str) -> Option<String> {
    prop(node, key).map(value_to_string)
}

pub(crate) fn positional(node: &KdlNode) -> Vec<&KdlValue> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .map(|e| e.value())
        .collect()
}

pub(crate) fn first_positional_string(node: &KdlNode) -> Option<String> {
    positional(node).first().map(|v| value_to_string(v))
}

pub(crate) fn child<'a>(node: &'a KdlNode, name: &str) -> Option<&'a KdlNode> {
    node.children()
        .and_then(|c| c.nodes().iter().find(|n| n.name().value() == name))
}

pub(crate) fn children(node: &KdlNode) -> &[KdlNode] {
    node.children().map(|c| c.nodes()).unwrap_or(&[])
}
