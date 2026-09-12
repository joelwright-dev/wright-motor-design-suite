//! The port type registry file (`library/ports.kdl`).

use indexmap::IndexMap;
use kdl::{KdlDocument, KdlNode};
use wmds_expr::Expr;

use crate::{
    Ctx, SchemaError, SchemaErrors, child, first_positional_string, prop, prop_string,
    value_to_string,
};

/// Declared type of a port parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamKind {
    Int,
    Text,
    Bool,
    /// A quantity in the named unit, e.g. `mm`, `N/mm`, `in`.
    Quantity(String),
}

#[derive(Debug, Clone)]
pub struct PortParamDecl {
    pub name: String,
    pub kind: ParamKind,
    pub optional: bool,
}

/// Degrees of freedom a mate leaves free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dof {
    #[default]
    Fixed,
    Revolute,
    Prismatic,
    Spherical,
    Planar,
}

impl Dof {
    pub fn parse(s: &str) -> Option<Dof> {
        Some(match s {
            "fixed" => Dof::Fixed,
            "revolute" => Dof::Revolute,
            "prismatic" => Dof::Prismatic,
            "spherical" => Dof::Spherical,
            "planar" => Dof::Planar,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Dof::Fixed => "fixed",
            Dof::Revolute => "revolute",
            Dof::Prismatic => "prismatic",
            Dof::Spherical => "spherical",
            Dof::Planar => "planar",
        }
    }
}

/// Who makes the joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    /// Made in production, before the kit ships. Bonding, welding, pressing are allowed here.
    #[default]
    Factory,
    /// Made by the person assembling the vehicle. Hand tools and threaded fasteners only.
    Kit,
}

impl Stage {
    pub fn parse(s: &str) -> Option<Stage> {
        match s {
            "factory" => Some(Stage::Factory),
            "kit" => Some(Stage::Kit),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Stage::Factory => "factory",
            Stage::Kit => "kit",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompatRule {
    pub other: String,
    pub when: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct PortTypeDef {
    pub name: String,
    pub doc: String,
    pub params: Vec<PortParamDecl>,
    pub dof: Dof,
    /// `true` for aligned, `false` for opposed.
    pub aligned: bool,
    pub compatible: Vec<CompatRule>,
    pub grid: bool,
    pub stage: Stage,
}

impl PortTypeDef {
    pub fn param(&self, name: &str) -> Option<&PortParamDecl> {
        self.params.iter().find(|p| p.name == name)
    }
}

/// Parse `ports.kdl`, returning the definitions in declaration order.
pub fn parse_port_types(
    name: &str,
    src: &str,
) -> Result<IndexMap<String, PortTypeDef>, SchemaErrors> {
    let doc: KdlDocument = match src.parse() {
        Ok(d) => d,
        Err(e) => {
            let errors = e
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
    let mut out: IndexMap<String, PortTypeDef> = IndexMap::new();
    for node in doc.nodes() {
        if node.name().value() != "port_type" {
            ctx.err(
                node,
                format!(
                    "unexpected node `{}`; expected `port_type`",
                    node.name().value()
                ),
            );
            continue;
        }
        if let Some(def) = parse_one(&mut ctx, node) {
            if out.contains_key(&def.name) {
                ctx.err(node, format!("duplicate port type `{}`", def.name));
            }
            out.insert(def.name.clone(), def);
        }
    }
    // Every `compatible_with` must name a known type.
    let known: Vec<String> = out.keys().cloned().collect();
    for node in doc.nodes() {
        for c in node.children().map(|c| c.nodes()).unwrap_or(&[]) {
            if c.name().value() == "compatible_with"
                && let Some(other) = first_positional_string(c)
                && !known.contains(&other)
            {
                ctx.err(c, format!("unknown port type `{other}`"));
            }
        }
    }
    if ctx.errors.is_empty() {
        Ok(out)
    } else {
        Err(SchemaErrors::new(name, src, ctx.errors))
    }
}

fn parse_one(ctx: &mut Ctx, node: &KdlNode) -> Option<PortTypeDef> {
    let name = match first_positional_string(node) {
        Some(n) => n,
        None => {
            ctx.err(node, "`port_type` needs a name");
            return None;
        }
    };
    let doc = child(node, "doc")
        .and_then(first_positional_string)
        .unwrap_or_default();

    let mut params = Vec::new();
    if let Some(p) = child(node, "params") {
        for e in p.entries() {
            let Some(key) = e.name() else {
                ctx.err_entry(e, "port params must be written as name=\"type\"");
                continue;
            };
            let raw = value_to_string(e.value());
            let optional = raw.ends_with('?');
            let base = raw.trim_end_matches('?');
            let kind = match base {
                "int" => ParamKind::Int,
                "string" => ParamKind::Text,
                "bool" => ParamKind::Bool,
                unit => match wmds_units::Quantity::dim_of_unit(unit) {
                    Ok(_) => ParamKind::Quantity(unit.to_string()),
                    Err(err) => {
                        ctx.err_entry(e, format!("param `{}`: {err}", key.value()));
                        continue;
                    }
                },
            };
            params.push(PortParamDecl {
                name: key.value().to_string(),
                kind,
                optional,
            });
        }
    }

    let dof = match child(node, "dof").and_then(first_positional_string) {
        Some(s) => match Dof::parse(&s) {
            Some(d) => d,
            None => {
                ctx.err(
                    node,
                    format!("unknown dof `{s}` (fixed, revolute, prismatic, spherical, planar)"),
                );
                Dof::Fixed
            }
        },
        None => Dof::Fixed,
    };

    let aligned = match child(node, "mate_axis").and_then(first_positional_string) {
        Some(s) => match s.as_str() {
            "aligned" => true,
            "opposed" => false,
            other => {
                ctx.err(
                    node,
                    format!("unknown mate_axis `{other}` (opposed or aligned)"),
                );
                false
            }
        },
        None => false,
    };

    let stage = match child(node, "stage").and_then(first_positional_string) {
        Some(s) => match Stage::parse(&s) {
            Some(v) => v,
            None => {
                ctx.err(node, format!("unknown stage `{s}` (factory or kit)"));
                Stage::Factory
            }
        },
        None => Stage::Factory,
    };

    let grid = child(node, "grid")
        .and_then(|g| {
            prop(g, "value")
                .and_then(|v| v.as_bool())
                .or_else(|| g.entries().first().and_then(|e| e.value().as_bool()))
        })
        .unwrap_or(false);

    let mut compatible = Vec::new();
    for c in node.children().map(|c| c.nodes()).unwrap_or(&[]) {
        if c.name().value() != "compatible_with" {
            continue;
        }
        let Some(other) = first_positional_string(c) else {
            ctx.err(c, "`compatible_with` needs a port type name");
            continue;
        };
        let when = match prop_string(c, "when") {
            Some(text) => match wmds_expr::parse(&text) {
                Ok(e) => Some(e),
                Err(err) => {
                    ctx.err(c, format!("`when` expression: {err}"));
                    None
                }
            },
            None => None,
        };
        compatible.push(CompatRule { other, when });
    }

    Some(PortTypeDef {
        name,
        doc,
        params,
        dof,
        aligned,
        compatible,
        grid,
        stage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
port_type "bolt.pattern" {
    doc "Circular bolt pattern"
    params count="int" pcd="mm" thread="string" centre_bore="mm?"
    dof "fixed"
    mate_axis "opposed"
    compatible_with "bolt.pattern" when="a.count == b.count and abs(a.pcd - b.pcd) < 0.2 mm"
    stage "kit"
}
port_type "bush.pivot" {
    params bush_od="mm" bolt="string"
    dof "revolute"
    mate_axis "aligned"
    compatible_with "bolt.pattern"
    grid #true
}
"#;

    #[test]
    fn parses_registry() {
        let r = parse_port_types("ports.kdl", SRC)
            .map_err(|e| format!("{e:?}"))
            .unwrap();
        assert_eq!(r.len(), 2);
        let bp = &r["bolt.pattern"];
        assert_eq!(bp.dof, Dof::Fixed);
        assert!(!bp.aligned);
        assert_eq!(bp.stage, Stage::Kit);
        assert_eq!(bp.params.len(), 4);
        assert!(bp.param("centre_bore").unwrap().optional);
        assert_eq!(bp.param("count").unwrap().kind, ParamKind::Int);
        assert_eq!(
            bp.param("pcd").unwrap().kind,
            ParamKind::Quantity("mm".into())
        );
        let bu = &r["bush.pivot"];
        assert_eq!(bu.dof, Dof::Revolute);
        assert!(bu.aligned);
        assert!(bu.grid);
        assert_eq!(bu.stage, Stage::Factory);
    }

    #[test]
    fn rejects_unknown_compatible_type_and_bad_unit() {
        let bad = SRC.replace(
            "compatible_with \"bolt.pattern\" when",
            "compatible_with \"nope.type\" when",
        );
        let e = parse_port_types("ports.kdl", &bad).expect_err("should fail");
        assert!(
            e.errors.iter().any(|x| x.msg.contains("nope.type")),
            "{:?}",
            e.errors
        );

        let bad = SRC.replace("pcd=\"mm\"", "pcd=\"cubits\"");
        let e = parse_port_types("ports.kdl", &bad).expect_err("should fail");
        assert!(e.errors.iter().any(|x| x.msg.contains("cubits")));
    }
}
