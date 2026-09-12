//! The equations of motion, and the integrator that advances them.
//!
//! A four wheel model in the horizontal plane, with roll. Each wheel gets its own vertical load
//! from static weight plus longitudinal and lateral transfer, its own slip angle from the
//! vehicle's motion and the steer angle, and its own tyre forces. That is the least that can be
//! called realistic: a two wheel bicycle model cannot show what load transfer does to balance,
//! and load transfer is most of what suspension tuning is about.
//!
//! Slip angles are passed through a first order lag set by the tyre's relaxation length, so the
//! response to a sudden steering input takes time to build, as it does on the road.

use crate::vehicle::SimVehicle;

/// The vehicle's state at one instant.
#[derive(Debug, Clone, Copy, Default)]
pub struct State {
    /// Forward speed, m/s.
    pub u: f64,
    /// Lateral speed, m/s, positive to the left.
    pub v: f64,
    /// Yaw rate, rad/s, positive nose left.
    pub r: f64,
    /// Roll angle, rad, positive leaning right in a left turn.
    pub roll: f64,
    pub roll_rate: f64,
    /// Lagged slip angle at each wheel: front left, front right, rear left, rear right.
    pub slip: [f64; 4],
    /// Position and heading in the world, for path following.
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    /// Distance travelled, m.
    pub distance: f64,
}

/// What the driver is asking for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Controls {
    /// Road wheel steer angle, radians. Positive turns left.
    pub steer: f64,
    /// Drive demand, 0 to 1.
    pub throttle: f64,
    /// Brake demand, 0 to 1.
    pub brake: f64,
}

/// Everything worth looking at after a step, beyond the state itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct Outputs {
    pub lateral_accel: f64,
    pub longitudinal_accel: f64,
    /// Vertical load at each wheel, newtons.
    pub loads: [f64; 4],
    /// Lateral force at each wheel, newtons.
    pub fy: [f64; 4],
    pub fx: [f64; 4],
    /// True when any wheel has lifted off the road.
    pub wheel_lift: bool,
    /// Body slip angle, radians.
    pub body_slip: f64,
}

const FL: usize = 0;
const FR: usize = 1;
const RL: usize = 2;
const RR: usize = 3;

/// Derivatives of the state, and the forces that produced them.
pub fn derivatives(veh: &SimVehicle, s: &State, c: &Controls) -> (State, Outputs) {
    let g = 9.81;
    let u = s.u.max(0.1);
    // Front wheels are 0 and 1, rear are 2 and 3.
    let tyre_of = |i: usize| if i < 2 { &veh.tyre } else { &veh.rear_tyre };

    // Wheel positions relative to the centre of gravity.
    let ax = [veh.a, veh.a, -veh.b, -veh.b];
    let ay = [
        veh.front_track / 2.0,
        -veh.front_track / 2.0,
        veh.rear_track / 2.0,
        -veh.rear_track / 2.0,
    ];
    let steer = [c.steer, c.steer, 0.0, 0.0];

    // Vertical loads. Longitudinal transfer follows the acceleration of the previous instant,
    // which is why it is estimated here from the forces rather than solved simultaneously: the
    // error is small at the step size used and the alternative is an implicit solve every step.
    let (fz_front_static, fz_rear_static) = veh.static_loads();

    // Lateral transfer is split by roll stiffness, which is what makes the balance of a car
    // depend on its anti-roll bars.
    let total_roll_k = veh.roll_stiffness_front + veh.roll_stiffness_rear;
    let roll_arm = (veh.cg_height - veh.roll_axis_height).max(0.02);

    // First pass with no load transfer to get an acceleration estimate.
    let mut loads = [fz_front_static, fz_front_static, fz_rear_static, fz_rear_static];
    let mut fy = [0.0; 4];
    let mut fx = [0.0; 4];
    let mut ay_accel = 0.0;
    let mut ax_accel = 0.0;

    for _ in 0..3 {
        // Slip angles at each wheel from the rigid body motion.
        let mut target_slip = [0.0; 4];
        for i in 0..4 {
            let vy = s.v + s.r * ax[i];
            let vx = u - s.r * ay[i];
            target_slip[i] = (vy / vx.max(0.1)).atan() - steer[i];
        }

        // Drive and brake.
        let drive_force = drive_at(veh, u) * c.throttle;
        let brake_force = max_brake(veh) * c.brake;
        for i in 0..4 {
            let front = i < 2;
            let driven = match veh.driven_axle {
                -1 => front,
                1 => !front,
                _ => true,
            };
            let share = if veh.driven_axle == 0 { 0.25 } else { 0.5 };
            let d = if driven { drive_force * share } else { 0.0 };
            let bias = if front {
                veh.brake_bias_front
            } else {
                1.0 - veh.brake_bias_front
            };
            let b = brake_force * bias / 2.0;
            let rolling = tyre_of(i).rolling_resistance * loads[i];
            fx[i] = d - b - rolling;
        }

        // Tyre forces, with each wheel's own load.
        for i in 0..4 {
            let ty = tyre_of(i);
            let fz = loads[i].max(0.0);
            // Longitudinal force is demanded rather than solved from a slip ratio: the wheel
            // rotational states are not modelled, so the demand is capped by the tyre instead.
            let max_x = ty.mu_x(fz).max(0.0) * fz;
            let fx_d = fx[i].clamp(-max_x, max_x);
            let fy_pure = ty.lateral(s.slip[i], fz);
            let max_y = ty.mu_y(fz).max(0.0) * fz;
            let used = if max_x > 0.0 { (fx_d / max_x).powi(2) } else { 0.0 };
            let left = (1.0 - used).max(0.0).sqrt();
            fy[i] = if max_y > 0.0 {
                fy_pure.clamp(-max_y * left, max_y * left)
            } else {
                0.0
            };
            fx[i] = fx_d;
        }
        let _ = target_slip;

        // Sum forces in body axes, accounting for the steered front wheels.
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        for i in 0..4 {
            let (cs, sn) = (steer[i].cos(), steer[i].sin());
            sum_x += fx[i] * cs - fy[i] * sn;
            sum_y += fx[i] * sn + fy[i] * cs;
        }
        // Aerodynamic drag.
        sum_x -= 0.5 * 1.225 * veh.cda * u * u;

        ax_accel = sum_x / veh.mass;
        ay_accel = sum_y / veh.mass;

        // Now redo the loads with those accelerations.
        let long_transfer = veh.mass * ax_accel * veh.cg_height / veh.wheelbase;
        let lat_total = veh.sprung_mass * ay_accel * roll_arm;
        let lat_front = if total_roll_k > 0.0 {
            lat_total * veh.roll_stiffness_front / total_roll_k
        } else {
            lat_total * 0.5
        };
        let lat_rear = lat_total - lat_front;
        // The unsprung mass transfers directly through the tyres, split by axle.
        let unsprung = (veh.mass - veh.sprung_mass) * ay_accel * veh.tyre.radius;
        let f_lat = (lat_front + unsprung * 0.5) / veh.front_track;
        let r_lat = (lat_rear + unsprung * 0.5) / veh.rear_track;

        loads[FL] = (fz_front_static - long_transfer / 2.0 - f_lat).max(0.0);
        loads[FR] = (fz_front_static - long_transfer / 2.0 + f_lat).max(0.0);
        loads[RL] = (fz_rear_static + long_transfer / 2.0 - r_lat).max(0.0);
        loads[RR] = (fz_rear_static + long_transfer / 2.0 + r_lat).max(0.0);
    }

    // Yaw moment.
    let mut mz = 0.0;
    for i in 0..4 {
        let (cs, sn) = (steer[i].cos(), steer[i].sin());
        let fxg = fx[i] * cs - fy[i] * sn;
        let fyg = fx[i] * sn + fy[i] * cs;
        mz += fyg * ax[i] - fxg * ay[i];
    }

    // Roll.
    let roll_moment = veh.sprung_mass * ay_accel * roll_arm
        - (veh.roll_stiffness_front + veh.roll_stiffness_rear) * s.roll
        - veh.roll_damping * s.roll_rate;

    // Slip angle lag: a tyre needs to roll about half a metre to build a new slip angle.
    let mut slip_dot = [0.0; 4];
    for i in 0..4 {
        let vy = s.v + s.r * ax[i];
        let vx = u - s.r * ay[i];
        let target = (vy / vx.max(0.1)).atan() - steer[i];
        let rate = u / tyre_of(i).relaxation_length.max(0.05);
        slip_dot[i] = rate * (target - s.slip[i]);
    }

    let d = State {
        u: ax_accel + s.v * s.r,
        v: ay_accel - s.u * s.r,
        r: mz / veh.yaw_inertia,
        roll: s.roll_rate,
        roll_rate: roll_moment / veh.roll_inertia.max(1.0),
        slip: slip_dot,
        x: s.u * s.heading.cos() - s.v * s.heading.sin(),
        y: s.u * s.heading.sin() + s.v * s.heading.cos(),
        heading: s.r,
        distance: (s.u * s.u + s.v * s.v).sqrt(),
    };
    let out = Outputs {
        lateral_accel: ay_accel / g,
        longitudinal_accel: ax_accel / g,
        loads,
        fy,
        fx,
        wheel_lift: loads.iter().any(|l| *l <= 1.0),
        body_slip: (s.v / u).atan(),
    };
    (d, out)
}

/// Drive force available at the wheels at this road speed, in newtons.
fn drive_at(veh: &SimVehicle, speed: f64) -> f64 {
    let curve = &veh.drive_torque;
    if curve.is_empty() {
        return 0.0;
    }
    let torque = if speed <= curve[0].0 {
        curve[0].1
    } else if speed >= curve[curve.len() - 1].0 {
        // Past the top of the curve the motor is done; do not extrapolate it upward.
        0.0
    } else {
        let mut t = curve[curve.len() - 1].1;
        for w in curve.windows(2) {
            if speed >= w[0].0 && speed <= w[1].0 {
                let f = (speed - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
                t = w[0].1 + f * (w[1].1 - w[0].1);
                break;
            }
        }
        t
    };
    torque / veh.tyre.rolling_radius
}

/// The most braking force the vehicle can ask for, before the tyres get a say.
fn max_brake(veh: &SimVehicle) -> f64 {
    // Enough to lock every wheel on a high friction surface, so the tyre model is what limits
    // braking rather than an arbitrary ceiling.
    veh.mass * 9.81 * 1.4
}

/// Advance one step with classical Runge-Kutta.
pub fn step(veh: &SimVehicle, s: &State, c: &Controls, dt: f64) -> (State, Outputs) {
    let (k1, out) = derivatives(veh, s, c);
    let s2 = add(s, &k1, dt * 0.5);
    let (k2, _) = derivatives(veh, &s2, c);
    let s3 = add(s, &k2, dt * 0.5);
    let (k3, _) = derivatives(veh, &s3, c);
    let s4 = add(s, &k3, dt);
    let (k4, _) = derivatives(veh, &s4, c);

    let mut out_state = *s;
    let w = |a: f64, b: f64, c: f64, d: f64| (a + 2.0 * b + 2.0 * c + d) / 6.0;
    out_state.u = (s.u + dt * w(k1.u, k2.u, k3.u, k4.u)).max(0.0);
    out_state.v = s.v + dt * w(k1.v, k2.v, k3.v, k4.v);
    out_state.r = s.r + dt * w(k1.r, k2.r, k3.r, k4.r);
    out_state.roll = s.roll + dt * w(k1.roll, k2.roll, k3.roll, k4.roll);
    out_state.roll_rate = s.roll_rate + dt * w(k1.roll_rate, k2.roll_rate, k3.roll_rate, k4.roll_rate);
    for i in 0..4 {
        out_state.slip[i] =
            s.slip[i] + dt * w(k1.slip[i], k2.slip[i], k3.slip[i], k4.slip[i]);
    }
    out_state.x = s.x + dt * w(k1.x, k2.x, k3.x, k4.x);
    out_state.y = s.y + dt * w(k1.y, k2.y, k3.y, k4.y);
    out_state.heading = s.heading + dt * w(k1.heading, k2.heading, k3.heading, k4.heading);
    out_state.distance = s.distance + dt * w(k1.distance, k2.distance, k3.distance, k4.distance);
    (out_state, out)
}

fn add(s: &State, d: &State, h: f64) -> State {
    let mut out = *s;
    out.u = (s.u + d.u * h).max(0.0);
    out.v = s.v + d.v * h;
    out.r = s.r + d.r * h;
    out.roll = s.roll + d.roll * h;
    out.roll_rate = s.roll_rate + d.roll_rate * h;
    for i in 0..4 {
        out.slip[i] = s.slip[i] + d.slip[i] * h;
    }
    out.x = s.x + d.x * h;
    out.y = s.y + d.y * h;
    out.heading = s.heading + d.heading * h;
    out.distance = s.distance + d.distance * h;
    out
}
