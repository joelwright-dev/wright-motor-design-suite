//! Core data model.
//!
//! Slice 1 of Phase 0: resolving a [`PrimitiveDef`] into a [`ResolvedPrimitive`] by evaluating
//! its parameters (with dependency ordering, unit coercion and range checks), geometry feature
//! arguments, port frames and manufacturing cost expressions.

pub mod assembly;
pub mod chassis;
pub mod library;
pub mod transform;

pub use assembly::{MateError, PlacedBy, PlacedInstance, ResolvedAssembly, ResolvedMate, resolve_assembly};
pub use chassis::{ChassisError, GeneratedChassis, generate as generate_chassis};
pub use library::Library;
pub use transform::{Frame, MateAxis, Transform, solve_mate};

use std::collections::HashMap;

use indexmap::IndexMap;
use thiserror::Error;
use wmds_expr::{Env, EvalError, Expr, Value, eval};
use wmds_schema::{AxisSpec, Feature, MassPropsDef, PrimitiveDef};
use wmds_units::{Dim, Quantity};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ModelError {
    #[error("param `{0}`: {1}")]
    Param(String, String),
    #[error("params could not be resolved (cycle or unknown name): {0:?}")]
    Unresolved(Vec<String>),
    #[error("port `{0}`: {1}")]
    Port(String, String),
    #[error("geometry feature `{0}`: {1}")]
    Feature(String, String),
    #[error("massprops: {0}")]
    MassProps(String),
    #[error("manufacturing method `{0}`: {1}")]
    Mfg(String, String),
    #[error("unknown variant `{0}`")]
    UnknownVariant(String),
    #[error("variant `{0}` has no option `{1}`")]
    BadVariantOption(String, String),
}

/// A primitive with every expression evaluated for one set of parameter values.
#[derive(Debug, Clone)]
pub struct ResolvedPrimitive {
    pub id: String,
    pub version: String,
    pub params: IndexMap<String, Value>,
    pub variants: IndexMap<String, String>,
    pub material: Option<String>,
    pub geometry: Vec<ResolvedLevel>,
    pub massprops: ResolvedMassProps,
    pub ports: Vec<ResolvedPort>,
    pub manufacturing: Vec<ResolvedMfg>,
}

#[derive(Debug, Clone)]
pub struct ResolvedLevel {
    pub level: String,
    pub features: Vec<ResolvedFeature>,
}

#[derive(Debug, Clone)]
pub struct ResolvedFeature {
    pub op: String,
    pub name: Option<String>,
    pub args: IndexMap<String, Value>,
    pub positional: Vec<Value>,
}

#[derive(Debug, Clone)]
pub enum ResolvedMassProps {
    Computed,
    Declared {
        mass: Quantity,
        cg: Option<[Quantity; 3]>,
        inertia: Option<Value>,
    },
}

/// A port with its frame in the primitive's coordinate system. Lengths are SI (metres).
#[derive(Debug, Clone)]
pub struct ResolvedPort {
    pub name: String,
    pub port_type: String,
    pub origin: [Quantity; 3],
    /// Unit vector of the mating direction.
    pub axis: [f64; 3],
    pub clock: Option<[f64; 3]>,
    pub symmetry: Option<u32>,
    pub params: IndexMap<String, Value>,
    pub load_rating: Option<Quantity>,
    pub grid: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedMfg {
    pub method: String,
    pub scale: Option<String>,
    pub exports: Vec<String>,
    pub cost_fixed: Option<Quantity>,
    pub cost_per_unit: Option<Quantity>,
}

/// Parameter and variant overrides supplied by an instance or the CLI.
#[derive(Default, Debug, Clone)]
pub struct Overrides {
    pub params: HashMap<String, Value>,
    pub variants: HashMap<String, String>,
}

impl Overrides {
    /// Parse `name=value` pairs. Values are expressions (`400mm`, `"left"`, `2*3`).
    pub fn parse_pairs(pairs: &[String]) -> Result<Overrides, String> {
        let mut o = Overrides::default();
        for p in pairs {
            let (k, v) = p
                .split_once('=')
                .ok_or_else(|| format!("override `{p}` is not name=value"))?;
            let expr = wmds_expr::parse(v).map_err(|e| format!("override `{p}`: {e}"))?;
            let empty = wmds_expr::MapEnv::new();
            let val = match eval(&expr, &empty) {
                Ok(val) => val,
                // A bare word is a string (variant option).
                Err(EvalError::Unknown(_)) if matches!(expr, Expr::Path(ref path) if path.len() == 1) => {
                    Value::Str(v.trim().to_string())
                }
                Err(e) => return Err(format!("override `{p}`: {e}")),
            };
            match &val {
                Value::Str(s) => {
                    o.variants.insert(k.trim().to_string(), s.clone());
                }
                _ => {
                    o.params.insert(k.trim().to_string(), val);
                }
            }
        }
        Ok(o)
    }
}

pub(crate) struct ParamEnv<'a> {
    pub values: &'a IndexMap<String, Value>,
    pub variants: &'a IndexMap<String, String>,
}

/// Evaluate a parameter block: defaults, derived expressions, overrides, unit coercion and
/// range checks. Parameters may be declared in any order; evaluation repeats until no further
/// progress is possible, which resolves forward references and detects cycles.
pub fn resolve_params(
    params: &[wmds_schema::ParamDef],
    variants: &IndexMap<String, String>,
    overrides: &Overrides,
) -> Result<IndexMap<String, Value>, ModelError> {
    let mut values: IndexMap<String, Value> = IndexMap::new();
    let mut pending: Vec<&wmds_schema::ParamDef> = params.iter().collect();
    loop {
        let before = pending.len();
        let mut still = Vec::new();
        for p in pending {
            let env = ParamEnv { values: &values, variants };
            let source: Option<&Expr> = if p.expr.is_some() { p.expr.as_ref() } else { p.default.as_ref() };
            let overridden = overrides
                .params
                .get(&p.name)
                .cloned()
                .or_else(|| overrides.variants.get(&p.name).map(|s| Value::Str(s.clone())));
            if overridden.is_some() && p.expr.is_some() {
                return Err(ModelError::Param(
                    p.name.clone(),
                    "derived parameters (expr=) cannot be overridden".into(),
                ));
            }
            let raw = match overridden {
                Some(v) => Ok(v),
                None => match source {
                    Some(e) => eval(e, &env),
                    None => Ok(Value::Str(String::new())),
                },
            };
            match raw {
                Ok(v) => {
                    let v = coerce_unit(&p.name, v, p.unit.as_deref())?;
                    check_range(&p.name, &v, p.min.as_ref(), p.max.as_ref(), p.unit.as_deref(), &env)?;
                    values.insert(p.name.clone(), v);
                }
                Err(EvalError::Unknown(_)) => still.push(p),
                Err(e) => return Err(ModelError::Param(p.name.clone(), e.to_string())),
            }
        }
        pending = still;
        if pending.is_empty() {
            break;
        }
        if pending.len() == before {
            return Err(ModelError::Unresolved(pending.iter().map(|p| p.name.clone()).collect()));
        }
    }
    Ok(values)
}

impl Env for ParamEnv<'_> {
    fn lookup(&self, path: &[String]) -> Option<Value> {
        if let Some(v) = self.values.get(&path[0]) {
            return wmds_expr::walk(v, &path[1..]);
        }
        if path.len() == 1
            && let Some(v) = self.variants.get(&path[0])
        {
            return Some(Value::Str(v.clone()));
        }
        None
    }
}

/// Resolve a primitive definition with the given overrides.
pub fn resolve(def: &PrimitiveDef, overrides: &Overrides) -> Result<ResolvedPrimitive, ModelError> {
    // Variants: default to the first option.
    let mut variants: IndexMap<String, String> = IndexMap::new();
    for v in &def.variants {
        let chosen = match overrides.variants.get(&v.name) {
            Some(o) => {
                if !v.options.contains(o) {
                    return Err(ModelError::BadVariantOption(v.name.clone(), o.clone()));
                }
                o.clone()
            }
            None => v.options[0].clone(),
        };
        variants.insert(v.name.clone(), chosen);
    }
    for k in overrides.variants.keys() {
        if !variants.contains_key(k) {
            // Could also be a string-valued param; only an error if no param has that name.
            if !def.params.iter().any(|p| p.name == *k) {
                return Err(ModelError::UnknownVariant(k.clone()));
            }
        }
    }
    for k in overrides.params.keys() {
        if !def.params.iter().any(|p| p.name == *k) {
            return Err(ModelError::Param(k.clone(), "no such parameter".into()));
        }
    }

    let values = resolve_params(&def.params, &variants, overrides)?;

    let env = ParamEnv {
        values: &values,
        variants: &variants,
    };

    // Geometry
    let mut geometry = Vec::new();
    for lvl in &def.geometry {
        let mut features = Vec::new();
        for f in &lvl.features {
            features.push(resolve_feature(f, &env)?);
        }
        geometry.push(ResolvedLevel {
            level: lvl.level.clone(),
            features,
        });
    }

    // Mass properties
    let massprops = match &def.massprops {
        MassPropsDef::Computed => ResolvedMassProps::Computed,
        MassPropsDef::Declared { mass, cg, inertia } => {
            let m = eval(mass, &env).map_err(|e| ModelError::MassProps(e.to_string()))?;
            let m = m
                .as_quantity()
                .filter(|q| q.dim == Dim::MASS)
                .ok_or_else(|| ModelError::MassProps("mass must be a mass quantity".into()))?;
            let cg = match cg {
                Some(e) => Some(
                    as_length_vec3(
                        &eval(e, &env).map_err(|e| ModelError::MassProps(e.to_string()))?,
                    )
                    .map_err(ModelError::MassProps)?,
                ),
                None => None,
            };
            let inertia = match inertia {
                Some(e) => Some(eval(e, &env).map_err(|e| ModelError::MassProps(e.to_string()))?),
                None => None,
            };
            ResolvedMassProps::Declared {
                mass: m,
                cg,
                inertia,
            }
        }
    };

    // Ports
    let mut ports = Vec::new();
    for p in &def.ports {
        let perr = |m: String| ModelError::Port(p.name.clone(), m);
        let origin =
            as_length_vec3(&eval(&p.at, &env).map_err(|e| perr(e.to_string()))?).map_err(perr)?;
        let axis = resolve_axis(&p.axis, &env).map_err(perr)?;
        let clock = match &p.clock {
            Some(c) => Some(resolve_axis(c, &env).map_err(perr)?),
            None => None,
        };
        let mut params = IndexMap::new();
        for (k, e) in &p.params {
            params.insert(
                k.clone(),
                eval_lenient(e, &env).map_err(|e| perr(format!("param `{k}`: {e}")))?,
            );
        }
        let load_rating = match &p.load_rating {
            Some(e) => {
                let v = eval(e, &env).map_err(|e| perr(e.to_string()))?;
                Some(
                    v.as_quantity()
                        .filter(|q| q.dim == Dim::FORCE || q.dim == Dim::TORQUE)
                        .ok_or_else(|| perr("load_rating must be a force or torque".into()))?,
                )
            }
            None => None,
        };
        ports.push(ResolvedPort {
            name: p.name.clone(),
            port_type: p.port_type.clone(),
            origin,
            axis,
            clock,
            symmetry: p.symmetry,
            params,
            load_rating,
            grid: p.grid,
        });
    }

    // Manufacturing
    let mut manufacturing = Vec::new();
    for m in &def.manufacturing {
        let merr = |e: String| ModelError::Mfg(m.method.clone(), e);
        let (cost_fixed, cost_per_unit) = match &m.cost {
            Some(c) => {
                let f = match &c.fixed {
                    Some(e) => Some(
                        eval(e, &env)
                            .map_err(|e| merr(e.to_string()))?
                            .as_quantity()
                            .filter(|q| q.dim == Dim::CURRENCY)
                            .ok_or_else(|| merr("fixed cost must be a currency amount".into()))?,
                    ),
                    None => None,
                };
                let u = match &c.per_unit {
                    Some(e) => Some(
                        eval(e, &env)
                            .map_err(|e| merr(e.to_string()))?
                            .as_quantity()
                            .filter(|q| q.dim == Dim::CURRENCY)
                            .ok_or_else(|| {
                                merr("per_unit cost must evaluate to a currency amount".into())
                            })?,
                    ),
                    None => None,
                };
                (f, u)
            }
            None => (None, None),
        };
        manufacturing.push(ResolvedMfg {
            method: m.method.clone(),
            scale: m.scale.clone(),
            exports: m.exports.clone(),
            cost_fixed,
            cost_per_unit,
        });
    }

    Ok(ResolvedPrimitive {
        id: def.id.clone(),
        version: def.version.clone(),
        params: values,
        variants,
        material: def.material.clone(),
        geometry,
        massprops,
        ports,
        manufacturing,
    })
}

/// Evaluate, treating an unknown bare word as a literal string (`bolt="M12"`).
fn eval_lenient(e: &Expr, env: &dyn Env) -> Result<Value, EvalError> {
    match eval(e, env) {
        Err(EvalError::Unknown(_)) => match e {
            Expr::Path(p) if p.len() == 1 => Ok(Value::Str(p[0].clone())),
            _ => eval(e, env),
        },
        other => other,
    }
}

fn resolve_feature(f: &Feature, env: &dyn Env) -> Result<ResolvedFeature, ModelError> {
    let label = f.name.clone().unwrap_or_else(|| f.op.clone());
    let mut args = IndexMap::new();
    for (k, e) in &f.args {
        args.insert(
            k.clone(),
            eval_lenient(e, env)
                .map_err(|e| ModelError::Feature(label.clone(), format!("arg `{k}`: {e}")))?,
        );
    }
    let mut positional = Vec::new();
    for e in &f.positional {
        positional.push(
            eval_lenient(e, env).map_err(|e| ModelError::Feature(label.clone(), e.to_string()))?,
        );
    }
    Ok(ResolvedFeature {
        op: f.op.clone(),
        name: f.name.clone(),
        args,
        positional,
    })
}

/// A plain number for a parameter with a declared unit is taken to be in that unit.
fn coerce_unit(name: &str, v: Value, unit: Option<&str>) -> Result<Value, ModelError> {
    let Some(unit) = unit else { return Ok(v) };
    let want =
        Quantity::dim_of_unit(unit).map_err(|e| ModelError::Param(name.into(), e.to_string()))?;
    match v {
        Value::Num(q) if q.dim == want => Ok(Value::Num(q)),
        Value::Num(q) if q.dim.is_dimensionless() => Ok(Value::Num(
            Quantity::from_unit(q.value, unit)
                .map_err(|e| ModelError::Param(name.into(), e.to_string()))?,
        )),
        Value::Num(q) => Err(ModelError::Param(
            name.into(),
            format!("expected a value in {unit}, found {q}"),
        )),
        other => Err(ModelError::Param(
            name.into(),
            format!("expected a number in {unit}, found {}", other.type_name()),
        )),
    }
}

fn check_range(
    name: &str,
    v: &Value,
    min: Option<&Expr>,
    max: Option<&Expr>,
    unit: Option<&str>,
    env: &dyn Env,
) -> Result<(), ModelError> {
    let Some(q) = v.as_quantity() else {
        return Ok(());
    };
    let bound = |e: &Expr| -> Result<Quantity, ModelError> {
        let b = eval(e, env).map_err(|e| ModelError::Param(name.into(), e.to_string()))?;
        let b = coerce_unit(name, b, unit)?;
        b.as_quantity()
            .ok_or_else(|| ModelError::Param(name.into(), "range bound must be a number".into()))
    };
    if let Some(m) = min {
        let m = bound(m)?;
        m.same_dim(&q)
            .map_err(|e| ModelError::Param(name.into(), e.to_string()))?;
        if q.value < m.value {
            return Err(ModelError::Param(
                name.into(),
                format!("{q} is below the minimum {m}"),
            ));
        }
    }
    if let Some(m) = max {
        let m = bound(m)?;
        m.same_dim(&q)
            .map_err(|e| ModelError::Param(name.into(), e.to_string()))?;
        if q.value > m.value {
            return Err(ModelError::Param(
                name.into(),
                format!("{q} is above the maximum {m}"),
            ));
        }
    }
    Ok(())
}

/// Convert a tuple to three lengths. Dimensionless zeros are accepted as zero length.
pub fn as_length_vec3(v: &Value) -> Result<[Quantity; 3], String> {
    let Value::Tuple(items) = v else {
        return Err(format!(
            "expected a 3-tuple of lengths, found {}",
            v.type_name()
        ));
    };
    if items.len() != 3 {
        return Err(format!("expected 3 components, found {}", items.len()));
    }
    let mut out = [Quantity::new(0.0, Dim::LENGTH); 3];
    for (i, it) in items.iter().enumerate() {
        let q = it
            .as_quantity()
            .ok_or_else(|| format!("component {i} is not a number"))?;
        if q.dim == Dim::LENGTH {
            out[i] = q;
        } else if q.dim.is_dimensionless() && q.value == 0.0 {
            out[i] = Quantity::new(0.0, Dim::LENGTH);
        } else {
            return Err(format!("component {i} must be a length, found {q}"));
        }
    }
    Ok(out)
}

fn resolve_axis(a: &AxisSpec, env: &dyn Env) -> Result<[f64; 3], String> {
    let v = match a {
        AxisSpec::Named(n) => match n.as_str() {
            "x" => [1.0, 0.0, 0.0],
            "y" => [0.0, 1.0, 0.0],
            "z" => [0.0, 0.0, 1.0],
            "-x" => [-1.0, 0.0, 0.0],
            "-y" => [0.0, -1.0, 0.0],
            "-z" => [0.0, 0.0, -1.0],
            other => return Err(format!("unknown axis `{other}`")),
        },
        AxisSpec::Expr(e) => {
            let val = eval(e, env).map_err(|e| e.to_string())?;
            let Value::Tuple(items) = &val else {
                return Err("axis must be x/y/z or a 3-tuple".into());
            };
            if items.len() != 3 {
                return Err("axis tuple needs 3 components".into());
            }
            let mut out = [0.0; 3];
            let mut dim: Option<Dim> = None;
            for (i, it) in items.iter().enumerate() {
                let q = it.as_quantity().ok_or("axis components must be numbers")?;
                if let Some(d) = dim {
                    if d != q.dim && !(q.value == 0.0) {
                        return Err("axis components must share a dimension".into());
                    }
                } else if q.value != 0.0 {
                    dim = Some(q.dim);
                }
                out[i] = q.value;
            }
            out
        }
    };
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n == 0.0 {
        return Err("axis has zero length".into());
    }
    Ok([v[0] / n, v[1] / n, v[2] / n])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARM: &str = r#"
primitive "suspension/arms/test" version="0.1.0" {
    description "test arm"
    category "suspension" sub="control-arm"
    params {
        leg_length unit="mm" expr="sqrt((span/2)^2 + reach^2)"
        span unit="mm" default=380 min=250 max=600
        reach unit="mm" default=320 min=200 max=500
        tube_od unit="mm" default=28
    }
    variants { hand "left" "right" }
    material "steel/e355-tube"
    geometry level="manufacture" {
        tube "front_leg" od=tube_od wall="2.5 mm" from="(-span/2, 0, 0)" to="(0, reach, 0)"
        union
    }
    massprops computed=#true
    ports {
        port "balljoint" type="balljoint.taper" at="(0, reach, 0)" axis="z" load_rating="35 kN" {
            params taper="1:8" stud="M14"
        }
        port "bush_front" type="bush.pivot" at="(-span/2, 0, 0)" axis="(1, 0, 0)"
    }
    manufacturing {
        method "tube-cut-notch-weld" scale="1..1000" {
            export "tube-list"
            cost fixed="45 AUD" per_unit="0.9 AUD/mm * (span + 2*reach)"
        }
    }
    compliance "suspension.arm" "structural"
}
"#;

    fn def() -> PrimitiveDef {
        wmds_schema::parse_primitive("arm.prim.kdl", ARM)
            .map_err(|e| format!("{e:?}"))
            .unwrap()
    }

    #[test]
    fn resolves_defaults_in_any_order() {
        let r = resolve(&def(), &Overrides::default()).unwrap();
        let ll = r.params["leg_length"]
            .as_quantity()
            .unwrap()
            .to_unit("mm")
            .unwrap();
        assert!((ll - (190.0f64.powi(2) + 320.0f64.powi(2)).sqrt()).abs() < 1e-6);
        assert_eq!(r.variants["hand"], "left");
        assert_eq!(r.ports[0].origin[1].to_unit("mm").unwrap(), 320.0);
        assert_eq!(r.ports[0].params["taper"], Value::Str("1:8".into()));
        assert_eq!(r.ports[0].params["stud"], Value::Str("M14".into()));
        assert_eq!(r.ports[1].axis, [1.0, 0.0, 0.0]);
        assert_eq!(r.ports[1].origin[0].to_unit("mm").unwrap(), -190.0);
        let cost = r.manufacturing[0]
            .cost_per_unit
            .unwrap()
            .to_unit("AUD")
            .unwrap();
        assert!((cost - 0.9 * (380.0 + 640.0)).abs() < 1e-6);
    }

    #[test]
    fn overrides_and_ranges() {
        let o = Overrides::parse_pairs(&["span=400mm".into(), "hand=right".into()]).unwrap();
        let r = resolve(&def(), &o).unwrap();
        assert_eq!(
            r.params["span"]
                .as_quantity()
                .unwrap()
                .to_unit("mm")
                .unwrap(),
            400.0
        );
        assert_eq!(r.variants["hand"], "right");
        let o = Overrides::parse_pairs(&["span=900".into()]).unwrap();
        let e = resolve(&def(), &o).unwrap_err();
        assert!(
            matches!(e, ModelError::Param(ref n, ref m) if n == "span" && m.contains("maximum"))
        );
        let o = Overrides::parse_pairs(&["span=3kg".into()]).unwrap();
        assert!(resolve(&def(), &o).is_err());
        let o = Overrides::parse_pairs(&["leg_length=1mm".into()]).unwrap();
        assert!(resolve(&def(), &o).is_err());
    }
}
