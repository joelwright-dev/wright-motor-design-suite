//! Cutting a vehicle into a chain of masses and crushable elements.
//!
//! The vehicle is sliced along its length. Each slice carries the mass that lives in it, and the
//! structure that spans between two slices becomes a crushable element whose force comes from
//! the material actually there, not from a number somebody typed.
//!
//! The force a crushing structure sustains is its specific energy absorption times the mass it
//! consumes per unit of length:
//!
//!   F = SEA x density x area
//!
//! which has units of energy per length, which is force. The stroke available is the length of
//! the element times the crush efficiency, because debris packs up and the last part of the
//! length is not usable.

use wmds_model::{Library, ResolvedAssembly};

use crate::Standard;

/// One slice of the vehicle, carrying the mass that lives in it.
#[derive(Debug, Clone)]
pub struct Slice {
    /// Front face of the slice, in vehicle coordinates, metres.
    pub x: f64,
    pub mass: f64,
    /// What is in it, for the report.
    pub contents: Vec<String>,
    /// True when this slice is the occupant compartment, which is what must not be crushed.
    pub occupied: bool,
}

/// A crushable structure between two slices.
#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    /// Steady crush force, newtons.
    pub force: f64,
    /// The initial peak, before it settles. A structure with no trigger peaks hard.
    pub peak_force: f64,
    /// How far it can crush before it packs solid, metres.
    pub stroke: f64,
    /// Energy it can absorb over that stroke, joules.
    pub capacity: f64,
    /// True when every material in it has had its crush behaviour validated by test.
    pub validated: bool,
    /// Which materials contributed, and how much of the force each gave.
    pub contributions: Vec<(String, f64)>,
}

#[derive(Debug, Clone)]
pub struct CrashModel {
    pub vehicle: String,
    pub slices: Vec<Slice>,
    pub elements: Vec<Element>,
    /// Total mass in the model, kilograms.
    pub mass: f64,
    /// Where the front of the occupant compartment is, metres.
    pub compartment_front: f64,
    /// Anything the model could not read and had to assume or leave out.
    pub notes: Vec<String>,
}

/// Build a crash model from a resolved vehicle.
///
/// `masses` gives each part's mass and centroid, because working those out needs a geometry
/// kernel and this crate does not depend on one.
pub fn build_model(
    asm: &ResolvedAssembly,
    lib: &Library,
    masses: &[wmds_geom::PartMass],
    standard: Standard,
    slice_count: usize,
) -> CrashModel {
    let mut notes = Vec::new();

    // The extent of the vehicle along its length, from the parts that have geometry.
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for m in masses {
        lo = lo.min(m.centroid[0]);
        hi = hi.max(m.centroid[0]);
    }
    if !lo.is_finite() || hi <= lo {
        notes.push("the vehicle has no geometry to slice".into());
        return CrashModel {
            vehicle: asm.id.clone(),
            slices: Vec::new(),
            elements: Vec::new(),
            mass: 0.0,
            compartment_front: 0.0,
            notes,
        };
    }
    // Reach a little ahead of the foremost part, so the front slice is the bumper rather than
    // the middle of the foremost component.
    lo -= 0.15;
    hi += 0.15;

    // The occupant compartment starts where the people are. Point masses know where they sit.
    let compartment_front = asm
        .point_masses
        .iter()
        .filter(|p| p.id.contains("driver") || p.id.contains("passenger") || p.state == "laden")
        .map(|p| p.at[0].value)
        .fold(f64::INFINITY, f64::min);
    let compartment_front = if compartment_front.is_finite() {
        // Allow for the space in front of a seated person: knees, pedals and the bulkhead.
        compartment_front - 0.75
    } else {
        notes.push(
            "no occupant masses in the vehicle, so the compartment was taken as the rear two \
             thirds of the wheelbase"
                .into(),
        );
        lo + (hi - lo) * 0.35
    };

    let n = slice_count.max(4);
    let step = (hi - lo) / n as f64;
    let mut slices: Vec<Slice> = (0..=n)
        .map(|i| Slice {
            x: lo + step * i as f64,
            mass: 0.0,
            contents: Vec::new(),
            occupied: lo + step * i as f64 >= compartment_front,
        })
        .collect();

    // Put each part's mass in the slice it sits in.
    let mut total = 0.0;
    let put = |slices: &mut Vec<Slice>, x: f64, mass: f64, what: &str| {
        let i = (((x - lo) / step).floor() as isize).clamp(0, slices.len() as isize - 1) as usize;
        slices[i].mass += mass;
        if !slices[i].contents.iter().any(|c| c == what) {
            slices[i].contents.push(what.to_string());
        }
    };
    for m in masses {
        let Some(kg) = m.mass else { continue };
        total += kg;
        let what = m.id.split('.').next_back().unwrap_or(&m.id).to_string();
        put(&mut slices, m.centroid[0], kg, &what);
    }
    // Point masses are most of a real vehicle at this stage, and leaving them out would make the
    // crash far gentler than it would be.
    for p in &asm.point_masses {
        total += p.mass.value;
        put(&mut slices, p.at[0].value, p.mass.value, &p.id);
    }

    // The structure between each pair of slices. Only parts that run along the vehicle can
    // carry a crush load, which in practice means the chassis rails.
    let mut elements = Vec::new();
    for i in 0..n {
        let front = slices[i].x;
        let back = slices[i + 1].x;
        let mut force = 0.0;
        let mut peak = 0.0;
        let mut validated = true;
        let mut contributions: Vec<(String, f64)> = Vec::new();
        let mut any = false;

        for inst in &asm.instances {
            // Only longitudinal structure counts. A cross-member does not resist a frontal
            // impact by crushing along its own length.
            if !is_longitudinal(&inst.source_id) {
                continue;
            }
            let Some(area) = section_area(inst) else {
                continue;
            };
            let (inst_lo, inst_hi) = longitudinal_extent(inst);
            // How much of this element's length this part spans.
            let overlap = (inst_hi.min(back) - inst_lo.max(front)).max(0.0);
            if overlap <= 0.0 {
                continue;
            }
            let Some(mat) = inst.primitive.material.as_deref() else {
                continue;
            };
            let Some(def) = lib.materials.get(mat) else {
                continue;
            };
            let Some(crush) = &def.crush else {
                notes.push(format!(
                    "{mat} has no crush behaviour, so {} contributes nothing to the structure",
                    inst.source_id
                ));
                continue;
            };
            any = true;
            if !crush.validated {
                validated = false;
            }
            let f = crush.sea.value * def.density_si() * area * standard.overlap();
            force += f;
            peak += f * crush.trigger_ratio;
            match contributions.iter_mut().find(|(m, _)| m == mat) {
                Some((_, v)) => *v += f,
                None => contributions.push((mat.to_string(), f)),
            }
        }

        if !any {
            continue;
        }
        // Stroke: the length of this element, less what the debris takes up.
        let efficiency = 0.70;
        let stroke = (back - front) * efficiency;
        elements.push(Element {
            name: format!("{:.0} to {:.0} mm", front * 1e3, back * 1e3),
            force,
            peak_force: peak,
            stroke,
            capacity: force * stroke,
            validated,
            contributions,
        });
    }

    notes.sort();
    notes.dedup();
    CrashModel {
        vehicle: asm.id.clone(),
        slices,
        elements,
        mass: total,
        compartment_front,
        notes,
    }
}

/// Does this part run along the vehicle, so that it can crush end on?
fn is_longitudinal(source_id: &str) -> bool {
    source_id.contains("rail")
}

/// The cross-sectional area of a longitudinal member, from its own parameters.
fn section_area(inst: &wmds_model::PlacedInstance) -> Option<f64> {
    let p = &inst.primitive.params;
    let get = |k: &str| p.get(k).and_then(|v| v.as_quantity()).map(|q| q.value);
    // A box section: the wall all the way round.
    match (get("height"), get("width"), get("wall")) {
        (Some(h), Some(w), Some(t)) if h > 0.0 && w > 0.0 && t > 0.0 => {
            Some(h * w - (h - 2.0 * t).max(0.0) * (w - 2.0 * t).max(0.0))
        }
        _ => None,
    }
}

/// How far along the vehicle this part reaches.
fn longitudinal_extent(inst: &wmds_model::PlacedInstance) -> (f64, f64) {
    let len = inst
        .primitive
        .params
        .get("length")
        .and_then(|v| v.as_quantity())
        .map(|q| q.value)
        .unwrap_or(0.0);
    let c = inst.placement.translation[0];
    (c - len / 2.0, c + len / 2.0)
}
