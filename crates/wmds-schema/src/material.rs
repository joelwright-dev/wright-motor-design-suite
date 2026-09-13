//! Material definitions (`.mat.kdl`).
//!
//! Density and cost are used today. The elastic constants, strengths, environmental notes and
//! solver cards are carried because structural and crash work will need them, and because a
//! material that records where its numbers came from is worth far more later than one that does
//! not.

use indexmap::IndexMap;
use kdl::{KdlDocument, KdlNode};
use wmds_units::{Dim, Quantity};

use crate::{
    Ctx, SchemaError, SchemaErrors, child, children, first_positional_string, prop, prop_string,
    value_to_string,
};

/// kg/m^3
fn density_dim() -> Dim {
    Dim {
        m: -3,
        kg: 1,
        ..Dim::NONE
    }
}

/// currency per kg
fn cost_dim() -> Dim {
    Dim {
        kg: -1,
        cur: 1,
        ..Dim::NONE
    }
}

#[derive(Debug, Clone)]
pub struct MaterialDef {
    pub id: String,
    pub version: String,
    pub description: String,
    /// metal, cfrp, gfrp, polymer, elastomer, glass, foam, wood, ...
    pub family: String,
    /// How it is supplied: pultrusion, extrusion, sheet, tube, casting, prepreg, ...
    pub form: String,
    pub density: Quantity,
    pub elastic: Elastic,
    pub strength: IndexMap<String, Quantity>,
    pub environment: IndexMap<String, String>,
    /// Cost per kilogram.
    pub cost: Option<Quantity>,
    /// Where the numbers came from. Empty means nobody has said, which is worth knowing.
    pub source: String,
    pub note: String,
    /// Per-solver material cards, keyed by solver name.
    pub solvers: IndexMap<String, IndexMap<String, String>>,
    /// How this material behaves when it is crushed, which is a different question from how
    /// strong it is. A carbon tube absorbs several times what steel does per kilogram, and only
    /// if it is triggered so that it fragments progressively instead of splitting in half.
    pub crush: Option<Crush>,
}

#[derive(Debug, Clone)]
pub struct Crush {
    /// Specific energy absorption: joules per kilogram of material actually crushed.
    pub sea: Quantity,
    /// How much of the available length can be consumed before the debris packs solid.
    pub efficiency: f64,
    /// The initial peak force, as a multiple of the steady crush force. A structure with no
    /// trigger peaks hard and then drops, and that peak is what hurts the occupants.
    pub trigger_ratio: f64,
    /// Whether progressive crush has been demonstrated on a real coupon. A composite that has
    /// not been tested does not get to claim these numbers.
    pub validated: bool,
}

#[derive(Debug, Clone, Default)]
pub enum Elastic {
    #[default]
    Unknown,
    Isotropic {
        e: Quantity,
        nu: f64,
    },
    /// Orthotropic in the 1-2 plane, which is what a laminate or a pultrusion needs.
    Orthotropic {
        e1: Quantity,
        e2: Quantity,
        g12: Quantity,
        nu12: f64,
    },
}

impl MaterialDef {
    /// Density in kg/m^3.
    pub fn density_si(&self) -> f64 {
        self.density.value
    }
}

/// Parse a `.mat.kdl` file, which may hold several materials.
pub fn parse_materials(name: &str, src: &str) -> Result<Vec<MaterialDef>, SchemaErrors> {
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
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in doc.nodes() {
        if node.name().value() != "material" {
            ctx.err(
                node,
                format!(
                    "unexpected node `{}`; expected `material`",
                    node.name().value()
                ),
            );
            continue;
        }
        if let Some(m) = parse_one(&mut ctx, node) {
            if !seen.insert(m.id.clone()) {
                ctx.err(node, format!("duplicate material `{}`", m.id));
            }
            out.push(m);
        }
    }
    if out.is_empty() && ctx.errors.is_empty() {
        ctx.errors.push(SchemaError {
            msg: "file declares no materials".into(),
            span: None,
        });
    }
    if ctx.errors.is_empty() {
        Ok(out)
    } else {
        Err(SchemaErrors::new(name, src, ctx.errors))
    }
}

/// The `crush` block, which says how a material behaves when it is destroyed rather than loaded.
fn crush_block(ctx: &mut Ctx, node: &KdlNode) -> Option<Crush> {
    let n = child(node, "crush")?;
    let sea = quantity(
        ctx,
        n,
        "sea",
        Dim {
            m: 2,
            s: -2,
            ..Dim::NONE
        },
        "specific energy absorption, in joules per kilogram",
    )?;
    Some(Crush {
        sea,
        efficiency: prop(n, "efficiency").and_then(|v| v.as_float()).unwrap_or(0.7),
        trigger_ratio: prop(n, "trigger_ratio")
            .and_then(|v| v.as_float())
            .unwrap_or(1.6),
        validated: prop(n, "validated").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

fn quantity(ctx: &mut Ctx, node: &KdlNode, key: &str, want: Dim, what: &str) -> Option<Quantity> {
    let raw = prop(node, key)?;
    let text = value_to_string(raw);
    match Quantity::parse(&text) {
        Ok(q) if q.dim == want => Some(q),
        Ok(q) => {
            ctx.err(node, format!("`{key}` should be {what}, found {q}"));
            None
        }
        Err(e) => {
            ctx.err(node, format!("`{key}`: {e}"));
            None
        }
    }
}

fn parse_one(ctx: &mut Ctx, node: &KdlNode) -> Option<MaterialDef> {
    let id = match first_positional_string(node) {
        Some(s) => s,
        None => {
            ctx.err(
                node,
                "`material` needs an id, e.g. material \"steel/e355-tube\"",
            );
            return None;
        }
    };

    let density_node = child(node, "density");
    let density = match density_node.and_then(first_positional_string) {
        Some(text) => match Quantity::parse(&text) {
            Ok(q) if q.dim == density_dim() => q,
            Ok(q) => {
                ctx.err(
                    node,
                    format!("material `{id}`: density should be a density, found {q}"),
                );
                return None;
            }
            Err(e) => {
                ctx.err(node, format!("material `{id}`: density: {e}"));
                return None;
            }
        },
        None => {
            // Without a density a material cannot produce a mass, which is the one thing every
            // part needs from it.
            ctx.err(
                node,
                format!("material `{id}` needs a density, e.g. density \"7850 kg/m^3\""),
            );
            return None;
        }
    };

    let pressure = Dim::PRESSURE;
    let elastic = if let Some(n) = child(node, "orthotropic") {
        match (
            quantity(ctx, n, "e1", pressure, "a modulus"),
            quantity(ctx, n, "e2", pressure, "a modulus"),
            quantity(ctx, n, "g12", pressure, "a modulus"),
        ) {
            (Some(e1), Some(e2), Some(g12)) => Elastic::Orthotropic {
                e1,
                e2,
                g12,
                nu12: prop(n, "nu12").and_then(|v| v.as_float()).unwrap_or(0.3),
            },
            _ => Elastic::Unknown,
        }
    } else if let Some(n) = child(node, "isotropic") {
        match quantity(ctx, n, "e", pressure, "a modulus") {
            Some(e) => Elastic::Isotropic {
                e,
                nu: prop(n, "nu").and_then(|v| v.as_float()).unwrap_or(0.3),
            },
            None => Elastic::Unknown,
        }
    } else {
        Elastic::Unknown
    };

    let mut strength = IndexMap::new();
    if let Some(n) = child(node, "strength") {
        for e in n.entries() {
            let Some(k) = e.name() else { continue };
            let text = value_to_string(e.value());
            match Quantity::parse(&text) {
                Ok(q) if q.dim == pressure => {
                    strength.insert(k.value().to_string(), q);
                }
                _ => ctx.err_entry(e, format!("strength `{}` should be a stress", k.value())),
            }
        }
    }

    let mut environment = IndexMap::new();
    if let Some(n) = child(node, "environment") {
        for e in n.entries() {
            if let Some(k) = e.name() {
                environment.insert(k.value().to_string(), value_to_string(e.value()));
            }
        }
    }

    let cost = child(node, "cost")
        .and_then(first_positional_string)
        .and_then(|t| match Quantity::parse(&t) {
            Ok(q) if q.dim == cost_dim() => Some(q),
            Ok(q) => {
                ctx.err(
                    node,
                    format!("material `{id}`: cost should be per kilogram, found {q}"),
                );
                None
            }
            Err(e) => {
                ctx.err(node, format!("material `{id}`: cost: {e}"));
                None
            }
        });

    let mut solvers = IndexMap::new();
    for n in children(node)
        .iter()
        .filter(|n| n.name().value() == "solver")
    {
        let Some(sname) = first_positional_string(n) else {
            ctx.err(n, "solver needs a name");
            continue;
        };
        let mut fields = IndexMap::new();
        for e in n.entries() {
            if let Some(k) = e.name() {
                fields.insert(k.value().to_string(), value_to_string(e.value()));
            }
        }
        solvers.insert(sname, fields);
    }

    Some(MaterialDef {
        id,
        version: prop_string(node, "version").unwrap_or_else(|| "0.0.0".into()),
        description: child(node, "description")
            .and_then(first_positional_string)
            .unwrap_or_default(),
        family: child(node, "family")
            .and_then(first_positional_string)
            .unwrap_or_default(),
        form: child(node, "form")
            .and_then(first_positional_string)
            .unwrap_or_default(),
        density,
        elastic,
        strength,
        environment,
        crush: crush_block(ctx, node),
        cost,
        source: child(node, "source")
            .and_then(first_positional_string)
            .unwrap_or_default(),
        note: child(node, "note")
            .and_then(first_positional_string)
            .unwrap_or_default(),
        solvers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
material "cfrp/pultruded-ud-t700-epoxy" version="0.1.0" {
    description "Pultruded unidirectional T700 carbon in epoxy"
    family "cfrp"
    form "pultrusion"
    density "1550 kg/m^3"
    orthotropic e1="130 GPa" e2="8 GPa" g12="4.5 GPa" nu12=0.3
    strength xt="2000 MPa" xc="1100 MPa" yt="50 MPa"
    environment max_service_temp="120 C" uv="coat-required" galvanic="isolate-from-aluminium"
    cost "38 AUD/kg"
    source "PROVISIONAL: typical values for the fibre and resin system"
    solver "openradioss" law="LAW25" note="CRASURV formulation"
}

material "steel/e355-tube" {
    family "metal"
    density "7850 kg/m^3"
    isotropic e="210 GPa" nu=0.3
    cost "3.2 AUD/kg"
}
"#;

    #[test]
    fn parses_materials() {
        let m = parse_materials("x.mat.kdl", SRC)
            .map_err(|e| format!("{e:?}"))
            .unwrap();
        assert_eq!(m.len(), 2);
        let c = &m[0];
        assert_eq!(c.family, "cfrp");
        assert_eq!(c.density_si(), 1550.0);
        assert!(matches!(c.elastic, Elastic::Orthotropic { .. }));
        assert_eq!(c.strength["xt"].to_unit("MPa").unwrap(), 2000.0);
        assert_eq!(c.environment["uv"], "coat-required");
        assert_eq!(c.cost.unwrap().to_unit("AUD/kg").unwrap(), 38.0);
        assert_eq!(c.solvers["openradioss"]["law"], "LAW25");
        assert!(matches!(m[1].elastic, Elastic::Isotropic { .. }));
    }

    #[test]
    fn a_material_without_a_density_is_refused() {
        let bad = SRC.replace("    density \"7850 kg/m^3\"\n", "");
        let e = parse_materials("x.mat.kdl", &bad)
            .err()
            .expect("should fail");
        assert!(
            e.errors.iter().any(|x| x.msg.contains("needs a density")),
            "{:?}",
            e.errors
        );
    }

    #[test]
    fn a_density_that_is_not_a_density_is_refused() {
        let bad = SRC.replace("density \"1550 kg/m^3\"", "density \"1550 kg\"");
        let e = parse_materials("x.mat.kdl", &bad)
            .err()
            .expect("should fail");
        assert!(
            e.errors
                .iter()
                .any(|x| x.msg.contains("should be a density"))
        );
    }
}
