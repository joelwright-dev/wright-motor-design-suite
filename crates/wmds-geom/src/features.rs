//! Interpreter from a resolved primitive's geometry features to kernel solids.
//!
//! Feature vocabulary (arguments are evaluated values from `wmds-model`):
//!
//! | feature | arguments |
//! |---------|-----------|
//! | `box "name"` | `size=(w, d, h)` and optionally `at=(x, y, z)` (centre), or `from=` / `to=` corners |
//! | `cylinder "name"` | `d=` or `r=`, `h=`, `at=` (centre), `axis=` (`"x"`, `"y"`, `"z"`, `"-x"` or a tuple); or `from=` / `to=` |
//! | `tube "name"` | `od=`, `wall=`, `from=`, `to=` |
//! | `box_tube "name"` | `size=(length, width, height)`, `wall=`, optional `at=` (centre). Hollow along x: the chassis rail shape. |
//! | `union` | no arguments: fuse every body built so far into one; or `union "name" a=X b=Y` |
//! | `subtract "name" a=X b=Y` | X minus Y |
//! | `intersect "name" a=X b=Y` | |
//! | `hull` | convex hull of the `manufacture` level (falls back to that level's solid if unsupported) |
//! | `mirror "name" of=X plane="xz"` | mirror a body about a principal plane through the origin |
//!
//! Bodies are named; unnamed features get `body<N>`. The level's result is the single remaining
//! body after the last feature, or the union of all remaining bodies.

use std::collections::BTreeMap;

use wmds_expr::Value;
use wmds_model::{ResolvedFeature, ResolvedLevel, ResolvedPrimitive};
use wmds_units::{Dim, Quantity};

use crate::{GeomError, GeomKernel, Result, Vec3, sub};

/// Solids built for each geometry level of a primitive.
pub struct BuiltGeometry<S> {
    pub levels: BTreeMap<String, S>,
    pub warnings: Vec<String>,
}

impl<S> BuiltGeometry<S> {
    /// The most detailed level available: manufacture, then display, then envelope.
    pub fn best(&self) -> Option<(&str, &S)> {
        for lvl in ["manufacture", "display", "envelope"] {
            if let Some(s) = self.levels.get(lvl) {
                return Some((lvl, s));
            }
        }
        self.levels.iter().next().map(|(k, v)| (k.as_str(), v))
    }
}

/// Build every geometry level of a resolved primitive.
pub fn build_primitive<K: GeomKernel>(
    k: &K,
    p: &ResolvedPrimitive,
) -> Result<BuiltGeometry<K::Solid>> {
    let mut out = BuiltGeometry {
        levels: BTreeMap::new(),
        warnings: Vec::new(),
    };
    // Build in an order that lets `hull` reference the manufacture level.
    let mut levels: Vec<&ResolvedLevel> = p.geometry.iter().collect();
    levels.sort_by_key(|l| match l.level.as_str() {
        "manufacture" => 0,
        "display" => 1,
        _ => 2,
    });
    for lvl in levels {
        let solid = build_level(k, lvl, &out)?;
        out.levels.insert(lvl.level.clone(), solid);
    }
    Ok(out)
}

/// Build one geometry level. `built` gives access to levels built earlier (for `hull`).
pub fn build_level<K: GeomKernel>(
    k: &K,
    lvl: &ResolvedLevel,
    built: &BuiltGeometry<K::Solid>,
) -> Result<K::Solid> {
    let mut bodies: Vec<(String, K::Solid)> = Vec::new();
    let mut counter = 0usize;
    let mut name_of = |f: &ResolvedFeature| -> String {
        f.name.clone().unwrap_or_else(|| {
            counter += 1;
            format!("body{counter}")
        })
    };

    for f in &lvl.features {
        let label = f.name.clone().unwrap_or_else(|| f.op.clone());
        let ferr = |m: String| GeomError::Feature(label.clone(), m);
        match f.op.as_str() {
            "box" => {
                let solid = if let (Some(a), Some(b)) = (f.args.get("from"), f.args.get("to")) {
                    let a = vec3_len(a).map_err(ferr)?;
                    let b = vec3_len(b).map_err(ferr)?;
                    let size = [
                        (b[0] - a[0]).abs(),
                        (b[1] - a[1]).abs(),
                        (b[2] - a[2]).abs(),
                    ];
                    let centre = [
                        (a[0] + b[0]) / 2.0,
                        (a[1] + b[1]) / 2.0,
                        (a[2] + b[2]) / 2.0,
                    ];
                    k.make_box(size, centre)?
                } else {
                    let size = vec3_len(
                        f.args
                            .get("size")
                            .ok_or_else(|| ferr("box needs size= or from=/to=".into()))?,
                    )
                    .map_err(ferr)?;
                    let centre = match f.args.get("at") {
                        Some(v) => vec3_len(v).map_err(ferr)?,
                        None => [0.0; 3],
                    };
                    k.make_box(size, centre)?
                };
                bodies.push((name_of(f), solid));
            }
            "cylinder" => {
                let r = match (f.args.get("r"), f.args.get("d")) {
                    (Some(r), _) => len(r).map_err(ferr)?,
                    (None, Some(d)) => len(d).map_err(ferr)? / 2.0,
                    _ => return Err(ferr("cylinder needs r= or d=".into())),
                };
                let solid = if let (Some(a), Some(b)) = (f.args.get("from"), f.args.get("to")) {
                    k.cylinder_between(vec3_len(a).map_err(ferr)?, vec3_len(b).map_err(ferr)?, r)?
                } else {
                    let h = len(f
                        .args
                        .get("h")
                        .ok_or_else(|| ferr("cylinder needs h= (or from=/to=)".into()))?)
                    .map_err(ferr)?;
                    let centre = match f.args.get("at") {
                        Some(v) => vec3_len(v).map_err(ferr)?,
                        None => [0.0; 3],
                    };
                    let axis = match f.args.get("axis") {
                        Some(v) => axis(v).map_err(ferr)?,
                        None => [0.0, 0.0, 1.0],
                    };
                    k.cylinder(centre, axis, r, h)?
                };
                bodies.push((name_of(f), solid));
            }
            "tube" => {
                let od = len(f
                    .args
                    .get("od")
                    .ok_or_else(|| ferr("tube needs od=".into()))?)
                .map_err(ferr)?;
                let wall = len(f
                    .args
                    .get("wall")
                    .ok_or_else(|| ferr("tube needs wall=".into()))?)
                .map_err(ferr)?;
                let a = vec3_len(
                    f.args
                        .get("from")
                        .ok_or_else(|| ferr("tube needs from=".into()))?,
                )
                .map_err(ferr)?;
                let b = vec3_len(
                    f.args
                        .get("to")
                        .ok_or_else(|| ferr("tube needs to=".into()))?,
                )
                .map_err(ferr)?;
                if crate::length(sub(b, a)) == 0.0 {
                    return Err(ferr("tube from= and to= coincide".into()));
                }
                let solid = k.tube_between(a, b, od, wall)?;
                bodies.push((name_of(f), solid));
            }
            "box_tube" => {
                let size = vec3_len(
                    f.args
                        .get("size")
                        .ok_or_else(|| ferr("box_tube needs size=(length, width, height)".into()))?,
                )
                .map_err(ferr)?;
                let wall = len(f.args.get("wall").ok_or_else(|| ferr("box_tube needs wall=".into()))?).map_err(ferr)?;
                let centre = match f.args.get("at") {
                    Some(v) => vec3_len(v).map_err(ferr)?,
                    None => [0.0; 3],
                };
                let solid = k.box_tube(size, wall, centre)?;
                bodies.push((name_of(f), solid));
            }
            "union" | "subtract" | "intersect" => {
                let op = f.op.as_str();
                if f.args.is_empty() && f.name.is_none() {
                    if op != "union" {
                        return Err(ferr(format!("{op} needs a=X b=Y")));
                    }
                    if bodies.is_empty() {
                        return Err(ferr("union with nothing built yet".into()));
                    }
                    let mut it = bodies.drain(..);
                    let (_, mut acc) = it.next().unwrap();
                    for (_, s) in it {
                        acc = k.union(&acc, &s)?;
                    }
                    bodies.push(("result".into(), acc));
                } else {
                    let a_name = str_arg(f, "a").ok_or_else(|| ferr(format!("{op} needs a=")))?;
                    let b_name = str_arg(f, "b").ok_or_else(|| ferr(format!("{op} needs b=")))?;
                    let a = take_body(&mut bodies, &a_name)
                        .ok_or_else(|| ferr(format!("no body named `{a_name}`")))?;
                    let b = take_body(&mut bodies, &b_name)
                        .ok_or_else(|| ferr(format!("no body named `{b_name}`")))?;
                    let solid = match op {
                        "union" => k.union(&a, &b)?,
                        "subtract" => k.subtract(&a, &b)?,
                        _ => k.intersect(&a, &b)?,
                    };
                    bodies.push((name_of(f), solid));
                }
            }
            "mirror" => {
                let of = str_arg(f, "of").ok_or_else(|| ferr("mirror needs of=".into()))?;
                let plane = str_arg(f, "plane").unwrap_or_else(|| "xz".into());
                let normal = match plane.as_str() {
                    "xy" => [0.0, 0.0, 1.0],
                    "xz" => [0.0, 1.0, 0.0],
                    "yz" => [1.0, 0.0, 0.0],
                    other => return Err(ferr(format!("unknown mirror plane `{other}`"))),
                };
                let src = bodies
                    .iter()
                    .find(|(n, _)| *n == of)
                    .map(|(_, s)| s.clone())
                    .ok_or_else(|| ferr(format!("no body named `{of}`")))?;
                let solid = k.mirrored(&src, [0.0; 3], normal)?;
                bodies.push((name_of(f), solid));
            }
            "hull" => {
                let src = built
                    .levels
                    .get("manufacture")
                    .or_else(|| built.levels.get("display"))
                    .ok_or_else(|| {
                        ferr("hull needs a manufacture or display level built first".into())
                    })?;
                let solid = match k.hull(src) {
                    Ok(s) => s,
                    Err(GeomError::Unsupported(_)) => src.clone(),
                    Err(e) => return Err(e),
                };
                bodies.push((name_of(f), solid));
            }
            other => return Err(ferr(format!("unknown geometry feature `{other}`"))),
        }
    }

    if bodies.is_empty() {
        return Err(GeomError::Feature(
            lvl.level.clone(),
            "level produced no geometry".into(),
        ));
    }
    let mut it = bodies.into_iter();
    let (_, mut acc) = it.next().unwrap();
    for (_, s) in it {
        acc = k.union(&acc, &s)?;
    }
    Ok(acc)
}

fn take_body<S>(bodies: &mut Vec<(String, S)>, name: &str) -> Option<S> {
    let i = bodies.iter().position(|(n, _)| n == name)?;
    Some(bodies.remove(i).1)
}

fn str_arg(f: &ResolvedFeature, key: &str) -> Option<String> {
    match f.args.get(key)? {
        Value::Str(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn len(v: &Value) -> std::result::Result<f64, String> {
    let q = v
        .as_quantity()
        .ok_or_else(|| format!("expected a length, found {}", v.type_name()))?;
    if q.dim == Dim::LENGTH {
        Ok(q.value)
    } else if q.dim.is_dimensionless() {
        Err(format!(
            "length `{q}` has no unit (write e.g. `{} mm`)",
            q.value
        ))
    } else {
        Err(format!("expected a length, found {q}"))
    }
}

fn vec3_len(v: &Value) -> std::result::Result<Vec3, String> {
    let q = wmds_model::as_length_vec3(v)?;
    Ok([q[0].value, q[1].value, q[2].value])
}

fn axis(v: &Value) -> std::result::Result<Vec3, String> {
    match v {
        Value::Str(s) => match s.trim() {
            "x" | "+x" => Ok([1.0, 0.0, 0.0]),
            "y" | "+y" => Ok([0.0, 1.0, 0.0]),
            "z" | "+z" => Ok([0.0, 0.0, 1.0]),
            "-x" => Ok([-1.0, 0.0, 0.0]),
            "-y" => Ok([0.0, -1.0, 0.0]),
            "-z" => Ok([0.0, 0.0, -1.0]),
            other => Err(format!("unknown axis `{other}`")),
        },
        Value::Tuple(t) if t.len() == 3 => {
            let c: Vec<Quantity> = t.iter().filter_map(|x| x.as_quantity()).collect();
            if c.len() != 3 {
                return Err("axis tuple must be numeric".into());
            }
            crate::normalize([c[0].value, c[1].value, c[2].value]).map_err(|e| e.to_string())
        }
        other => Err(format!(
            "axis must be x/y/z or a tuple, found {}",
            other.type_name()
        )),
    }
}
