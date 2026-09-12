//! The vehicle a simulation runs on, and where each number in it came from.
//!
//! Every field is either read from the resolved model or assumed. Which of the two is recorded,
//! because a handling result carries the authority of its weakest input and the reader has to be
//! able to see which inputs those were.

use wmds_model::ResolvedAssembly;

use crate::tyre::Tyre;

/// Where a number came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Measured off the model.
    Model,
    /// Not in the model, so a stated default was used.
    Assumed(&'static str),
}

#[derive(Debug, Clone)]
pub struct Input {
    pub name: &'static str,
    pub value: f64,
    pub unit: &'static str,
    pub source: Source,
}

/// A vehicle reduced to what the equations of motion need.
#[derive(Debug, Clone)]
pub struct SimVehicle {
    pub id: String,
    /// kg, in the state being simulated.
    pub mass: f64,
    /// kg, the part of the mass carried on the springs.
    pub sprung_mass: f64,
    /// Yaw inertia about the centre of gravity, kg m^2.
    pub yaw_inertia: f64,
    /// Roll inertia of the sprung mass about the roll axis, kg m^2.
    pub roll_inertia: f64,
    /// Metres, from the centre of gravity forward to the front axle.
    pub a: f64,
    /// Metres, from the centre of gravity back to the rear axle.
    pub b: f64,
    pub wheelbase: f64,
    pub front_track: f64,
    pub rear_track: f64,
    /// Metres above the ground.
    pub cg_height: f64,
    /// Metres above the ground, the axis the body rolls about.
    pub roll_axis_height: f64,
    /// Roll stiffness in newton metres per radian, front and rear.
    pub roll_stiffness_front: f64,
    pub roll_stiffness_rear: f64,
    /// Roll damping, newton metre seconds per radian.
    pub roll_damping: f64,
    /// The front tyre. Kept separate from the rear because a different tyre at each end is the
    /// most direct way to change the balance of a car, and a model with one tyre cannot see it.
    pub tyre: Tyre,
    pub rear_tyre: Tyre,
    /// Steering ratio: hand wheel angle over road wheel angle.
    pub steering_ratio: f64,
    /// Drive torque at the wheels, newton metres, as a function of road speed.
    pub drive_torque: Vec<(f64, f64)>,
    /// Which axle drives: -1 front, 1 rear, 0 all.
    pub driven_axle: i8,
    /// Frontal area times drag coefficient, m^2.
    pub cda: f64,
    /// Share of the braking force that goes to the front axle.
    pub brake_bias_front: f64,
    /// Every number above, with where it came from.
    pub inputs: Vec<Input>,
    /// Places where the model contradicts itself, found while reading it.
    pub inconsistencies: Vec<String>,
}

impl SimVehicle {
    /// Build a simulation vehicle from a resolved model.
    ///
    /// `mass` and `cg` come from the caller because computing them needs a geometry kernel, and
    /// this crate deliberately does not depend on one.
    pub fn from_model(
        asm: &ResolvedAssembly,
        tier0: &wmds_analytics::Tier0,
        mass: f64,
        cg: [f64; 3],
    ) -> SimVehicle {
        let mut inputs = Vec::new();
        macro_rules! note {
            ($name:expr, $value:expr, $unit:expr, $source:expr $(,)?) => {{
                let v = $value;
                inputs.push(Input {
                    name: $name,
                    value: v,
                    unit: $unit,
                    source: $source,
                });
                v
            }};
        }

        let wheelbase = match tier0.wheelbase {
            Some(w) => note!("wheelbase", w, "m", Source::Model),
            None => note!(
                "wheelbase",
                2.4,
                "m",
                Source::Assumed("no two axles found in the model"),
            ),
        };
        let front_x = tier0.front_axle().map(|a| a.x);
        let rear_x = tier0.rear_axle().map(|a| a.x);
        // Vehicle x runs rearward, so the front axle is at a smaller x than the centre of gravity.
        let a = match front_x {
            Some(x) => note!("cg to front axle", (cg[0] - x).abs(), "m", Source::Model),
            None => note!("cg to front axle", wheelbase * 0.5, "m", Source::Assumed("no front axle")),
        };
        let b = match rear_x {
            Some(x) => note!("cg to rear axle", (x - cg[0]).abs(), "m", Source::Model),
            None => note!("cg to rear axle", wheelbase * 0.5, "m", Source::Assumed("no rear axle")),
        };
        let front_track = match tier0.front_axle().map(|a| a.track) {
            Some(t) => note!("front track", t, "m", Source::Model),
            None => note!("front track", 1.3, "m", Source::Assumed("no front axle")),
        };
        let rear_track = match tier0.rear_axle().map(|a| a.track) {
            Some(t) => note!("rear track", t, "m", Source::Model),
            None => note!("rear track", 1.3, "m", Source::Assumed("no rear axle")),
        };
        let cg_height = match tier0.cg_height {
            Some(h) => note!("centre of gravity height", h, "m", Source::Model),
            None => note!("centre of gravity height", 0.55, "m", Source::Assumed("no wheels")),
        };
        note!("mass", mass, "kg", Source::Model);

        // Unsprung mass: everything that moves with the wheels. Taken from the model by
        // category, which is why parts declare one.
        let unsprung: f64 = asm
            .instances
            .iter()
            .filter(|i| {
                let c = i.source_id.split('/').next().unwrap_or("");
                matches!(c, "wheels" | "braking") || i.source_id.contains("upright")
            })
            .filter_map(|i| match &i.primitive.massprops {
                wmds_model::ResolvedMassProps::Declared { mass, .. } => Some(mass.value),
                _ => None,
            })
            .sum();
        let unsprung = if unsprung > 0.0 {
            note!("unsprung mass", unsprung, "kg", Source::Model)
        } else {
            note!(
                "unsprung mass",
                mass * 0.12,
                "kg",
                Source::Assumed("no declared masses on wheels, brakes or uprights"),
            )
        };
        let sprung_mass = (mass - unsprung).max(mass * 0.5);

        // Yaw inertia. The usual estimate for a passenger car is mass times the product of the
        // two axle distances, which is exact for a two-mass car and close for a real one.
        let yaw_inertia = note!(
            "yaw inertia",
            mass * a * b,
            "kg m2",
            Source::Assumed("estimated as mass times the axle distances; no inertia tensor yet"),
        );
        let roll_axis_height = note!(
            "roll axis height",
            0.10,
            "m",
            Source::Assumed("no suspension kinematics; a low roll axis is typical"),
        );
        let roll_arm = (cg_height - roll_axis_height).max(0.05);
        let roll_inertia = note!(
            "roll inertia",
            sprung_mass * roll_arm * roll_arm * 1.4,
            "kg m2",
            Source::Assumed("estimated from the sprung mass and the roll arm"),
        );

        // Roll stiffness, from the springs and anti-roll bars in the vehicle if it has any.
        let measured = roll_stiffness_from(asm, cg[0], front_track, rear_track);
        let (roll_stiffness_front, roll_stiffness_rear) = match measured {
            Some((f, r, wf, wr)) => {
                // Ride frequency is worth reporting on its own: it is how a suspension is
                // usually specified and it says immediately whether the springs are sensible.
                let corner_mass_f = sprung_mass * (b / wheelbase) / 2.0;
                let corner_mass_r = sprung_mass * (a / wheelbase) / 2.0;
                note!(
                    "front ride frequency",
                    (wf / corner_mass_f).sqrt() / (2.0 * std::f64::consts::PI),
                    "Hz",
                    Source::Model,
                );
                note!(
                    "rear ride frequency",
                    (wr / corner_mass_r).sqrt() / (2.0 * std::f64::consts::PI),
                    "Hz",
                    Source::Model,
                );
                (
                    note!("front roll stiffness", f, "Nm/rad", Source::Model),
                    note!("rear roll stiffness", r, "Nm/rad", Source::Model),
                )
            }
            None => {
                let total_roll = {
                    let k_wheel =
                        sprung_mass * (2.0 * std::f64::consts::PI * 1.35).powi(2) / 2.0;
                    let t = (front_track + rear_track) / 2.0;
                    k_wheel * t * t / 2.0
                };
                (
                    note!(
                        "front roll stiffness",
                        total_roll * 0.55,
                        "Nm/rad",
                        Source::Assumed("the vehicle has no springs; from a 1.35 Hz ride frequency"),
                    ),
                    note!(
                        "rear roll stiffness",
                        total_roll * 0.45,
                        "Nm/rad",
                        Source::Assumed("the vehicle has no springs; from a 1.35 Hz ride frequency"),
                    ),
                )
            }
        };
        let roll_damping = note!(
            "roll damping",
            2.0 * 0.35
                * ((roll_stiffness_front + roll_stiffness_rear) * roll_inertia).sqrt(),
            "Nms/rad",
            Source::Assumed("35 percent of critical, typical for a road car"),
        );

        // The tyres, one per axle.
        let (tyre, tyre_source) = tyre_from(asm, cg[0], true);
        let (rear_tyre, _) = tyre_from(asm, cg[0], false);
        if (rear_tyre.cornering_c1 - tyre.cornering_c1).abs() > 1e-9 {
            inputs.push(Input {
                name: "rear tyre cornering stiffness",
                value: rear_tyre.cornering_c1 / tyre.cornering_c1,
                unit: "x front",
                source: Source::Model,
            });
        }
        inputs.push(Input {
            name: "tyre peak friction",
            value: tyre.peak_friction_y,
            unit: "",
            source: tyre_source,
        });

        let steering_ratio = steering_ratio_from(asm).unwrap_or_else(|| 16.0);
        note!(
            "steering ratio",
            steering_ratio,
            "",
            Source::Assumed("rack ratio and arm length not yet combined; 16 to 1 assumed"),
        );

        let (drive_torque, driven_axle, drive_source) = drive_from(asm, tyre.rolling_radius);

        // A torque curve and a declared peak power have to agree. When they do not, the
        // acceleration figures come from the curve and the specification sheet is wrong, which
        // is worth saying out loud rather than quietly believing one of them.
        let mut inconsistencies = Vec::new();
        if let Some(motor) = asm
            .instances
            .iter()
            .find(|i| i.primitive.behaviour.as_ref().is_some_and(|b| b.kind == "e-motor"))
            && let Some(declared) = motor
                .primitive
                .params
                .get("peak_power")
                .and_then(|v| v.as_quantity())
                .map(|q| q.value)
        {
            let implied = drive_torque
                .iter()
                .map(|(speed, torque)| torque / tyre.rolling_radius * speed)
                .fold(0.0_f64, f64::max);
            if implied > declared * 1.15 {
                inconsistencies.push(format!(
                    "{} declares {:.0} kW but its torque curve implies {:.0} kW at the wheels; the acceleration figures follow the curve.",
                    motor.source_id,
                    declared / 1e3,
                    implied / 1e3
                ));
            } else if implied < declared * 0.85 && implied > 0.0 {
                inconsistencies.push(format!(
                    "{} declares {:.0} kW but its torque curve only reaches {:.0} kW at the wheels.",
                    motor.source_id,
                    declared / 1e3,
                    implied / 1e3
                ));
            }
        }
        inputs.push(Input {
            name: "peak wheel torque",
            value: drive_torque
                .iter()
                .map(|(_, t)| *t)
                .fold(0.0_f64, f64::max),
            unit: "Nm",
            source: drive_source,
        });

        let cda = note!(
            "drag area",
            0.68,
            "m2",
            Source::Assumed("no body in the model; typical of a small hatchback"),
        );
        // Brake bias set to match the static weight split shifted forward for load transfer.
        let brake_bias_front = note!(
            "front brake bias",
            (b / wheelbase + 0.15).clamp(0.5, 0.85),
            "",
            Source::Assumed("set from the static weight split plus a typical forward shift"),
        );

        SimVehicle {
            id: asm.id.clone(),
            mass,
            sprung_mass,
            yaw_inertia,
            roll_inertia,
            a,
            b,
            wheelbase,
            front_track,
            rear_track,
            cg_height,
            roll_axis_height,
            roll_stiffness_front,
            roll_stiffness_rear,
            roll_damping,
            tyre,
            rear_tyre,
            steering_ratio,
            drive_torque,
            driven_axle,
            cda,
            brake_bias_front,
            inputs,
            inconsistencies,
        }
    }

    /// Static vertical load on one wheel of each axle, in newtons.
    pub fn static_loads(&self) -> (f64, f64) {
        let g = 9.81;
        let front = self.mass * g * self.b / self.wheelbase / 2.0;
        let rear = self.mass * g * self.a / self.wheelbase / 2.0;
        (front, rear)
    }

    /// Everything that had to be assumed rather than read from the model.
    pub fn assumptions(&self) -> Vec<String> {
        self.inputs
            .iter()
            .filter_map(|i| match &i.source {
                Source::Assumed(why) => Some(format!(
                    "{} taken as {:.3} {} because {why}",
                    i.name, i.value, i.unit
                )),
                Source::Model => None,
            })
            .collect()
    }
}

/// The tyre fitted to one axle.
fn tyre_from(asm: &ResolvedAssembly, cg_x: f64, front: bool) -> (Tyre, Source) {
    for inst in &asm.instances {
        let Some(b) = &inst.primitive.behaviour else {
            continue;
        };
        if b.kind != "tyre" {
            continue;
        }
        if (inst.placement.translation[0] < cg_x) != front {
            continue;
        }
        let d = Tyre::default();
        let radius = b.num("unloaded_radius").unwrap_or(d.radius);
        let t = Tyre {
            radius,
            rolling_radius: radius * b.num("rolling_radius_factor").unwrap_or(0.97),
            nominal_load: b.num("nominal_load").unwrap_or(d.nominal_load),
            peak_friction_y: b.num("peak_friction_y").unwrap_or(d.peak_friction_y),
            peak_friction_x: b.num("peak_friction_x").unwrap_or(d.peak_friction_x),
            load_sensitivity: b.num("load_sensitivity").unwrap_or(d.load_sensitivity),
            cornering_c1: b.num("cornering_c1").unwrap_or(d.cornering_c1),
            cornering_c2: b.num("cornering_c2").unwrap_or(d.cornering_c2),
            slip_stiffness: b.num("slip_stiffness").unwrap_or(d.slip_stiffness),
            shape_y: b.num("shape_y").unwrap_or(d.shape_y),
            shape_x: b.num("shape_x").unwrap_or(d.shape_x),
            curvature_y: b.num("curvature_y").unwrap_or(d.curvature_y),
            curvature_x: b.num("curvature_x").unwrap_or(d.curvature_x),
            relaxation_length: b.num("relaxation_length").unwrap_or(d.relaxation_length),
            rolling_resistance: b.num("rolling_resistance").unwrap_or(d.rolling_resistance),
        };
        return (t, Source::Model);
    }
    (
        Tyre::default(),
        Source::Assumed("the vehicle has no tyre with a behaviour model"),
    )
}

fn steering_ratio_from(asm: &ResolvedAssembly) -> Option<f64> {
    // The rack travel per turn and the steering arm length together give the ratio. The arm
    // length is on the upright; the travel is on the rack.
    let rack = asm
        .instances
        .iter()
        .find(|i| i.source_id.contains("steering/rack"))?;
    let ratio_mm = rack.primitive.params.get("ratio")?.as_quantity()?.value;
    let arm = asm
        .instances
        .iter()
        .find(|i| i.source_id.contains("upright"))
        .and_then(|i| i.primitive.params.get("steer_arm"))
        .and_then(|v| v.as_quantity())
        .map(|q| q.value)?;
    // One turn of the wheel moves the rack by `ratio_mm`, which swings the arm through
    // approximately travel over arm length radians.
    let road_wheel_per_turn = ratio_mm / arm;
    Some(2.0 * std::f64::consts::PI / road_wheel_per_turn)
}

fn drive_from(asm: &ResolvedAssembly, rolling_radius: f64) -> (Vec<(f64, f64)>, i8, Source) {
    for inst in &asm.instances {
        let Some(b) = &inst.primitive.behaviour else {
            continue;
        };
        if b.kind != "e-motor" {
            continue;
        }
        let final_drive = b.num("final_drive").unwrap_or(9.0);
        let rpm = tuple(b, "torque_curve_rpm");
        let nm = tuple(b, "torque_curve_nm");
        if rpm.len() != nm.len() || rpm.is_empty() {
            continue;
        }
        // Motor speed to road speed, through the reduction and the rolling radius.
        let curve: Vec<(f64, f64)> = rpm
            .iter()
            .zip(&nm)
            .map(|(r, t)| {
                let wheel_rad_s = r * 2.0 * std::f64::consts::PI / 60.0 / final_drive;
                (wheel_rad_s * rolling_radius, t * final_drive)
            })
            .collect();
        let axle = match b.text("drives") {
            Some("front") => -1,
            Some("all") => 0,
            _ => 1,
        };
        return (curve, axle, Source::Model);
    }
    (
        vec![(0.0, 0.0)],
        1,
        Source::Assumed("the vehicle has no motor with a behaviour model"),
    )
}

fn tuple(b: &wmds_model::ResolvedBehaviour, name: &str) -> Vec<f64> {
    match b.props.get(name) {
        Some(wmds_expr::Value::Tuple(items)) => items
            .iter()
            .filter_map(|v| v.as_quantity().map(|q| q.value))
            .collect(),
        Some(v) => v.as_quantity().map(|q| vec![q.value]).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Roll stiffness at each axle, taken from the springs and anti-roll bars in the model.
///
/// This is the number that used to be assumed. It is worth reading properly because it decides
/// how much load moves across each axle, and therefore the balance of the car: stiffening the
/// front bar is how a rear-heavy design is pushed back toward understeer.
///
///   wheel rate  = spring rate x motion ratio squared
///   roll rate   = wheel rate x track squared / 2
///
/// The motion ratio comes from where the spring sits along the suspension arm, which the model
/// already knows, so moving the spring outboard in the editor stiffens the car here.
fn roll_stiffness_from(
    asm: &ResolvedAssembly,
    cg_x: f64,
    front_track: f64,
    rear_track: f64,
) -> Option<(f64, f64, f64, f64)> {
    // Motion ratio per corner, from the lower arm in each sub-assembly.
    let mut ratio_of: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for i in &asm.instances {
        let Some((unit, _)) = i.id.split_once('.') else {
            continue;
        };
        if let Some(mr) = i
            .primitive
            .params
            .get("motion_ratio")
            .and_then(|v| v.as_quantity())
            .map(|q| q.value)
            && mr > 0.0
        {
            ratio_of.insert(unit.to_string(), mr);
        }
    }

    // Wheel rate at each corner, from its spring.
    let mut front_wheel_rate = 0.0;
    let mut rear_wheel_rate = 0.0;
    let mut front_count = 0;
    let mut rear_count = 0;
    for i in &asm.instances {
        let Some(b) = &i.primitive.behaviour else {
            continue;
        };
        if b.kind != "spring" {
            continue;
        }
        let Some(rate) = b.num("rate") else { continue };
        let unit = i.id.split('.').next().unwrap_or("");
        // Without a motion ratio the spring is acting directly on the wheel, which is what a
        // strut does and is the right fallback.
        let mr = ratio_of.get(unit).copied().unwrap_or(1.0);
        let wheel_rate = rate * mr * mr;
        if i.placement.translation[0] < cg_x {
            front_wheel_rate += wheel_rate;
            front_count += 1;
        } else {
            rear_wheel_rate += wheel_rate;
            rear_count += 1;
        }
    }
    if front_count == 0 || rear_count == 0 {
        return None;
    }
    front_wheel_rate /= front_count as f64;
    rear_wheel_rate /= rear_count as f64;

    // Anti-roll bars, added to the axle they sit on. Their rate is quoted at the drop links,
    // which hang from the same point on the arm as the spring, so the motion ratio is the same.
    let mut front_bar = 0.0;
    let mut rear_bar = 0.0;
    for i in &asm.instances {
        let Some(b) = &i.primitive.behaviour else {
            continue;
        };
        if b.kind != "anti-roll-bar" {
            continue;
        }
        let Some(rate) = b.num("rate") else { continue };
        let front = i.placement.translation[0] < cg_x;
        // Use the motion ratio of a corner on the same axle.
        let mr = ratio_of.values().copied().next().unwrap_or(1.0);
        let at_wheel = rate * mr * mr;
        if front {
            front_bar += at_wheel;
        } else {
            rear_bar += at_wheel;
        }
    }

    // Roll stiffness. A pair of springs at half a track either side of the roll axis gives
    // k t squared over two. An anti-roll bar works only in roll, so it adds on top.
    let f = (front_wheel_rate + front_bar) * front_track * front_track / 2.0;
    let r = (rear_wheel_rate + rear_bar) * rear_track * rear_track / 2.0;
    Some((f, r, front_wheel_rate, rear_wheel_rate))
}
