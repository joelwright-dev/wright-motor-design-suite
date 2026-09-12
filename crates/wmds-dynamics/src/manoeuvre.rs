//! The tests a vehicle is put through.
//!
//! These are the standard ones, chosen because they are what a chassis engineer would ask for
//! and what the regulations lean on: steady state cornering for balance and grip, a step input
//! for how quickly the car responds, braking for stopping distance, acceleration for
//! performance, and a lane change for whether it is controllable when it is hurried.

use crate::solver::{Controls, State, step};
use crate::vehicle::SimVehicle;

const DT: f64 = 0.002;

/// What a steady state cornering sweep found.
#[derive(Debug, Clone)]
pub struct SteadyState {
    /// One row per speed: lateral acceleration in g, and the steer angle it needed, in degrees.
    pub points: Vec<(f64, f64)>,
    /// Degrees of steer per g, near the linear part. Positive means understeer.
    pub understeer_gradient: f64,
    /// The most lateral acceleration the vehicle reached, in g.
    pub max_lateral_g: f64,
    /// What ran out first.
    pub limit: String,
    /// Body roll at the limit, degrees.
    pub roll_at_limit: f64,
    /// True if a wheel came off the ground before the tyres let go.
    pub lifted_a_wheel: bool,
}

/// Drive a constant radius circle at rising speed and record what it takes.
///
/// The classic skidpad. The understeer gradient that falls out of it is the single number that
/// says most about how a car behaves, and it is a property of the whole vehicle rather than of
/// any one part, which is exactly the kind of thing this suite exists to compute.
pub fn steady_state(veh: &SimVehicle, radius: f64) -> SteadyState {
    let mut points = Vec::new();
    let mut max_g: f64 = 0.0;
    let mut limit = "the speed sweep reached its end without the tyres letting go".to_string();
    let mut roll_at_limit = 0.0;
    let mut lifted = false;

    // Start from the Ackermann angle and carry each solution forward as the guess for the next
    // speed. Continuation like this is what keeps the solve stable as the tyres approach their
    // limit, where the relationship between steer angle and yaw rate stops being linear.
    let mut guess = veh.wheelbase / radius;
    let mut speed = 5.0;
    while speed < 60.0 {
        let target_yaw = speed / radius;
        let Some((steer, out, state)) = solve_for_yaw(veh, speed, target_yaw, guess) else {
            limit = format!("the tyres let go at {max_g:.2} g, around {:.0} km/h", speed * 3.6);
            break;
        };
        guess = steer;
        let g = out.lateral_accel.abs();
        points.push((g, steer.to_degrees()));
        if g > max_g {
            max_g = g;
            roll_at_limit = state.roll.to_degrees().abs();
        }
        if out.wheel_lift {
            lifted = true;
            limit = format!("a wheel lifted off the road at {g:.2} g");
            break;
        }
        speed += 0.5;
    }

    // Understeer gradient from the low part of the curve, where the relationship is linear and
    // the number means what it is usually taken to mean.
    let gradient = {
        let low: Vec<&(f64, f64)> = points.iter().filter(|(g, _)| *g > 0.08 && *g < 0.4).collect();
        if low.len() >= 2 {
            let first = low[0];
            let last = low[low.len() - 1];
            (last.1 - first.1) / (last.0 - first.0).max(1e-6)
        } else {
            0.0
        }
    };

    SteadyState {
        points,
        understeer_gradient: gradient,
        max_lateral_g: max_g,
        limit,
        roll_at_limit,
        lifted_a_wheel: lifted,
    }
}

/// Run to equilibrium at a fixed speed and steer angle, and report the yaw rate reached.
fn equilibrium(veh: &SimVehicle, speed: f64, steer: f64) -> Option<(f64, crate::solver::Outputs, State)> {
    let mut s = State {
        u: speed,
        ..Default::default()
    };
    let c = Controls {
        steer,
        ..Default::default()
    };
    let mut out = Default::default();
    let mut last = 0.0;
    let mut t = 0.0;
    while t < 8.0 {
        s.u = speed;
        let (ns, o) = step(veh, &s, &c, DT);
        s = ns;
        out = o;
        t += DT;
        if !s.r.is_finite() || s.r.abs() > 10.0 {
            return None;
        }
        // Settled once the yaw rate stops changing. Checked every tenth of a second so noise in
        // one step cannot end it early.
        if t > 0.5 && (t / DT) as usize % 50 == 0 {
            if (s.r - last).abs() < 1e-6 {
                break;
            }
            last = s.r;
        }
    }
    Some((s.r, out, s))
}

/// Find the steer angle that holds a given yaw rate, by secant iteration.
///
/// A secant rather than a fixed gain, because near the limit a small change in steer angle
/// produces almost no change in yaw rate, and any fixed gain either crawls there or overshoots
/// into nonsense.
fn solve_for_yaw(
    veh: &SimVehicle,
    speed: f64,
    target: f64,
    guess: f64,
) -> Option<(f64, crate::solver::Outputs, State)> {
    let f = |steer: f64| equilibrium(veh, speed, steer).map(|(r, o, s)| (r - target, o, s));

    let mut x0 = guess;
    let (mut f0, mut out, mut st) = f(x0)?;
    if f0.abs() < 1e-5 {
        return Some((x0, out, st));
    }
    let mut x1 = guess * 1.1 + 1e-3;
    let (mut f1, o1, s1) = f(x1)?;
    out = o1;
    st = s1;

    for _ in 0..30 {
        if f1.abs() < 1e-5 {
            return Some((x1, out, st));
        }
        let denom = f1 - f0;
        if denom.abs() < 1e-12 {
            // The yaw rate has stopped responding to steer, which is what running out of front
            // grip looks like.
            return None;
        }
        let mut x2 = x1 - f1 * (x1 - x0) / denom;
        // A road wheel angle beyond about forty degrees is not a steering input, it is a
        // diverging solve.
        if !x2.is_finite() || x2.abs() > 0.7 {
            return None;
        }
        // Damp the step so the secant cannot leap past the solution near the limit.
        x2 = x1 + (x2 - x1).clamp(-0.05, 0.05);
        let (f2, o2, s2) = f(x2)?;
        x0 = x1;
        f0 = f1;
        x1 = x2;
        f1 = f2;
        out = o2;
        st = s2;
    }
    if f1.abs() < 1e-3 {
        Some((x1, out, st))
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct StepSteer {
    /// Seconds from the input to 90 percent of the final yaw rate.
    pub response_time: f64,
    /// How far the yaw rate overshoots its final value, as a fraction.
    pub overshoot: f64,
    pub steady_yaw_rate: f64,
    pub peak_lateral_g: f64,
    /// Seconds until the roll angle settles.
    pub roll_settling_time: f64,
}

/// Turn the wheel suddenly and watch how the car catches up.
///
/// Response time and overshoot are what make a car feel prompt or vague, and they come from the
/// yaw inertia, the tyres and the balance, none of which a static calculation can see.
pub fn step_steer(veh: &SimVehicle, speed: f64, steer_deg: f64) -> StepSteer {
    let mut s = State {
        u: speed,
        ..Default::default()
    };
    let c = Controls {
        steer: steer_deg.to_radians(),
        ..Default::default()
    };
    let mut history: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut t = 0.0;
    while t < 4.0 {
        s.u = speed;
        let (ns, o) = step(veh, &s, &c, DT);
        s = ns;
        t += DT;
        history.push((t, s.r, o.lateral_accel, s.roll));
    }
    let steady = history[history.len() - 200..]
        .iter()
        .map(|h| h.1)
        .sum::<f64>()
        / 200.0;
    let peak_r = history.iter().map(|h| h.1.abs()).fold(0.0, f64::max);
    let peak_g = history.iter().map(|h| h.2.abs()).fold(0.0, f64::max);
    let response_time = history
        .iter()
        .find(|h| h.1.abs() >= 0.9 * steady.abs())
        .map(|h| h.0)
        .unwrap_or(f64::NAN);
    let overshoot = if steady.abs() > 1e-6 {
        (peak_r / steady.abs() - 1.0).max(0.0)
    } else {
        0.0
    };
    let final_roll = history[history.len() - 1].3;
    let roll_settling = history
        .iter()
        .rev()
        .find(|h| (h.3 - final_roll).abs() > 0.05 * final_roll.abs().max(1e-6))
        .map(|h| h.0)
        .unwrap_or(0.0);

    StepSteer {
        response_time,
        overshoot,
        steady_yaw_rate: steady,
        peak_lateral_g: peak_g,
        roll_settling_time: roll_settling,
    }
}

#[derive(Debug, Clone)]
pub struct Braking {
    pub from_speed: f64,
    /// Metres.
    pub distance: f64,
    pub time: f64,
    /// Best deceleration reached, in g.
    pub peak_g: f64,
    /// Which axle locked first, if the bias is wrong.
    pub note: String,
}

/// Brake in a straight line from a speed until stopped.
pub fn braking(veh: &SimVehicle, from_kph: f64) -> Braking {
    let from = from_kph / 3.6;
    let mut s = State {
        u: from,
        ..Default::default()
    };
    let c = Controls {
        brake: 1.0,
        ..Default::default()
    };
    let mut t = 0.0;
    let mut peak = 0.0;
    let mut front_first = None;
    while s.u > 0.1 && t < 20.0 {
        let (ns, o) = step(veh, &s, &c, DT);
        s = ns;
        t += DT;
        peak = f64::max(peak, -o.longitudinal_accel);
        if front_first.is_none() {
            // The axle whose tyres saturate first decides whether it stops straight or spins.
            let front_use = (o.fx[0].abs() + o.fx[1].abs())
                / (veh.tyre.mu_x(o.loads[0]) * (o.loads[0] + o.loads[1])).max(1.0);
            let rear_use = (o.fx[2].abs() + o.fx[3].abs())
                / (veh.tyre.mu_x(o.loads[2]) * (o.loads[2] + o.loads[3])).max(1.0);
            if front_use > 0.98 && front_use > rear_use {
                front_first = Some(true);
            } else if rear_use > 0.98 && rear_use > front_use {
                front_first = Some(false);
            }
        }
    }
    let note = match front_first {
        Some(true) => "the front axle reaches its limit first, which is what keeps it straight"
            .to_string(),
        Some(false) => "the REAR axle reaches its limit first, which makes it unstable under \
                        braking. Move the bias forward."
            .to_string(),
        None => "neither axle saturated; the brakes are not the limit here".to_string(),
    };
    Braking {
        from_speed: from_kph,
        distance: s.distance,
        time: t,
        peak_g: peak,
        note,
    }
}

#[derive(Debug, Clone)]
pub struct Acceleration {
    /// Seconds to 100 km/h, when it gets there.
    pub to_100: Option<f64>,
    pub to_60: Option<f64>,
    /// Metres covered in the standing 400.
    pub quarter_mile_time: Option<f64>,
    pub top_speed: f64,
    /// True when the tyres, not the motor, set the launch.
    pub traction_limited: bool,
}

/// Accelerate from rest on full throttle.
pub fn acceleration(veh: &SimVehicle) -> Acceleration {
    let mut s = State {
        u: 0.5,
        ..Default::default()
    };
    let c = Controls {
        throttle: 1.0,
        ..Default::default()
    };
    let mut t = 0.0;
    let mut to_60 = None;
    let mut to_100 = None;
    let mut quarter = None;
    let mut traction_limited = false;
    let mut last_u = 0.0;
    while t < 60.0 {
        let (ns, o) = step(veh, &s, &c, DT);
        // If the demanded drive force was clipped by the tyres in the first moments, the launch
        // is traction limited rather than power limited.
        if t < 1.0 {
            let driven: f64 = if veh.driven_axle == 1 {
                o.fx[2] + o.fx[3]
            } else if veh.driven_axle == -1 {
                o.fx[0] + o.fx[1]
            } else {
                o.fx.iter().sum()
            };
            let cap: f64 = if veh.driven_axle == 1 {
                veh.tyre.mu_x(o.loads[2]) * (o.loads[2] + o.loads[3])
            } else if veh.driven_axle == -1 {
                veh.tyre.mu_x(o.loads[0]) * (o.loads[0] + o.loads[1])
            } else {
                o.loads.iter().map(|l| veh.tyre.mu_x(*l) * l).sum()
            };
            if cap > 0.0 && driven / cap > 0.97 {
                traction_limited = true;
            }
        }
        s = ns;
        t += DT;
        if to_60.is_none() && s.u >= 60.0 / 3.6 {
            to_60 = Some(t);
        }
        if to_100.is_none() && s.u >= 100.0 / 3.6 {
            to_100 = Some(t);
        }
        if quarter.is_none() && s.distance >= 402.3 {
            quarter = Some(t);
        }
        // Stop once it has effectively stopped accelerating.
        if t > 5.0 && (s.u - last_u).abs() < 1e-4 {
            break;
        }
        last_u = s.u;
    }
    Acceleration {
        to_100,
        to_60,
        quarter_mile_time: quarter,
        top_speed: s.u * 3.6,
        traction_limited,
    }
}

#[derive(Debug, Clone)]
pub struct LaneChange {
    pub entry_speed: f64,
    /// The worst distance from the intended path, in metres.
    pub max_path_error: f64,
    pub peak_lateral_g: f64,
    pub peak_body_slip: f64,
    /// Peak steering wheel angle the driver needed, degrees.
    pub peak_steering_wheel: f64,
    /// True when the car went where it was pointed.
    pub completed: bool,
    pub note: String,
}

/// A double lane change, driven by a simple preview driver.
///
/// The manoeuvre in ISO 3888, approximated: swerve out by three and a half metres over
/// twenty five, hold, and swerve back. What it is really testing is whether the car stays
/// controllable when it is asked to change direction twice in quick succession, which is where
/// a car with too little rear grip lets go.
pub fn lane_change(veh: &SimVehicle, speed_kph: f64) -> LaneChange {
    let speed = speed_kph / 3.6;
    let mut s = State {
        u: speed,
        ..Default::default()
    };
    let mut max_err: f64 = 0.0;
    let mut peak_g: f64 = 0.0;
    let mut peak_slip: f64 = 0.0;
    let mut peak_steer: f64 = 0.0;
    let mut t = 0.0;
    let preview = (speed * 0.6).max(6.0);

    while s.x < 130.0 && t < 20.0 {
        let target = lane_change_path(s.x + preview);
        let here = lane_change_path(s.x);
        // A proportional and derivative driver on the lateral error at the preview point.
        let error = target - (s.y + preview * s.heading.tan().clamp(-1.0, 1.0));
        let steer = (error / preview.powi(2) * veh.wheelbase * 2.0).clamp(-0.6, 0.6);
        let c = Controls {
            steer,
            ..Default::default()
        };
        s.u = speed;
        let (ns, o) = step(veh, &s, &c, DT);
        s = ns;
        t += DT;
        max_err = max_err.max((s.y - here).abs());
        peak_g = peak_g.max(o.lateral_accel.abs());
        peak_slip = peak_slip.max(o.body_slip.abs());
        peak_steer = peak_steer.max((steer * veh.steering_ratio).abs());
        if !s.y.is_finite() || s.y.abs() > 20.0 {
            break;
        }
    }
    let completed = max_err < 1.0 && s.x >= 120.0;
    let note = if !completed {
        format!(
            "the car did not hold the path at {speed_kph:.0} km/h; it was {max_err:.2} m off at \
             the worst point"
        )
    } else if peak_slip.to_degrees() > 8.0 {
        format!(
            "held the path, but with {:.1} degrees of body slip, which is a car that is \
             working hard",
            peak_slip.to_degrees()
        )
    } else {
        "held the path without drama".to_string()
    };

    LaneChange {
        entry_speed: speed_kph,
        max_path_error: max_err,
        peak_lateral_g: peak_g,
        peak_body_slip: peak_slip.to_degrees(),
        peak_steering_wheel: peak_steer.to_degrees(),
        completed,
        note,
    }
}

/// The lateral offset the double lane change asks for, at a distance along the course.
fn lane_change_path(x: f64) -> f64 {
    // Straight, out, hold, back, straight. Smoothed so the driver is not chasing a step.
    let smooth = |a: f64, b: f64, x: f64| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    3.5 * smooth(15.0, 40.0, x) - 3.5 * smooth(65.0, 90.0, x)
}
