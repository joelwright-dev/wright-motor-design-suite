//! The `primitive` definition: header, params, variants, material, geometry, massprops, ports,
//! behaviour, manufacturing, compliance tags.

use indexmap::IndexMap;
use kdl::KdlNode;
use wmds_expr::Expr;

use crate::{
    Ctx, child, children, first_positional_string, positional, prop, prop_expr, prop_string,
    value_to_expr, value_to_string,
};

#[derive(Debug, Clone)]
pub struct PrimitiveDef {
    pub id: String,
    pub version: String,
    pub description: String,
    pub category: String,
    pub sub: Option<String>,
    pub params: Vec<ParamDef>,
    pub variants: Vec<VariantDef>,
    pub material: Option<String>,
    pub geometry: Vec<GeometryLevel>,
    pub massprops: MassPropsDef,
    pub ports: Vec<PortDef>,
    pub behaviour: Option<BehaviourDef>,
    pub manufacturing: Vec<MfgMethodDef>,
    pub compliance_tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub unit: Option<String>,
    pub default: Option<Expr>,
    pub min: Option<Expr>,
    pub max: Option<Expr>,
    /// A derived parameter: computed from other parameters, not user-settable.
    pub expr: Option<Expr>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GeometryLevel {
    pub level: String,
    pub features: Vec<Feature>,
}

/// One geometry feature such as `tube "front_leg" od=tube_od from="(...)" to="(...)"`.
#[derive(Debug, Clone)]
pub struct Feature {
    pub op: String,
    pub name: Option<String>,
    pub args: IndexMap<String, Expr>,
    pub positional: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub enum MassPropsDef {
    Computed,
    Declared {
        mass: Expr,
        cg: Option<Expr>,
        inertia: Option<Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum AxisSpec {
    /// `"x"`, `"-y"`, `"z"` ...
    Named(String),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct PortDef {
    pub name: String,
    pub port_type: String,
    pub at: Expr,
    pub axis: AxisSpec,
    /// Secondary (clocking) axis; optional.
    pub clock: Option<AxisSpec>,
    pub symmetry: Option<u32>,
    pub params: IndexMap<String, Expr>,
    pub load_rating: Option<Expr>,
    /// Snap to the chassis mount grid.
    pub grid: bool,
}

#[derive(Debug, Clone)]
pub struct BehaviourDef {
    pub kind: String,
    /// The behaviour block is kept verbatim for the behaviour-model crates to interpret.
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct MfgMethodDef {
    pub method: String,
    pub scale: Option<String>,
    pub exports: Vec<String>,
    pub cost: Option<CostDef>,
}

#[derive(Debug, Clone)]
pub struct CostDef {
    pub fixed: Option<Expr>,
    pub per_unit: Option<Expr>,
}

pub(crate) fn parse_primitive_node(ctx: &mut Ctx, node: &KdlNode) -> Option<PrimitiveDef> {
    let id = match first_positional_string(node) {
        Some(s) => s,
        None => {
            ctx.err(
                node,
                "`primitive` needs an id, e.g. primitive \"suspension/arms/lca\"",
            );
            return None;
        }
    };
    let version = prop_string(node, "version").unwrap_or_else(|| {
        ctx.err(node, "`primitive` needs version=\"x.y.z\"");
        String::new()
    });

    let description = child(node, "description")
        .and_then(first_positional_string)
        .unwrap_or_default();
    let (category, sub) = match child(node, "category") {
        Some(c) => (
            first_positional_string(c).unwrap_or_default(),
            prop_string(c, "sub"),
        ),
        None => {
            ctx.err(node, "missing `category`");
            (String::new(), None)
        }
    };

    // params
    let mut params = Vec::new();
    match child(node, "params") {
        Some(p) => {
            for n in children(p) {
                let name = n.name().value().to_string();
                if let Some(u) = prop_string(n, "unit")
                    && let Err(e) = wmds_units::Quantity::dim_of_unit(&u)
                {
                    ctx.err(n, format!("param `{name}`: {e}"));
                }
                let default = prop_expr(n, "default");
                let expr = prop_expr(n, "expr");
                if default.is_none() && expr.is_none() {
                    ctx.err(n, format!("param `{name}` needs a default= or an expr="));
                }
                if let Some(Expr::Str(s)) = &expr {
                    ctx.err(
                        n,
                        format!("param `{name}`: expr `{s}` is not a valid expression"),
                    );
                }
                params.push(ParamDef {
                    name,
                    unit: prop_string(n, "unit"),
                    default,
                    min: prop_expr(n, "min"),
                    max: prop_expr(n, "max"),
                    expr,
                    doc: prop_string(n, "doc"),
                });
            }
        }
        None => ctx.err(node, "missing `params` block (it may be empty)"),
    }

    // variants
    let mut variants = Vec::new();
    if let Some(v) = child(node, "variants") {
        for n in children(v) {
            let options: Vec<String> = positional(n).iter().map(|x| value_to_string(x)).collect();
            if options.is_empty() {
                ctx.err(n, "variant needs at least one option");
            }
            variants.push(VariantDef {
                name: n.name().value().to_string(),
                options,
            });
        }
    }

    let material = child(node, "material").and_then(first_positional_string);

    // geometry levels
    let mut geometry = Vec::new();
    for g in children(node)
        .iter()
        .filter(|n| n.name().value() == "geometry")
    {
        let level = prop_string(g, "level").unwrap_or_else(|| "display".to_string());
        let mut features = Vec::new();
        for f in children(g) {
            let mut args = IndexMap::new();
            let mut pos = Vec::new();
            for e in f.entries() {
                match e.name() {
                    Some(k) => {
                        args.insert(k.value().to_string(), value_to_expr(e.value()));
                    }
                    None => pos.push(value_to_expr(e.value())),
                }
            }
            // First positional string is the feature name by convention.
            let name = match pos.first() {
                Some(Expr::Str(s)) => Some(s.clone()),
                Some(Expr::Path(p)) if p.len() == 1 => Some(p[0].clone()),
                _ => None,
            };
            if name.is_some() {
                pos.remove(0);
            }
            features.push(Feature {
                op: f.name().value().to_string(),
                name,
                args,
                positional: pos,
            });
        }
        geometry.push(GeometryLevel { level, features });
    }
    if geometry.is_empty() {
        ctx.err(node, "missing `geometry` block");
    }

    // massprops
    let massprops = match child(node, "massprops") {
        Some(m) => {
            let computed = prop(m, "computed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let declared = child(m, "declared").or(
                if first_positional_string(m).as_deref() == Some("declared") {
                    Some(m)
                } else {
                    None
                },
            );
            match (computed, declared) {
                (true, _) => MassPropsDef::Computed,
                (false, Some(d)) => match prop_expr(d, "mass") {
                    Some(mass) => MassPropsDef::Declared {
                        mass,
                        cg: prop_expr(d, "cg"),
                        inertia: prop_expr(d, "inertia"),
                    },
                    None => {
                        ctx.err(d, "declared massprops need mass=");
                        MassPropsDef::Computed
                    }
                },
                (false, None) => {
                    ctx.err(m, "massprops must be `computed=#true` or `declared mass=... cg=... inertia=...`");
                    MassPropsDef::Computed
                }
            }
        }
        None => {
            ctx.err(node, "missing `massprops`");
            MassPropsDef::Computed
        }
    };

    // ports
    let mut ports = Vec::new();
    match child(node, "ports") {
        Some(p) => {
            for n in children(p) {
                if n.name().value() != "port" {
                    ctx.err(n, "only `port` nodes are allowed inside `ports`");
                    continue;
                }
                let name = first_positional_string(n).unwrap_or_else(|| {
                    ctx.err(n, "port needs a name");
                    String::new()
                });
                let port_type = prop_string(n, "type").unwrap_or_else(|| {
                    ctx.err(n, format!("port `{name}` needs type=\"...\""));
                    String::new()
                });
                let at = prop_expr(n, "at").unwrap_or_else(|| {
                    ctx.err(n, format!("port `{name}` needs at=\"(x, y, z)\""));
                    Expr::Tuple(vec![])
                });
                let axis = axis_spec(prop(n, "axis")).unwrap_or_else(|| {
                    ctx.err(
                        n,
                        format!("port `{name}` needs axis=\"x|y|z|-x|...\" or a tuple"),
                    );
                    AxisSpec::Named("z".into())
                });
                let clock = axis_spec(prop(n, "clock"));
                let symmetry = prop(n, "symmetry")
                    .and_then(|v| v.as_integer())
                    .map(|i| i as u32);
                let mut params = IndexMap::new();
                if let Some(pp) = child(n, "params") {
                    for e in pp.entries() {
                        if let Some(k) = e.name() {
                            params.insert(k.value().to_string(), value_to_expr(e.value()));
                        }
                    }
                }
                ports.push(PortDef {
                    name,
                    port_type,
                    at,
                    axis,
                    clock,
                    symmetry,
                    params,
                    load_rating: prop_expr(n, "load_rating"),
                    grid: prop(n, "grid").and_then(|v| v.as_bool()).unwrap_or(false),
                });
            }
        }
        None => ctx.err(node, "missing `ports` block (it may be empty)"),
    }

    // behaviour
    let behaviour = child(node, "behaviour").and_then(|b| {
        let kind = first_positional_string(b).unwrap_or_else(|| "none".into());
        if kind == "none" {
            None
        } else {
            Some(BehaviourDef {
                kind,
                raw: b.to_string(),
            })
        }
    });

    // manufacturing
    let mut manufacturing = Vec::new();
    match child(node, "manufacturing") {
        Some(m) => {
            for n in children(m) {
                if n.name().value() != "method" {
                    ctx.err(n, "only `method` nodes are allowed inside `manufacturing`");
                    continue;
                }
                let method = first_positional_string(n).unwrap_or_else(|| {
                    ctx.err(n, "method needs a name");
                    String::new()
                });
                let exports = children(n)
                    .iter()
                    .filter(|c| c.name().value() == "export")
                    .filter_map(first_positional_string)
                    .collect();
                let cost = child(n, "cost").map(|c| CostDef {
                    fixed: prop_expr(c, "fixed"),
                    per_unit: prop_expr(c, "per_unit"),
                });
                manufacturing.push(MfgMethodDef {
                    method,
                    scale: prop_string(n, "scale"),
                    exports,
                    cost,
                });
            }
        }
        None => ctx.err(node, "missing `manufacturing` block"),
    }
    if manufacturing.is_empty() {
        ctx.err(node, "`manufacturing` needs at least one `method`");
    }

    let compliance_tags = child(node, "compliance")
        .map(|c| positional(c).iter().map(|v| value_to_string(v)).collect())
        .unwrap_or_default();

    Some(PrimitiveDef {
        id,
        version,
        description,
        category,
        sub,
        params,
        variants,
        material,
        geometry,
        massprops,
        ports,
        behaviour,
        manufacturing,
        compliance_tags,
    })
}

fn axis_spec(v: Option<&kdl::KdlValue>) -> Option<AxisSpec> {
    let v = v?;
    if let Some(s) = v.as_string() {
        let t = s.trim();
        if matches!(t, "x" | "y" | "z" | "-x" | "-y" | "-z" | "+x" | "+y" | "+z") {
            return Some(AxisSpec::Named(t.trim_start_matches('+').to_string()));
        }
    }
    Some(AxisSpec::Expr(value_to_expr(v)))
}

#[cfg(test)]
mod tests {
    use crate::parse_primitive;

    const MINIMAL: &str = r#"
primitive "test/box" version="0.1.0" {
    description "A box"
    category "body" sub="panel"
    params {
        w unit="mm" default=100 min=10 max=1000
        h unit="mm" default=50
        area unit="mm^2" expr="w * h"
    }
    material "steel/generic"
    geometry level="manufacture" {
        box "b" size="(w, h, 5 mm)"
    }
    massprops computed=#true
    ports {
        port "face" type="bolt.pattern" at="(0 mm, 0 mm, 0 mm)" axis="z" symmetry=4 {
            params count=4 pcd="100 mm" thread="M12"
        }
    }
    manufacturing {
        method "flat-cut" scale="1..*" { export "dxf"; cost fixed="5 AUD" per_unit="0.01 AUD/mm^2 * area" }
    }
    compliance "structural"
}
"#;

    #[test]
    fn parses_minimal_primitive() {
        let p = parse_primitive("test.prim.kdl", MINIMAL)
            .map_err(|e| format!("{e:?}"))
            .unwrap();
        assert_eq!(p.id, "test/box");
        assert_eq!(p.params.len(), 3);
        assert_eq!(p.ports[0].symmetry, Some(4));
        assert_eq!(p.ports[0].params.len(), 3);
        assert_eq!(p.manufacturing[0].exports, vec!["dxf"]);
        assert_eq!(p.compliance_tags, vec!["structural"]);
    }

    #[test]
    fn reports_missing_blocks_with_names() {
        let src = r#"primitive "x/y" version="0.0.1" { category "body" }"#;
        let err = parse_primitive("bad.prim.kdl", src).expect_err("should fail");
        let msgs: Vec<String> = err.errors.iter().map(|e| e.msg.clone()).collect();
        assert!(msgs.iter().any(|m| m.contains("params")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("geometry")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("massprops")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("ports")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("manufacturing")), "{msgs:?}");
    }

    #[test]
    fn rejects_unknown_units() {
        let src = MINIMAL.replace("unit=\"mm\" default=100", "unit=\"cubits\" default=100");
        let err = parse_primitive("bad.prim.kdl", &src).expect_err("should fail");
        assert!(err.errors.iter().any(|e| e.msg.contains("cubits")));
    }
}
