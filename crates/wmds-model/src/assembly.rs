//! Resolving an assembly: instantiate the parts, check every mate, and solve where each part
//! sits.
//!
//! Placement is derived, never authored. One instance is held fixed (the root), and every other
//! instance is positioned by walking outward along the mate graph. That is what makes a change
//! to the chassis ripple through the whole vehicle instead of leaving parts floating where a
//! designer last dragged them.
//!
//! Sub-assemblies are resolved first, in their own coordinates, and then placed as a unit
//! through the ports they export. Final placements are flattened to a single list with dotted
//! ids, so everything downstream (geometry, mass, bills of materials) walks one flat structure.

use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::IndexMap;
use thiserror::Error;
use wmds_expr::{Env, Value, eval};
use wmds_schema::{
    AssemblyDef, AssemblyKind, Dof, FastenerDef, InstanceSource, ParamKind, PortTypeDef, Stage,
};
use wmds_units::Quantity;

use crate::library::Library;
use crate::transform::{Frame, MateAxis, Transform, solve_mate};
use crate::{ModelError, Overrides, ResolvedPort, ResolvedPrimitive, as_length_vec3, resolve, resolve_params};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum MateError {
    #[error("instance `{0}` has no port `{1}`")]
    NoSuchPort(String, String),
    #[error("port type `{0}` is not in the registry")]
    UnknownPortType(String),
    #[error("`{a}` ({a_type}) is not compatible with `{b}` ({b_type})")]
    Incompatible { a: String, a_type: String, b: String, b_type: String },
    #[error("`{a}` and `{b}` are compatible types but their parameters do not match: {reason}")]
    ParamsDiffer { a: String, b: String, reason: String },
    #[error("missing required port parameter `{0}` on `{1}`")]
    MissingParam(String, String),
}

/// How an instance came to be where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacedBy {
    /// Held fixed at the assembly origin.
    Root,
    /// Positioned by this mate.
    Mate(String),
    /// Positioned explicitly, with the justification recorded.
    Free(String),
    /// Not reachable through mates; left at the origin and reported.
    Unreached,
}

#[derive(Debug, Clone)]
pub struct PlacedInstance {
    /// Dotted path, e.g. `corner_fl.lca`.
    pub id: String,
    /// Which primitive this is an instance of.
    pub source_id: String,
    pub primitive: ResolvedPrimitive,
    pub placement: Transform,
    pub placed_by: PlacedBy,
}

impl PlacedInstance {
    /// A port's frame in assembly coordinates.
    pub fn port_world(&self, port: &str) -> Option<Transform> {
        let p = self.primitive.ports.iter().find(|p| p.name == port)?;
        Some(port_frame(p).then(&self.placement))
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedMate {
    pub id: String,
    pub a: String,
    pub a_port: String,
    pub b: String,
    pub b_port: String,
    pub dof: Dof,
    pub stage: Stage,
    pub fasteners: Option<FastenerDef>,
    /// `Ok` when the two ports may legally be joined.
    pub compatible: Result<(), MateError>,
}

#[derive(Debug, Clone)]
pub struct PointMass {
    pub id: String,
    pub mass: Quantity,
    pub at: [Quantity; 3],
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedAssembly {
    pub id: String,
    pub version: String,
    pub kind: AssemblyKind,
    pub instances: Vec<PlacedInstance>,
    pub mates: Vec<ResolvedMate>,
    pub point_masses: Vec<PointMass>,
    /// Ports this assembly offers to a parent: exported name -> (instance id, port name).
    pub exports: IndexMap<String, (String, String)>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ResolvedAssembly {
    pub fn instance(&self, id: &str) -> Option<&PlacedInstance> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty() && self.mates.iter().all(|m| m.compatible.is_ok())
    }
}

/// The port frame of a resolved port, in its primitive's coordinates.
pub fn port_frame(p: &ResolvedPort) -> Transform {
    Frame {
        origin: [p.origin[0].value, p.origin[1].value, p.origin[2].value],
        axis: p.axis,
        clock: p.clock,
    }
    .to_transform()
}

/// One thing that can be mated: a primitive instance, or a whole sub-assembly.
struct Unit {
    id: String,
    source_id: String,
    /// Instances contributed by this unit, in unit-local coordinates.
    parts: Vec<PlacedInstance>,
    /// Port name -> (index into `parts`, port name on that part).
    ports: IndexMap<String, (usize, String)>,

    free: Option<(Transform, String)>,
}

impl Unit {
    /// A port's frame in unit-local coordinates.
    fn port_local(&self, port: &str) -> Option<Transform> {
        let (idx, name) = self.ports.get(port)?;
        let part = &self.parts[*idx];
        let p = part.primitive.ports.iter().find(|p| p.name == *name)?;
        Some(port_frame(p).then(&part.placement))
    }

    /// The port type name behind an exported port.
    fn port_type(&self, port: &str) -> Option<&str> {
        let (idx, name) = self.ports.get(port)?;
        self.parts[*idx]
            .primitive
            .ports
            .iter()
            .find(|p| p.name == *name)
            .map(|p| p.port_type.as_str())
    }

    fn port_params(&self, port: &str) -> Option<&IndexMap<String, Value>> {
        let (idx, name) = self.ports.get(port)?;
        self.parts[*idx]
            .primitive
            .ports
            .iter()
            .find(|p| p.name == *name)
            .map(|p| &p.params)
    }
}

/// Resolve an assembly definition into placed instances and checked mates.
///
/// `extra_units` lets a caller inject generated content, such as a chassis, as if it had been
/// written in the file. Each entry is (instance id, resolved sub-assembly).
pub fn resolve_assembly(
    lib: &Library,
    def: &AssemblyDef,
    overrides: &Overrides,
    extra_units: Vec<(String, ResolvedAssembly)>,
) -> Result<ResolvedAssembly, ModelError> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // Assembly parameters give context to the instance parameter expressions.
    let no_variants = IndexMap::new();
    let params = resolve_params(&def.params, &no_variants, overrides)?;
    let env = crate::ParamEnv { values: &params, variants: &no_variants };

    // Build the units.
    let mut units: Vec<Unit> = Vec::new();
    for (id, inner) in extra_units {
        units.push(unit_from_assembly(id, inner));
    }
    for inst in &def.instances {
        match &inst.source {
            InstanceSource::Primitive(pid) => {
                let Some(pdef) = lib.primitive(pid) else {
                    errors.push(format!("instance `{}`: unknown primitive `{pid}`", inst.id));
                    continue;
                };
                let mut o = Overrides::default();
                for (k, e) in &inst.params {
                    match eval(e, &env) {
                        Ok(v) => {
                            o.params.insert(k.clone(), v);
                        }
                        Err(err) => errors.push(format!("instance `{}`: parameter `{k}`: {err}", inst.id)),
                    }
                }
                for (k, v) in &inst.variants {
                    o.variants.insert(k.clone(), v.clone());
                }
                match resolve(pdef, &o) {
                    Ok(r) => {
                        let part = PlacedInstance {
                            id: inst.id.clone(),
                            source_id: pid.clone(),
                            primitive: r,
                            placement: Transform::IDENTITY,
                            placed_by: PlacedBy::Unreached,
                        };
                        let ports = part
                            .primitive
                            .ports
                            .iter()
                            .map(|p| (p.name.clone(), (0usize, p.name.clone())))
                            .collect();
                        units.push(Unit {
                            id: inst.id.clone(),
                            source_id: pid.clone(),
                            parts: vec![part],
                            ports,
                            free: free_placement(inst, &env, &mut errors),
                        });
                    }
                    Err(e) => errors.push(format!("instance `{}`: {e}", inst.id)),
                }
            }
            InstanceSource::Assembly(aid) => {
                let Some(adef) = lib.assembly(aid) else {
                    errors.push(format!("instance `{}`: unknown assembly `{aid}`", inst.id));
                    continue;
                };
                let mut o = Overrides::default();
                for (k, e) in &inst.params {
                    if let Ok(v) = eval(e, &env) {
                        o.params.insert(k.clone(), v);
                    }
                }
                match resolve_assembly(lib, adef, &o, Vec::new()) {
                    Ok(inner) => {
                        warnings.extend(inner.warnings.iter().map(|w| format!("{}: {w}", inst.id)));
                        errors.extend(inner.errors.iter().map(|e| format!("{}: {e}", inst.id)));
                        let mut u = unit_from_assembly(inst.id.clone(), inner);
                        u.source_id = aid.clone();
                        u.free = free_placement(inst, &env, &mut errors);
                        units.push(u);
                    }
                    Err(e) => errors.push(format!("instance `{}`: {e}", inst.id)),
                }
            }
        }
    }

    // Check every mate and record it.
    let index: HashMap<String, usize> = units.iter().enumerate().map(|(i, u)| (u.id.clone(), i)).collect();
    let mut mates: Vec<ResolvedMate> = Vec::new();
    for m in &def.mates {
        let (Some(&ia), Some(&ib)) = (index.get(&m.a.instance), index.get(&m.b.instance)) else {
            // The parser reports unknown instances; skip quietly here.
            continue;
        };
        let ua = &units[ia];
        let ub = &units[ib];
        let compatible = check_mate(lib, ua, &m.a.port, ub, &m.b.port);
        let dof = m.dof.unwrap_or_else(|| {
            ua.port_type(&m.a.port)
                .and_then(|t| lib.port_types.get(t))
                .map(|t| t.dof)
                .unwrap_or_default()
        });
        let stage = m.stage.unwrap_or_else(|| {
            ua.port_type(&m.a.port)
                .and_then(|t| lib.port_types.get(t))
                .map(|t| t.stage)
                .unwrap_or_default()
        });
        mates.push(ResolvedMate {
            id: m.id.clone(),
            a: m.a.instance.clone(),
            a_port: m.a.port.clone(),
            b: m.b.instance.clone(),
            b_port: m.b.port.clone(),
            dof,
            stage,
            fasteners: m.fasteners.clone(),
            compatible,
        });
    }

    // Solve placement.
    let placements = solve_placement(def, &units, &mates, lib, &env, &mut warnings, &mut errors);

    // Flatten.
    let mut instances = Vec::new();
    for (i, u) in units.iter().enumerate() {
        let (unit_xform, placed_by) = placements[i].clone();
        for part in &u.parts {
            let mut p = part.clone();
            p.id = if u.parts.len() == 1 && p.id == u.id { u.id.clone() } else { format!("{}.{}", u.id, p.id) };
            p.placement = part.placement.then(&unit_xform);
            p.placed_by = if u.parts.len() == 1 { placed_by.clone() } else { part.placed_by.clone() };
            instances.push(p);
        }
    }

    // Point masses.
    let mut point_masses = Vec::new();
    for pm in &def.point_masses {
        let mass = match eval(&pm.mass, &env).ok().and_then(|v| v.as_quantity()) {
            Some(q) if q.dim == wmds_units::Dim::MASS => q,
            _ => {
                errors.push(format!("mass `{}`: value must be a mass", pm.id));
                continue;
            }
        };
        let at = match eval(&pm.at, &env).ok().as_ref().map(as_length_vec3) {
            Some(Ok(v)) => v,
            _ => {
                errors.push(format!("mass `{}`: at must be a 3-tuple of lengths", pm.id));
                continue;
            }
        };
        point_masses.push(PointMass { id: pm.id.clone(), mass, at, state: pm.state.clone() });
    }

    // Exports, translated to the flattened instance ids.
    let mut exports = IndexMap::new();
    for e in &def.exports {
        if let Some(&i) = index.get(&e.source.instance) {
            let u = &units[i];
            if u.ports.contains_key(&e.source.port) {
                let inst_id = if u.parts.len() == 1 { u.id.clone() } else { format!("{}.{}", u.id, u.parts[u.ports[&e.source.port].0].id) };
                exports.insert(e.name.clone(), (inst_id, u.ports[&e.source.port].1.clone()));
            } else {
                errors.push(format!("export `{}`: `{}` has no port `{}`", e.name, e.source.instance, e.source.port));
            }
        }
    }

    for m in &mates {
        if let Err(e) = &m.compatible {
            errors.push(format!("mate `{}`: {e}", m.id));
        }
    }

    Ok(ResolvedAssembly {
        id: def.id.clone(),
        version: def.version.clone(),
        kind: def.kind,
        instances,
        mates,
        point_masses,
        exports,
        warnings,
        errors,
    })
}

fn unit_from_assembly(id: String, inner: ResolvedAssembly) -> Unit {
    let parts = inner.instances.clone();
    let pos: HashMap<&str, usize> = parts.iter().enumerate().map(|(i, p)| (p.id.as_str(), i)).collect();
    let mut ports = IndexMap::new();
    for (name, (inst, port)) in &inner.exports {
        if let Some(&i) = pos.get(inst.as_str()) {
            ports.insert(name.clone(), (i, port.clone()));
        }
    }
    Unit { id, source_id: inner.id.clone(), parts, ports, free: None }
}

fn free_placement(
    inst: &wmds_schema::InstanceDef,
    env: &dyn Env,
    errors: &mut Vec<String>,
) -> Option<(Transform, String)> {
    let pl = inst.placement.as_ref()?;
    let at = match eval(&pl.at, env).ok().as_ref().map(as_length_vec3) {
        Some(Ok(v)) => [v[0].value, v[1].value, v[2].value],
        _ => {
            errors.push(format!("instance `{}`: place at= must be a 3-tuple of lengths", inst.id));
            [0.0; 3]
        }
    };
    let mut t = Transform::IDENTITY;
    if let Some(r) = &pl.rotate {
        if let Some(Value::Tuple(items)) = eval(r, env).ok() {
            let angles: Vec<f64> = items
                .iter()
                .filter_map(|v| v.as_quantity())
                .map(|q| if q.dim == wmds_units::Dim::ANGLE { q.value } else { q.value.to_radians() })
                .collect();
            if angles.len() == 3 {
                t = t
                    .then(&Transform::rotation([1.0, 0.0, 0.0], angles[0]))
                    .then(&Transform::rotation([0.0, 1.0, 0.0], angles[1]))
                    .then(&Transform::rotation([0.0, 0.0, 1.0], angles[2]));
            }
        }
    }
    if let Some(m) = &pl.mirror {
        let n = match m.as_str() {
            "xy" => [0.0, 0.0, 1.0],
            "xz" => [0.0, 1.0, 0.0],
            "yz" => [1.0, 0.0, 0.0],
            _ => [0.0, 1.0, 0.0],
        };
        t = t.then(&Transform::mirror(n));
    }
    t.translation = at;
    Some((t, pl.justification.clone()))
}

/// Breadth-first placement from the root outward along the mate graph.
#[allow(clippy::too_many_arguments)]
fn solve_placement(
    def: &AssemblyDef,
    units: &[Unit],
    mates: &[ResolvedMate],
    lib: &Library,
    _env: &dyn Env,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) -> Vec<(Transform, PlacedBy)> {
    let n = units.len();
    let mut out: Vec<(Transform, PlacedBy)> = vec![(Transform::IDENTITY, PlacedBy::Unreached); n];
    let mut placed = vec![false; n];
    if n == 0 {
        return out;
    }
    let index: HashMap<&str, usize> = units.iter().enumerate().map(|(i, u)| (u.id.as_str(), i)).collect();

    let mut queue: VecDeque<usize> = VecDeque::new();

    // Seed: explicit root, then anything with a free placement, then the first unit.
    let root_idx = def
        .root
        .as_deref()
        .and_then(|r| index.get(r).copied())
        .or_else(|| units.iter().position(|u| u.free.is_none()));
    if let Some(r) = root_idx {
        out[r] = (Transform::IDENTITY, PlacedBy::Root);
        placed[r] = true;
        queue.push_back(r);
    }
    for (i, u) in units.iter().enumerate() {
        if let Some((t, why)) = &u.free {
            out[i] = (*t, PlacedBy::Free(why.clone()));
            placed[i] = true;
            queue.push_back(i);
        }
    }

    // Adjacency, skipping mates that failed their compatibility check: placing through an
    // invalid joint would produce a plausible-looking but wrong model.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (mi, m) in mates.iter().enumerate() {
        if m.compatible.is_err() {
            continue;
        }
        let (Some(&a), Some(&b)) = (index.get(m.a.as_str()), index.get(m.b.as_str())) else { continue };
        adj[a].push(mi);
        adj[b].push(mi);
    }

    while let Some(i) = queue.pop_front() {
        for &mi in &adj[i] {
            let m = &mates[mi];
            let a = index[m.a.as_str()];
            let b = index[m.b.as_str()];
            let (from, from_port, to, to_port) = if a == i { (a, &m.a_port, b, &m.b_port) } else { (b, &m.b_port, a, &m.a_port) };
            if placed[to] {
                continue;
            }
            let (Some(fixed_local), Some(moving_local)) = (units[from].port_local(from_port), units[to].port_local(to_port)) else {
                continue;
            };
            let fixed_world = fixed_local.then(&out[from].0);
            let aligned = units[from]
                .port_type(from_port)
                .and_then(|t| lib.port_types.get(t))
                .map(|t| t.aligned)
                .unwrap_or(false);
            let axis = if aligned { MateAxis::Aligned } else { MateAxis::Opposed };
            let placement = solve_mate(&fixed_world, &moving_local, axis, None);
            out[to] = (placement, PlacedBy::Mate(m.id.clone()));
            placed[to] = true;
            queue.push_back(to);
        }
    }

    // Anything left unplaced is a hole in the design, not a detail.
    let unplaced: Vec<&str> = units
        .iter()
        .enumerate()
        .filter(|(i, _)| !placed[*i])
        .map(|(_, u)| u.id.as_str())
        .collect();
    if !unplaced.is_empty() {
        errors.push(format!(
            "not connected to the rest of the assembly, so their position is unknown: {}",
            unplaced.join(", ")
        ));
    }

    // Over-constrained mates are legitimate (a part bolted at four corners) but worth counting,
    // because a closed loop that does not actually close is a common modelling mistake.
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut redundant = 0;
    for m in mates.iter().filter(|m| m.compatible.is_ok()) {
        let (Some(&a), Some(&b)) = (index.get(m.a.as_str()), index.get(m.b.as_str())) else { continue };
        let key = (a.min(b), a.max(b));
        if !seen.insert(key) {
            redundant += 1;
        }
    }
    if redundant > 0 {
        warnings.push(format!(
            "{redundant} mate(s) join a pair of parts that is already joined; positions come from the first and the rest are assumed consistent"
        ));
    }
    out
}

/// Are these two ports allowed to be joined?
fn check_mate(lib: &Library, ua: &Unit, pa: &str, ub: &Unit, pb: &str) -> Result<(), MateError> {
    let Some(ta) = ua.port_type(pa) else {
        return Err(MateError::NoSuchPort(ua.id.clone(), pa.to_string()));
    };
    let Some(tb) = ub.port_type(pb) else {
        return Err(MateError::NoSuchPort(ub.id.clone(), pb.to_string()));
    };
    let Some(defa) = lib.port_types.get(ta) else {
        return Err(MateError::UnknownPortType(ta.to_string()));
    };
    if !lib.port_types.contains_key(tb) {
        return Err(MateError::UnknownPortType(tb.to_string()));
    }
    let rule = defa.compatible.iter().find(|c| c.other == tb);
    let Some(rule) = rule else {
        return Err(MateError::Incompatible {
            a: format!("{}.{}", ua.id, pa),
            a_type: ta.to_string(),
            b: format!("{}.{}", ub.id, pb),
            b_type: tb.to_string(),
        });
    };
    let Some(when) = &rule.when else { return Ok(()) };

    let params_a = ua.port_params(pa).cloned().unwrap_or_default();
    let params_b = ub.port_params(pb).cloned().unwrap_or_default();
    check_required(defa, &params_a, &format!("{}.{}", ua.id, pa))?;
    if let Some(defb) = lib.port_types.get(tb) {
        check_required(defb, &params_b, &format!("{}.{}", ub.id, pb))?;
    }

    let env = PortPairEnv { a: record(&params_a), b: record(&params_b) };
    match eval(when, &env) {
        Ok(Value::Bool(true)) => Ok(()),
        Ok(Value::Bool(false)) => Err(MateError::ParamsDiffer {
            a: format!("{}.{} [{}]", ua.id, pa, describe(&params_a)),
            b: format!("{}.{} [{}]", ub.id, pb, describe(&params_b)),
            reason: "the compatibility rule for these port types is not satisfied".into(),
        }),
        Ok(v) => Err(MateError::ParamsDiffer {
            a: format!("{}.{}", ua.id, pa),
            b: format!("{}.{}", ub.id, pb),
            reason: format!("compatibility rule returned {} instead of a yes or no", v.type_name()),
        }),
        Err(e) => Err(MateError::ParamsDiffer {
            a: format!("{}.{}", ua.id, pa),
            b: format!("{}.{}", ub.id, pb),
            reason: e.to_string(),
        }),
    }
}

fn check_required(def: &PortTypeDef, params: &IndexMap<String, Value>, who: &str) -> Result<(), MateError> {
    for p in &def.params {
        if p.optional {
            continue;
        }
        match params.get(&p.name) {
            None => return Err(MateError::MissingParam(p.name.clone(), who.to_string())),
            Some(v) => {
                let ok = match &p.kind {
                    ParamKind::Int => v.as_quantity().is_some_and(|q| q.dim.is_dimensionless()),
                    ParamKind::Bool => matches!(v, Value::Bool(_)),
                    ParamKind::Text => matches!(v, Value::Str(_)),
                    ParamKind::Quantity(u) => {
                        let want = Quantity::dim_of_unit(u).unwrap_or_default();
                        v.as_quantity().is_some_and(|q| q.dim == want)
                    }
                };
                if !ok {
                    return Err(MateError::ParamsDiffer {
                        a: who.to_string(),
                        b: def.name.clone(),
                        reason: format!("parameter `{}` has the wrong type", p.name),
                    });
                }
            }
        }
    }
    Ok(())
}

fn record(params: &IndexMap<String, Value>) -> Value {
    Value::record(params.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn describe(params: &IndexMap<String, Value>) -> String {
    params.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")
}

/// Exposes `a` and `b` to a port compatibility expression.
struct PortPairEnv {
    a: Value,
    b: Value,
}

impl Env for PortPairEnv {
    fn lookup(&self, path: &[String]) -> Option<Value> {
        let base = match path[0].as_str() {
            "a" => &self.a,
            "b" => &self.b,
            _ => return None,
        };
        wmds_expr::walk(base, &path[1..])
    }
}
