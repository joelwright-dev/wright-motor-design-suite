//! Bill of materials, cut list and fastener schedule.

use indexmap::IndexMap;
use wmds_geom::PartMass;
use wmds_model::{Library, ResolvedAssembly};
use wmds_schema::MfgMethodDef;

/// The manufacturing route chosen for a part at a given volume.
#[derive(Debug, Clone)]
pub struct MakePlan {
    pub method: String,
    /// The scale range the method declares, as written.
    pub scale: Option<String>,
    pub fixed_cost: Option<f64>,
    pub unit_cost: Option<f64>,
    pub exports: Vec<String>,
    /// Set when more than one method covered this volume and the cheapest was taken.
    pub alternative: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BomLine {
    pub source_id: String,
    pub quantity: usize,
    /// Which handed versions are needed, e.g. two left and two right.
    pub variants: Vec<(String, usize)>,
    pub material: Option<String>,
    pub unit_mass: Option<f64>,
    pub total_mass: Option<f64>,
    pub plan: Option<MakePlan>,
    /// True when this part is bought in rather than made.
    pub purchased: bool,
}

/// Every distinct part in the vehicle, with how many are needed.
///
/// Parts are grouped by what they are and how they are configured, not just by name: two arms of
/// the same design at different lengths are two different things to make, and lumping them
/// together would produce a bill nobody could order from.
pub fn bill_of_materials(
    asm: &ResolvedAssembly,
    lib: &Library,
    masses: &[PartMass],
    volume: u32,
) -> Vec<BomLine> {
    let mut groups: IndexMap<String, BomLine> = IndexMap::new();
    for inst in &asm.instances {
        // The grouping key is everything that changes what has to be made. Handedness is
        // deliberately not in it: a left and a right come off the same drawing and belong on one
        // line, with the split shown.
        let config: Vec<String> = inst
            .primitive
            .params
            .iter()
            .map(|(k, v)| format!("{k}={}", v.to_string()))
            .collect();
        let key = format!("{}|{}", inst.source_id, config.join(","));
        let mass = masses
            .iter()
            .find(|m| m.id == inst.id)
            .and_then(|m| m.mass);

        let hand = inst
            .primitive
            .variants
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(" ");

        let line = groups.entry(key).or_insert_with(|| BomLine {
            source_id: inst.source_id.clone(),
            quantity: 0,
            variants: Vec::new(),
            material: inst.primitive.material.clone(),
            unit_mass: mass,
            total_mass: None,
            plan: choose_method(lib, &inst.source_id, volume),
            purchased: false,
        });
        line.quantity += 1;
        if !hand.is_empty() {
            match line.variants.iter_mut().find(|(h, _)| *h == hand) {
                Some((_, n)) => *n += 1,
                None => line.variants.push((hand, 1)),
            }
        }
    }
    for line in groups.values_mut() {
        line.total_mass = line.unit_mass.map(|m| m * line.quantity as f64);
        line.purchased = line
            .plan
            .as_ref()
            .map(|p| p.method == "purchased")
            .unwrap_or(false);
    }
    let mut out: Vec<BomLine> = groups.into_values().collect();
    out.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    out
}

/// Pick the manufacturing method for a part at this build volume.
///
/// A method declares the scale it suits, such as `1..500` for machining or `500..*` for casting.
/// When more than one covers the volume, the one with the lower total cost over the whole
/// programme wins, and the runner-up is recorded so the choice can be argued with.
fn choose_method(lib: &Library, source_id: &str, volume: u32) -> Option<MakePlan> {
    let def = lib.primitive(source_id)?;
    let n = volume.max(1) as f64;
    let mut candidates: Vec<(f64, &MfgMethodDef)> = Vec::new();
    for m in &def.manufacturing {
        if !scale_covers(m.scale.as_deref(), volume) {
            continue;
        }
        let fixed = m.cost.as_ref().and_then(|c| c.fixed.as_ref()).and_then(literal);
        let unit = m
            .cost
            .as_ref()
            .and_then(|c| c.per_unit.as_ref())
            .and_then(literal);
        // A method with no cost still has to be selectable, so it sorts as if it were free and
        // the missing figures are reported rather than invented.
        let total = fixed.unwrap_or(0.0) + unit.unwrap_or(0.0) * n;
        candidates.push((total, m));
    }
    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (_, best) = candidates.first()?;
    Some(MakePlan {
        method: best.method.clone(),
        scale: best.scale.clone(),
        fixed_cost: best.cost.as_ref().and_then(|c| c.fixed.as_ref()).and_then(literal),
        unit_cost: best
            .cost
            .as_ref()
            .and_then(|c| c.per_unit.as_ref())
            .and_then(literal),
        exports: best.exports.clone(),
        alternative: candidates.get(1).map(|(_, m)| m.method.clone()),
    })
}

/// Does a scale range such as `1..500`, `500..*` or `1..*` contain this volume?
fn scale_covers(scale: Option<&str>, volume: u32) -> bool {
    let Some(s) = scale else { return true };
    let Some((lo, hi)) = s.split_once("..") else {
        return true;
    };
    let lo: u32 = lo.trim().parse().unwrap_or(0);
    let hi = hi.trim();
    let hi: u32 = if hi == "*" || hi.is_empty() {
        u32::MAX
    } else {
        hi.parse().unwrap_or(u32::MAX)
    };
    volume >= lo && volume <= hi
}

fn literal(e: &wmds_expr::Expr) -> Option<f64> {
    let mut e = e;
    while let wmds_expr::Expr::TextOr(_, inner) = e {
        e = inner;
    }
    match e {
        wmds_expr::Expr::Num(q) => Some(q.value),
        _ => None,
    }
}

// ------------------------------------------------------------------------------- cut list

#[derive(Debug, Clone)]
pub struct CutLine {
    pub source_id: String,
    pub body: String,
    pub section: String,
    /// Cut length in metres.
    pub length: f64,
    pub quantity: usize,
    pub material: Option<String>,
}

/// Every piece of tube or section that has to be cut to length.
///
/// Read straight out of the geometry: a `tube` feature runs between two points, and the distance
/// between them is what someone has to cut. This is the difference between a design and
/// something a workshop can start on.
pub fn cut_list(asm: &ResolvedAssembly, _lib: &Library) -> Vec<CutLine> {
    let mut groups: IndexMap<String, CutLine> = IndexMap::new();
    for inst in &asm.instances {
        for lvl in &inst.primitive.geometry {
            if lvl.level != "manufacture" && inst.primitive.geometry.len() > 1 {
                continue;
            }
            for f in &lvl.features {
                let (section, length) = match f.op.as_str() {
                    "tube" => {
                        let od = num(f.args.get("od"));
                        let wall = num(f.args.get("wall"));
                        let a = vec3(f.args.get("from"));
                        let b = vec3(f.args.get("to"));
                        match (od, wall, a, b) {
                            (Some(od), Some(w), Some(a), Some(b)) => (
                                format!("round {:.0} x {:.1} wall", od * 1e3, w * 1e3),
                                distance(a, b),
                            ),
                            _ => continue,
                        }
                    }
                    "box_tube" => {
                        let size = vec3(f.args.get("size"));
                        let wall = num(f.args.get("wall"));
                        match (size, wall) {
                            (Some(s), Some(w)) => (
                                format!(
                                    "box {:.0} x {:.0} x {:.1} wall",
                                    s[2] * 1e3,
                                    s[1] * 1e3,
                                    w * 1e3
                                ),
                                s[0],
                            ),
                            _ => continue,
                        }
                    }
                    _ => continue,
                };
                if length <= 0.0 {
                    continue;
                }
                let body = f.name.clone().unwrap_or_else(|| f.op.clone());
                let key = format!("{}|{body}|{section}|{:.4}", inst.source_id, length);
                let e = groups.entry(key).or_insert_with(|| CutLine {
                    source_id: inst.source_id.clone(),
                    body,
                    section,
                    length,
                    quantity: 0,
                    material: inst.primitive.material.clone(),
                });
                e.quantity += 1;
            }
        }
    }
    let mut out: Vec<CutLine> = groups.into_values().collect();
    out.sort_by(|a, b| {
        (a.section.as_str(), a.source_id.as_str())
            .cmp(&(b.section.as_str(), b.source_id.as_str()))
    });
    out
}

fn num(v: Option<&wmds_expr::Value>) -> Option<f64> {
    v?.as_quantity().map(|q| q.value)
}

fn vec3(v: Option<&wmds_expr::Value>) -> Option<[f64; 3]> {
    match v? {
        wmds_expr::Value::Tuple(items) if items.len() == 3 => {
            let mut out = [0.0; 3];
            for (i, it) in items.iter().enumerate() {
                out[i] = it.as_quantity()?.value;
            }
            Some(out)
        }
        _ => None,
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

// ------------------------------------------------------------------------ fastener schedule

#[derive(Debug, Clone)]
pub struct Fastener {
    pub kind: String,
    pub size: String,
    pub grade: String,
    pub quantity: u32,
    /// Newton metres, when every joint using this fastener agrees on one figure.
    pub torque: Option<f64>,
    pub nut: Option<String>,
    pub washer: Option<String>,
    pub thread_locker: Option<String>,
    /// True when at least one joint using it is made by the person building the vehicle.
    pub kit: bool,
    /// The joints that use it, so a missing torque can be traced.
    pub joints: Vec<String>,
}

/// Every fastener in the vehicle, grouped by what you would order.
pub fn fasteners(asm: &ResolvedAssembly) -> Vec<Fastener> {
    let mut groups: IndexMap<String, Fastener> = IndexMap::new();
    for m in &asm.mates {
        let Some(f) = &m.fasteners else { continue };
        let torque = f.torque.as_ref().and_then(literal);
        // Torque is part of the key, not something averaged away. Two M12 bolts tightened to
        // different figures are two lines on the schedule, because one line with "varies" on it
        // is useless to the person holding the wrench.
        let key = format!(
            "{}|{}|{}|{}",
            f.kind,
            f.size,
            f.grade,
            torque.map(|t| format!("{t:.1}")).unwrap_or_default()
        );
        let e = groups.entry(key).or_insert_with(|| Fastener {
            kind: f.kind.clone(),
            size: f.size.clone(),
            grade: f.grade.clone(),
            quantity: 0,
            torque,
            nut: f.nut.clone(),
            washer: f.washer.clone(),
            thread_locker: f.thread_locker.clone(),
            kit: false,
            joints: Vec::new(),
        });
        e.quantity += f.quantity;
        e.joints.push(m.id.clone());
        if m.stage == wmds_schema::Stage::Kit {
            e.kit = true;
        }
    }
    let mut out: Vec<Fastener> = groups.into_values().collect();
    out.sort_by(|a, b| {
        (a.kind.as_str(), a.size.as_str(), a.grade.as_str())
            .cmp(&(b.kind.as_str(), b.size.as_str(), b.grade.as_str()))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_range_is_read_the_way_it_is_written() {
        assert!(scale_covers(Some("1..500"), 1));
        assert!(scale_covers(Some("1..500"), 500));
        assert!(!scale_covers(Some("1..500"), 501));
        assert!(scale_covers(Some("500..*"), 500));
        assert!(scale_covers(Some("500..*"), 1_000_000));
        assert!(!scale_covers(Some("500..*"), 499));
        // No range means the method suits any volume.
        assert!(scale_covers(None, 1));
        assert!(scale_covers(Some("1..*"), 42));
    }
}
