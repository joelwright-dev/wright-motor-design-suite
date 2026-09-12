//! Assembly and vehicle definitions (`.asm.kdl`, `.veh.kdl`).
//!
//! An assembly is a set of named instances joined by mates. A vehicle is an assembly with
//! metadata (category, market, rule packs) and, usually, a chassis block that a generator
//! expands into instances and mates before placement.

use indexmap::IndexMap;
use kdl::{KdlDocument, KdlNode};
use wmds_expr::Expr;

use crate::{
    Ctx, Dof, ParamDef, SchemaError, SchemaErrors, Stage, child, children, first_positional_string,
    parse_params_block, prop_expr, prop_string, value_to_expr, value_to_string,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyKind {
    Assembly,
    Vehicle,
}

#[derive(Debug, Clone)]
pub struct AssemblyDef {
    pub kind: AssemblyKind,
    pub id: String,
    pub version: String,
    pub description: String,
    pub params: Vec<ParamDef>,
    pub instances: Vec<InstanceDef>,
    pub mates: Vec<MateDef>,
    /// Instance held fixed at the origin. Defaults to the first instance, or the chassis.
    pub root: Option<String>,
    pub exports: Vec<PortExport>,
    pub point_masses: Vec<PointMassDef>,
    /// Vehicle metadata; present only when `kind` is `Vehicle`.
    pub vehicle: Option<VehicleMeta>,
    /// Chassis to generate and include; vehicles only.
    pub chassis: Option<ChassisRef>,
}

#[derive(Debug, Clone, Default)]
pub struct VehicleMeta {
    /// ADR vehicle category: MA, MB, MC, NA, ...
    pub category: String,
    pub markets: Vec<String>,
    pub rule_packs: Vec<String>,
}

/// A chassis to generate into the vehicle.
#[derive(Debug, Clone)]
pub struct ChassisRef {
    pub system: String,
    pub configuration: String,
    pub width: String,
    pub rail_section: String,
    /// Section kind -> chosen length expression.
    pub section_lengths: IndexMap<String, Expr>,
    /// Instance id prefix for the generated parts.
    pub id: String,
}

#[derive(Debug, Clone)]
pub enum InstanceSource {
    Primitive(String),
    Assembly(String),
}

#[derive(Debug, Clone)]
pub struct InstanceDef {
    pub id: String,
    pub source: InstanceSource,
    pub version: Option<String>,
    pub params: IndexMap<String, Expr>,
    pub variants: IndexMap<String, String>,
    /// Free placement, used only when an instance cannot be reached through mates. Reported.
    pub placement: Option<Placement>,
}

/// An explicit position, as an escape hatch from the mate graph.
#[derive(Debug, Clone)]
pub struct Placement {
    pub at: Expr,
    /// Rotations in degrees about x, y, z applied in that order.
    pub rotate: Option<Expr>,
    pub mirror: Option<String>,
    pub justification: String,
}

#[derive(Debug, Clone)]
pub struct PortRef {
    pub instance: String,
    pub port: String,
}

impl std::fmt::Display for PortRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.instance, self.port)
    }
}

impl PortRef {
    fn parse(text: &str) -> Option<PortRef> {
        let (i, p) = text.rsplit_once('.')?;
        if i.is_empty() || p.is_empty() {
            return None;
        }
        Some(PortRef { instance: i.to_string(), port: p.to_string() })
    }
}

#[derive(Debug, Clone)]
pub struct MateDef {
    pub id: String,
    pub a: PortRef,
    pub b: PortRef,
    /// Overrides the port type's degree of freedom.
    pub dof: Option<Dof>,
    /// Overrides the port type's stage.
    pub stage: Option<Stage>,
    /// Small transform applied at the joint (shim, adjustment).
    pub offset: Option<Expr>,
    /// Rotation about the mating axis, in degrees. Used for clocking.
    pub clock: Option<Expr>,
    pub fasteners: Option<FastenerDef>,
}

#[derive(Debug, Clone, Default)]
pub struct FastenerDef {
    pub kind: String,
    pub size: String,
    pub grade: String,
    pub quantity: u32,
    pub torque: Option<Expr>,
    pub nut: Option<String>,
    pub washer: Option<String>,
    pub thread_locker: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PortExport {
    pub source: PortRef,
    pub name: String,
}

/// A mass with no geometry: occupants, payload, fluids, or a component modelled as a lump.
#[derive(Debug, Clone)]
pub struct PointMassDef {
    pub id: String,
    pub mass: Expr,
    pub at: Expr,
    /// Which load state this mass belongs to: `kerb`, `laden`, `gvm`.
    pub state: String,
}

/// Parse an `.asm.kdl` or `.veh.kdl` file.
pub fn parse_assembly(name: &str, src: &str) -> Result<AssemblyDef, SchemaErrors> {
    let doc: KdlDocument = match src.parse() {
        Ok(d) => d,
        Err(e) => {
            let errors = e
                .diagnostics
                .iter()
                .map(|d| SchemaError { msg: d.to_string(), span: Some(d.span) })
                .collect();
            return Err(SchemaErrors::new(name, src, errors));
        }
    };
    let mut ctx = Ctx { errors: Vec::new() };
    let roots: Vec<&KdlNode> = doc
        .nodes()
        .iter()
        .filter(|n| matches!(n.name().value(), "assembly" | "vehicle"))
        .collect();
    let def = match roots.as_slice() {
        [one] => parse_root(&mut ctx, one),
        [] => {
            ctx.errors.push(SchemaError {
                msg: "file has no `assembly` or `vehicle` node".into(),
                span: None,
            });
            None
        }
        _ => {
            ctx.err(roots[1], "only one `assembly` or `vehicle` node is allowed per file");
            None
        }
    };
    match def {
        Some(d) if ctx.errors.is_empty() => Ok(d),
        _ => Err(SchemaErrors::new(name, src, ctx.errors)),
    }
}

fn parse_root(ctx: &mut Ctx, node: &KdlNode) -> Option<AssemblyDef> {
    let kind = if node.name().value() == "vehicle" { AssemblyKind::Vehicle } else { AssemblyKind::Assembly };
    let id = match first_positional_string(node) {
        Some(s) => s,
        None => {
            ctx.err(node, "needs an id, e.g. assembly \"corner/front-left\"");
            return None;
        }
    };
    let version = prop_string(node, "version").unwrap_or_else(|| "0.0.0".to_string());
    let description = child(node, "description").and_then(first_positional_string).unwrap_or_default();
    let params = child(node, "params").map(|p| parse_params_block(ctx, p)).unwrap_or_default();

    // instances
    let mut instances = Vec::new();
    if let Some(block) = child(node, "instances") {
        for n in children(block) {
            if n.name().value() != "instance" {
                ctx.err(n, "only `instance` nodes are allowed inside `instances`");
                continue;
            }
            let Some(iid) = first_positional_string(n) else {
                ctx.err(n, "instance needs an id");
                continue;
            };
            let source = match (prop_string(n, "primitive"), prop_string(n, "assembly")) {
                (Some(p), None) => InstanceSource::Primitive(p),
                (None, Some(a)) => InstanceSource::Assembly(a),
                (Some(_), Some(_)) => {
                    ctx.err(n, format!("instance `{iid}` sets both primitive= and assembly="));
                    continue;
                }
                (None, None) => {
                    ctx.err(n, format!("instance `{iid}` needs primitive= or assembly="));
                    continue;
                }
            };
            let mut params = IndexMap::new();
            if let Some(s) = child(n, "set") {
                for e in s.entries() {
                    if let Some(k) = e.name() {
                        params.insert(k.value().to_string(), value_to_expr(e.value()));
                    }
                }
            }
            let mut variants = IndexMap::new();
            if let Some(v) = child(n, "variant") {
                for e in v.entries() {
                    if let Some(k) = e.name() {
                        variants.insert(k.value().to_string(), value_to_string(e.value()));
                    }
                }
            }
            let placement = child(n, "place").map(|p| Placement {
                at: prop_expr(p, "at").unwrap_or(Expr::Tuple(vec![])),
                rotate: prop_expr(p, "rotate"),
                mirror: prop_string(p, "mirror"),
                justification: prop_string(p, "because").unwrap_or_default(),
            });
            if let Some(pl) = &placement {
                if pl.justification.is_empty() {
                    ctx.err(n, format!("instance `{iid}`: free placement needs because=\"...\" explaining why it is not mated"));
                }
            }
            instances.push(InstanceDef {
                id: iid,
                source,
                version: prop_string(n, "version"),
                params,
                variants,
                placement,
            });
        }
    }

    // mates
    let mut mates = Vec::new();
    if let Some(block) = child(node, "mates") {
        for n in children(block) {
            if n.name().value() != "mate" {
                ctx.err(n, "only `mate` nodes are allowed inside `mates`");
                continue;
            }
            let mid = first_positional_string(n).unwrap_or_else(|| format!("mate{}", mates.len() + 1));
            let (Some(a), Some(b)) = (prop_string(n, "a"), prop_string(n, "b")) else {
                ctx.err(n, format!("mate `{mid}` needs a=\"instance.port\" and b=\"instance.port\""));
                continue;
            };
            let (Some(a), Some(b)) = (PortRef::parse(&a), PortRef::parse(&b)) else {
                ctx.err(n, format!("mate `{mid}`: port references must be `instance.port`"));
                continue;
            };
            let dof = match prop_string(n, "dof") {
                Some(s) => match Dof::parse(&s) {
                    Some(d) => Some(d),
                    None => {
                        ctx.err(n, format!("mate `{mid}`: unknown dof `{s}`"));
                        None
                    }
                },
                None => None,
            };
            let stage = match child(n, "stage").and_then(first_positional_string).or_else(|| prop_string(n, "stage")) {
                Some(s) => match Stage::parse(&s) {
                    Some(v) => Some(v),
                    None => {
                        ctx.err(n, format!("mate `{mid}`: unknown stage `{s}`"));
                        None
                    }
                },
                None => None,
            };
            let fasteners = child(n, "fasteners").map(|f| FastenerDef {
                kind: prop_string(f, "kind").unwrap_or_else(|| "bolt".into()),
                size: prop_string(f, "size").unwrap_or_default(),
                grade: prop_string(f, "grade").unwrap_or_default(),
                quantity: prop_string(f, "qty").and_then(|q| q.parse().ok()).unwrap_or(1),
                torque: prop_expr(f, "torque"),
                nut: prop_string(f, "nut"),
                washer: prop_string(f, "washer"),
                thread_locker: prop_string(f, "thread_locker"),
            });
            mates.push(MateDef {
                id: mid,
                a,
                b,
                dof,
                stage,
                offset: prop_expr(n, "offset"),
                clock: prop_expr(n, "clock"),
                fasteners,
            });
        }
    }

    let root = child(node, "root").and_then(first_positional_string);

    let mut exports = Vec::new();
    if let Some(block) = child(node, "ports") {
        for n in children(block) {
            if n.name().value() != "export" {
                ctx.err(n, "only `export` nodes are allowed inside an assembly's `ports`");
                continue;
            }
            let Some(src) = first_positional_string(n).and_then(|s| PortRef::parse(&s)) else {
                ctx.err(n, "export needs \"instance.port\"");
                continue;
            };
            let name = prop_string(n, "as").unwrap_or_else(|| src.port.clone());
            exports.push(PortExport { source: src, name });
        }
    }

    let mut point_masses = Vec::new();
    if let Some(block) = child(node, "masses") {
        for n in children(block) {
            if n.name().value() != "mass" {
                ctx.err(n, "only `mass` nodes are allowed inside `masses`");
                continue;
            }
            let mid = first_positional_string(n).unwrap_or_else(|| format!("mass{}", point_masses.len() + 1));
            let (Some(mass), Some(at)) = (prop_expr(n, "value"), prop_expr(n, "at")) else {
                ctx.err(n, format!("mass `{mid}` needs value= and at="));
                continue;
            };
            point_masses.push(PointMassDef {
                id: mid,
                mass,
                at,
                state: prop_string(n, "state").unwrap_or_else(|| "laden".into()),
            });
        }
    }

    // vehicle metadata and chassis
    let mut vehicle = None;
    let mut chassis = None;
    if kind == AssemblyKind::Vehicle {
        let meta = VehicleMeta {
            category: child(node, "category").and_then(first_positional_string).unwrap_or_default(),
            markets: child(node, "markets")
                .map(|m| crate::positional(m).iter().map(|v| value_to_string(v)).collect())
                .unwrap_or_default(),
            rule_packs: child(node, "rule_packs")
                .map(|m| crate::positional(m).iter().map(|v| value_to_string(v)).collect())
                .unwrap_or_default(),
        };
        if meta.category.is_empty() {
            ctx.err(node, "vehicle needs a `category` (MA, MB, MC, NA, ...)");
        }
        vehicle = Some(meta);

        if let Some(c) = child(node, "chassis") {
            let system = prop_string(c, "system").or_else(|| first_positional_string(c));
            let Some(system) = system else {
                ctx.err(c, "chassis needs system=\"mcds-v1\"");
                return finish(kind, id, version, description, params, instances, mates, root, exports, point_masses, vehicle, None);
            };
            let mut section_lengths = IndexMap::new();
            for s in children(c).iter().filter(|n| n.name().value() == "section") {
                let Some(kindname) = first_positional_string(s) else {
                    ctx.err(s, "section needs a kind, e.g. section \"front\" length=\"1100 mm\"");
                    continue;
                };
                match prop_expr(s, "length") {
                    Some(l) => {
                        section_lengths.insert(kindname, l);
                    }
                    None => ctx.err(s, format!("section `{kindname}` needs length=")),
                }
            }
            chassis = Some(ChassisRef {
                system,
                configuration: child(c, "configuration").and_then(first_positional_string).unwrap_or_else(|| "full-length".into()),
                width: child(c, "width").and_then(first_positional_string).unwrap_or_else(|| "standard".into()),
                rail_section: child(c, "rail_section").and_then(first_positional_string).unwrap_or_default(),
                section_lengths,
                id: prop_string(c, "id").unwrap_or_else(|| "chassis".into()),
            });
        }
    }

    // Referential checks.
    let ids: Vec<&str> = instances.iter().map(|i| i.id.as_str()).collect();
    let chassis_prefix = chassis.as_ref().map(|c| c.id.clone());
    let known = |inst: &str| -> bool {
        ids.contains(&inst) || chassis_prefix.as_ref().is_some_and(|p| inst == p || inst.starts_with(&format!("{p}.")))
    };
    for m in &mates {
        for p in [&m.a, &m.b] {
            if !known(&p.instance) {
                ctx.err(node, format!("mate `{}` refers to unknown instance `{}`", m.id, p.instance));
            }
        }
    }
    for e in &exports {
        if !known(&e.source.instance) {
            ctx.err(node, format!("export `{}` refers to unknown instance `{}`", e.name, e.source.instance));
        }
    }
    if let Some(r) = &root {
        if !known(r) {
            ctx.err(node, format!("root `{r}` is not an instance"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for i in &instances {
        if !seen.insert(i.id.clone()) {
            ctx.err(node, format!("duplicate instance id `{}`", i.id));
        }
    }

    finish(kind, id, version, description, params, instances, mates, root, exports, point_masses, vehicle, chassis)
}

#[allow(clippy::too_many_arguments)]
fn finish(
    kind: AssemblyKind,
    id: String,
    version: String,
    description: String,
    params: Vec<ParamDef>,
    instances: Vec<InstanceDef>,
    mates: Vec<MateDef>,
    root: Option<String>,
    exports: Vec<PortExport>,
    point_masses: Vec<PointMassDef>,
    vehicle: Option<VehicleMeta>,
    chassis: Option<ChassisRef>,
) -> Option<AssemblyDef> {
    Some(AssemblyDef {
        kind,
        id,
        version,
        description,
        params,
        instances,
        mates,
        root,
        exports,
        point_masses,
        vehicle,
        chassis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASM: &str = r#"
assembly "corner/front-left" version="0.1.0" {
    description "Front left corner"
    params { track_half unit="mm" default=750 }
    instances {
        instance "lca" primitive="suspension/arms/lca-wishbone-a" version="1.2.0" {
            set span="380 mm" reach="320 mm"
            variant hand="left"
        }
        instance "upright" primitive="suspension/uprights/upright-std"
    }
    mates {
        mate "arm_to_upright" a="lca.balljoint" b="upright.lca_socket" {
            fasteners kind="bolt" size="M14" grade="10.9" qty="1" torque="110 Nm" nut="nyloc"
            stage "kit"
        }
    }
    root "upright"
    ports {
        export "upright.hub" as="hub"
    }
    masses {
        mass "brake_fluid" value="0.4 kg" at="(0 mm, 0 mm, 100 mm)" state="kerb"
    }
}
"#;

    #[test]
    fn parses_assembly() {
        let a = parse_assembly("x.asm.kdl", ASM).map_err(|e| format!("{e:?}")).unwrap();
        assert_eq!(a.kind, AssemblyKind::Assembly);
        assert_eq!(a.instances.len(), 2);
        assert_eq!(a.instances[0].variants["hand"], "left");
        assert_eq!(a.mates.len(), 1);
        assert_eq!(a.mates[0].a.to_string(), "lca.balljoint");
        assert_eq!(a.mates[0].stage, Some(Stage::Kit));
        let f = a.mates[0].fasteners.as_ref().unwrap();
        assert_eq!(f.size, "M14");
        assert_eq!(f.quantity, 1);
        assert_eq!(a.root.as_deref(), Some("upright"));
        assert_eq!(a.exports[0].name, "hub");
        assert_eq!(a.point_masses[0].state, "kerb");
    }

    #[test]
    fn catches_dangling_references() {
        let bad = ASM.replace("b=\"upright.lca_socket\"", "b=\"nosuch.port\"");
        let e = parse_assembly("x.asm.kdl", &bad).err().expect("should fail");
        assert!(e.errors.iter().any(|x| x.msg.contains("nosuch")), "{:?}", e.errors);
    }

    #[test]
    fn free_placement_requires_justification() {
        let bad = ASM.replace(
            "instance \"upright\" primitive=\"suspension/uprights/upright-std\"",
            "instance \"upright\" primitive=\"suspension/uprights/upright-std\" { place at=\"(0 mm, 0 mm, 0 mm)\" }",
        );
        let e = parse_assembly("x.asm.kdl", &bad).err().expect("should fail");
        assert!(e.errors.iter().any(|x| x.msg.contains("because")), "{:?}", e.errors);
    }

    const VEH: &str = r#"
vehicle "reference/city-ev" version="0.1.0" {
    description "Reference vehicle"
    category "MA"
    markets "AU"
    rule_packs "wright-internal"
    chassis system="mcds-v1" id="chassis" {
        configuration "2/3-length"
        width "narrow"
        rail_section "120x60"
        section "front" length="1100 mm"
        section "central" length="1900 mm"
    }
    instances {
        instance "battery" primitive="energy/battery/pack-40kwh"
    }
    mates {
        mate "battery_mount" a="battery.mount_fl" b="chassis.station_left_4"
    }
}
"#;

    #[test]
    fn parses_vehicle_with_chassis() {
        let v = parse_assembly("x.veh.kdl", VEH).map_err(|e| format!("{e:?}")).unwrap();
        assert_eq!(v.kind, AssemblyKind::Vehicle);
        assert_eq!(v.vehicle.as_ref().unwrap().category, "MA");
        let c = v.chassis.as_ref().unwrap();
        assert_eq!(c.system, "mcds-v1");
        assert_eq!(c.configuration, "2/3-length");
        assert_eq!(c.width, "narrow");
        assert_eq!(c.section_lengths.len(), 2);
        // A mate onto a generated chassis port must not be reported as dangling.
        assert_eq!(v.mates.len(), 1);
    }
}
