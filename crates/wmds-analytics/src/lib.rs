//! Tier 0: the vehicle numbers that follow directly from mass and geometry.
//!
//! Everything here is closed form and costs microseconds, so it can run on every change. That is
//! the point of it: a designer should see the axle loads move while they are still deciding
//! where to put the battery, not after a simulation queue.
//!
//! Nothing here is a substitute for Tier 1 or Tier 2. A static weight distribution says nothing
//! about how a car behaves in a corner. Results are labelled Tier 0 wherever they are shown.

use std::collections::BTreeMap;

use wmds_model::ResolvedAssembly;

/// A mass with a position, in SI units: kilograms and metres.
#[derive(Debug, Clone, Copy)]
pub struct MassPoint {
    pub mass: f64,
    pub at: [f64; 3],
}

/// One axle, found by grouping the wheels by longitudinal position.
#[derive(Debug, Clone)]
pub struct Axle {
    /// Longitudinal position of the wheel centres, metres.
    pub x: f64,
    /// Distance between the wheel centre planes, metres.
    pub track: f64,
    pub wheel_count: usize,
    /// Share of the total mass this axle carries, 0 to 1.
    pub load_fraction: f64,
    /// Mass on this axle, kilograms.
    pub load: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Tier0 {
    /// Total mass considered, kilograms.
    pub mass: f64,
    /// Centre of gravity in vehicle coordinates, metres.
    pub cg: [f64; 3],
    /// Axles front to rear.
    pub axles: Vec<Axle>,
    /// Distance from the front axle to the rear, metres. `None` unless there are exactly two.
    pub wheelbase: Option<f64>,
    /// Ground plane: the lowest point of the tyres, metres.
    pub ground_z: Option<f64>,
    /// Centre of gravity height above the ground, metres.
    pub cg_height: Option<f64>,
    /// Half the track divided by the centre of gravity height. Higher is harder to roll over.
    pub static_stability_factor: Option<f64>,
    /// Bounding box of the whole vehicle, metres.
    pub bounds: Option<([f64; 3], [f64; 3])>,
    /// Distance from the front axle forward to the foremost point, metres.
    pub front_overhang: Option<f64>,
    /// Distance from the rear axle back to the rearmost point, metres.
    pub rear_overhang: Option<f64>,
    /// Lowest point of anything that is not a wheel or tyre, above the ground, metres.
    pub ground_clearance: Option<f64>,
    /// Anything the calculation could not do, and why.
    pub notes: Vec<String>,
}

/// Wheel and tyre positions taken from the model, in metres.
#[derive(Debug, Clone, Default)]
pub struct RollingStock {
    /// Wheel centres.
    pub wheels: Vec<[f64; 3]>,
    /// Unloaded tyre radius, if any tyre declares one.
    pub tyre_radius: Option<f64>,
}

/// Find the wheels and tyres in a resolved assembly.
///
/// Wheels and tyres are recognised by the category of the primitive they came from, so a new
/// wheel primitive is picked up without touching this code as long as it says it is a wheel.
pub fn rolling_stock(asm: &ResolvedAssembly) -> RollingStock {
    let mut out = RollingStock::default();
    for i in &asm.instances {
        if i.source_id.starts_with("wheels/wheel") {
            out.wheels.push(i.placement.translation);
        }
        if i.source_id.starts_with("wheels/tyre") && out.tyre_radius.is_none() {
            // The tyre primitive derives its radius from its size designation.
            if let Some(r) = i
                .primitive
                .params
                .get("radius")
                .and_then(|v| v.as_quantity())
            {
                out.tyre_radius = Some(r.value);
            }
        }
    }
    out
}

/// Compute the Tier 0 figures.
///
/// `masses` is everything that weighs anything, already placed. `bounds` is the bounding box of
/// the built geometry, and `structure_bounds` excludes the wheels so that ground clearance means
/// what a person means by it.
pub fn compute(
    masses: &[MassPoint],
    rolling: &RollingStock,
    bounds: Option<([f64; 3], [f64; 3])>,
    structure_low_z: Option<f64>,
) -> Tier0 {
    let mut t = Tier0 {
        bounds,
        ..Default::default()
    };

    let total: f64 = masses.iter().map(|m| m.mass).sum();
    t.mass = total;
    if total > 0.0 {
        for i in 0..3 {
            t.cg[i] = masses.iter().map(|m| m.mass * m.at[i]).sum::<f64>() / total;
        }
    } else {
        t.notes
            .push("nothing has a mass, so there is no centre of gravity".into());
    }

    // Group wheels into axles by longitudinal position. A 50 mm window is far tighter than any
    // real axle offset and far looser than the numerical noise.
    const AXLE_WINDOW: f64 = 0.05;
    let mut groups: BTreeMap<i64, Vec<[f64; 3]>> = BTreeMap::new();
    for w in &rolling.wheels {
        let key = (w[0] / AXLE_WINDOW).round() as i64;
        groups.entry(key).or_default().push(*w);
    }
    for (_, wheels) in groups {
        let x = wheels.iter().map(|w| w[0]).sum::<f64>() / wheels.len() as f64;
        let (min_y, max_y) = wheels.iter().fold((f64::MAX, f64::MIN), |(lo, hi), w| {
            (lo.min(w[1]), hi.max(w[1]))
        });
        t.axles.push(Axle {
            x,
            track: if wheels.len() > 1 { max_y - min_y } else { 0.0 },
            wheel_count: wheels.len(),
            load_fraction: 0.0,
            load: 0.0,
        });
    }
    t.axles.sort_by(|a, b| a.x.total_cmp(&b.x));

    match t.axles.len() {
        0 => t.notes.push("no wheels, so there are no axle loads".into()),
        2 => {
            let (front, rear) = (t.axles[0].x, t.axles[1].x);
            let wb = rear - front;
            if wb.abs() < 1e-6 {
                t.notes.push("both axles are at the same place".into());
            } else {
                t.wheelbase = Some(wb);
                // Static balance about each axle. The fraction on the rear axle is how far the
                // centre of gravity sits toward it.
                let rear_fraction = ((t.cg[0] - front) / wb).clamp(0.0, 1.0);
                t.axles[0].load_fraction = 1.0 - rear_fraction;
                t.axles[1].load_fraction = rear_fraction;
                for a in &mut t.axles {
                    a.load = a.load_fraction * total;
                }
                if t.cg[0] < front || t.cg[0] > rear {
                    t.notes.push(
                        "the centre of gravity is outside the wheelbase, so one axle would lift; \
                         the load split has been clamped and is not meaningful"
                            .into(),
                    );
                }
            }
        }
        n => t.notes.push(format!(
            "{n} axles found; axle loads are only computed for two"
        )),
    }

    // The ground is where the tyres touch it.
    if let Some(r) = rolling.tyre_radius {
        if let Some(lowest) = rolling
            .wheels
            .iter()
            .map(|w| w[2])
            .fold(None, |acc: Option<f64>, z| {
                Some(acc.map_or(z, |a: f64| a.min(z)))
            })
        {
            let ground = lowest - r;
            t.ground_z = Some(ground);
            t.cg_height = Some(t.cg[2] - ground);
            if let Some(low) = structure_low_z {
                t.ground_clearance = Some(low - ground);
            }
        }
    } else if !rolling.wheels.is_empty() {
        t.notes
            .push("no tyre declares a radius, so the ground plane is unknown".into());
    }

    // Static stability factor: half the widest track over the centre of gravity height. A rough
    // rollover indicator, and only that: it ignores suspension, tyres and everything dynamic.
    if let (Some(h), Some(track)) = (
        t.cg_height,
        t.axles
            .iter()
            .map(|a| a.track)
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a: f64| a.max(v)))
            }),
    ) {
        if h > 1e-6 && track > 1e-6 {
            t.static_stability_factor = Some(track / 2.0 / h);
        }
    }

    if let (Some((lo, hi)), 2) = (bounds, t.axles.len()) {
        t.front_overhang = Some(t.axles[0].x - lo[0]);
        t.rear_overhang = Some(hi[0] - t.axles[1].x);
    }

    t
}

impl Tier0 {
    pub fn front_axle(&self) -> Option<&Axle> {
        self.axles.first()
    }

    pub fn rear_axle(&self) -> Option<&Axle> {
        self.axles.last()
    }

    /// Overall length, width and height in metres.
    pub fn overall(&self) -> Option<[f64; 3]> {
        self.bounds
            .map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheels(front_x: f64, rear_x: f64, half_track: f64) -> RollingStock {
        RollingStock {
            wheels: vec![
                [front_x, half_track, 0.0],
                [front_x, -half_track, 0.0],
                [rear_x, half_track, 0.0],
                [rear_x, -half_track, 0.0],
            ],
            tyre_radius: Some(0.3),
        }
    }

    #[test]
    fn two_axles_and_a_centred_mass_split_evenly() {
        let r = wheels(-0.8, 1.6, 0.63);
        let m = [MassPoint {
            mass: 1000.0,
            at: [0.4, 0.0, 0.5],
        }];
        let t = compute(&m, &r, None, None);
        assert_eq!(t.axles.len(), 2);
        assert!((t.wheelbase.unwrap() - 2.4).abs() < 1e-9);
        assert!((t.axles[0].track - 1.26).abs() < 1e-9);
        // The centre of gravity sits exactly halfway, so the load splits in half.
        assert!(
            (t.axles[0].load - 500.0).abs() < 1e-6,
            "front {}",
            t.axles[0].load
        );
        assert!((t.axles[1].load - 500.0).abs() < 1e-6);
    }

    #[test]
    fn mass_forward_loads_the_front_axle() {
        let r = wheels(-0.8, 1.6, 0.63);
        // A quarter of the wheelbase behind the front axle.
        let m = [MassPoint {
            mass: 1000.0,
            at: [-0.2, 0.0, 0.5],
        }];
        let t = compute(&m, &r, None, None);
        assert!(
            (t.axles[0].load_fraction - 0.75).abs() < 1e-9,
            "{}",
            t.axles[0].load_fraction
        );
        assert!((t.axles[1].load_fraction - 0.25).abs() < 1e-9);
    }

    #[test]
    fn ground_and_stability_come_from_the_tyres() {
        let r = wheels(-0.8, 1.6, 0.63);
        let m = [MassPoint {
            mass: 1000.0,
            at: [0.4, 0.0, 0.2],
        }];
        let t = compute(&m, &r, None, Some(-0.12));
        // Wheel centres at z = 0 with a 300 mm tyre radius puts the ground at -300 mm.
        assert!((t.ground_z.unwrap() + 0.3).abs() < 1e-9);
        assert!((t.cg_height.unwrap() - 0.5).abs() < 1e-9);
        // Half of a 1.26 m track over a 0.5 m centre of gravity height.
        assert!((t.static_stability_factor.unwrap() - 1.26) < 1e-9);
        // Structure at -120 mm is 180 mm above the ground.
        assert!((t.ground_clearance.unwrap() - 0.18).abs() < 1e-9);
    }

    #[test]
    fn a_centre_of_gravity_outside_the_wheelbase_is_called_out() {
        let r = wheels(-0.8, 1.6, 0.63);
        let m = [MassPoint {
            mass: 1000.0,
            at: [2.5, 0.0, 0.5],
        }];
        let t = compute(&m, &r, None, None);
        assert!(
            t.notes.iter().any(|n| n.contains("outside the wheelbase")),
            "{:?}",
            t.notes
        );
        // Clamped rather than reported as a negative load on the front axle.
        assert!(t.axles[0].load >= 0.0);
    }

    #[test]
    fn no_wheels_says_so_rather_than_inventing_an_answer() {
        let t = compute(
            &[MassPoint {
                mass: 100.0,
                at: [0.0; 3],
            }],
            &RollingStock::default(),
            None,
            None,
        );
        assert!(t.wheelbase.is_none());
        assert!(t.notes.iter().any(|n| n.contains("no wheels")));
        assert!((t.mass - 100.0).abs() < 1e-9);
    }

    #[test]
    fn overhangs_come_from_the_bounding_box() {
        let r = wheels(-0.8, 1.6, 0.63);
        let m = [MassPoint {
            mass: 1000.0,
            at: [0.4, 0.0, 0.5],
        }];
        let bounds = Some(([-1.5, -0.8, -0.3], [2.2, 0.8, 1.4]));
        let t = compute(&m, &r, bounds, None);
        assert!((t.front_overhang.unwrap() - 0.7).abs() < 1e-9);
        assert!((t.rear_overhang.unwrap() - 0.6).abs() < 1e-9);
        let o = t.overall().unwrap();
        assert!((o[0] - 3.7).abs() < 1e-9 && (o[1] - 1.6).abs() < 1e-9);
    }
}
