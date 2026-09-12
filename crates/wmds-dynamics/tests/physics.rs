//! Does the simulator obey physics, and does it give numbers a car actually gives?
//!
//! A vehicle model is easy to write and hard to trust. These tests check the things that must be
//! true regardless of the vehicle: grip cannot exceed what the tyres have, a rear-heavy car with
//! equal tyres oversteers, stopping distance follows friction, and doubling the mass does not
//! change the cornering limit much. They are written against a vehicle built here rather than
//! the reference one, so a change to the library cannot quietly move the goalposts.

use wmds_dynamics::{SimVehicle, Source, Tyre};

/// A plain front-engined hatchback, made up here so the expected answers are known.
fn car() -> SimVehicle {
    let mass = 1200.0;
    let a = 1.05;
    let b = 1.55;
    let wheelbase = a + b;
    let track = 1.5;
    let tyre = Tyre::default();
    SimVehicle {
        id: "test/hatchback".into(),
        mass,
        sprung_mass: mass * 0.87,
        yaw_inertia: mass * a * b,
        roll_inertia: 380.0,
        a,
        b,
        wheelbase,
        front_track: track,
        rear_track: track,
        cg_height: 0.55,
        roll_axis_height: 0.10,
        roll_stiffness_front: 30000.0,
        roll_stiffness_rear: 22000.0,
        roll_damping: 4000.0,
        tyre,
        steering_ratio: 16.0,
        drive_torque: vec![(0.0, 1400.0), (15.0, 1400.0), (40.0, 700.0), (60.0, 300.0)],
        driven_axle: -1,
        cda: 0.68,
        brake_bias_front: 0.68,
        inputs: vec![],
        inconsistencies: vec![],
    }
}

#[test]
fn cornering_grip_cannot_exceed_what_the_tyres_have() {
    let v = car();
    let r = wmds_dynamics::steady_state(&v, 30.0);
    let mu = v.tyre.peak_friction_y;
    assert!(
        r.max_lateral_g <= mu * 1.05,
        "the car pulled {:.2} g on tyres with a peak friction of {mu:.2}",
        r.max_lateral_g
    );
    assert!(
        r.max_lateral_g > mu * 0.7,
        "only {:.2} g out of a possible {mu:.2}; the model is leaving most of the grip unused",
        r.max_lateral_g
    );
}

#[test]
fn a_front_heavy_car_understeers() {
    // 47 per cent of the mass on the front axle of a 2.6 m wheelbase, equal tyres all round.
    // With more load on the front tyres they run out first, which is understeer, and a road car
    // is required to behave this way.
    let v = car();
    let r = wmds_dynamics::steady_state(&v, 30.0);
    assert!(
        r.understeer_gradient > 0.0,
        "a car with its mass forward should understeer, gradient was {:.2} degrees per g",
        r.understeer_gradient
    );
    assert!(
        r.understeer_gradient < 12.0,
        "gradient of {:.2} degrees per g is more understeer than any production car",
        r.understeer_gradient
    );
}

#[test]
fn moving_the_mass_rearward_reduces_understeer() {
    // The single clearest check that balance comes out of the model rather than being assumed.
    let front_heavy = wmds_dynamics::steady_state(&car(), 30.0);
    let rear_heavy = {
        let mut v = car();
        std::mem::swap(&mut v.a, &mut v.b);
        v.yaw_inertia = v.mass * v.a * v.b;
        wmds_dynamics::steady_state(&v, 30.0)
    };
    assert!(
        rear_heavy.understeer_gradient < front_heavy.understeer_gradient,
        "moving the mass to the back should reduce understeer: {:.2} against {:.2} degrees per g",
        rear_heavy.understeer_gradient,
        front_heavy.understeer_gradient
    );
}

#[test]
fn a_higher_centre_of_gravity_costs_grip() {
    // Because load transfer takes more from the inside tyre than the outside one gains.
    let low = {
        let mut v = car();
        v.cg_height = 0.40;
        wmds_dynamics::steady_state(&v, 30.0).max_lateral_g
    };
    let high = {
        let mut v = car();
        v.cg_height = 0.75;
        wmds_dynamics::steady_state(&v, 30.0).max_lateral_g
    };
    assert!(
        high < low,
        "raising the centre of gravity should cost grip: {high:.3} g against {low:.3} g"
    );
}

#[test]
fn stopping_distance_matches_the_friction_available() {
    let v = car();
    let b = wmds_dynamics::braking(&v, 100.0);
    // v squared over twice the deceleration, with the deceleration the tyres can give.
    let speed = 100.0 / 3.6;
    let ideal = speed * speed / (2.0 * v.tyre.peak_friction_x * 9.81);
    assert!(
        b.distance > ideal * 0.9 && b.distance < ideal * 1.35,
        "stopped in {:.1} m; the friction available says about {ideal:.1} m",
        b.distance
    );
    assert!(
        b.peak_g <= v.tyre.peak_friction_x * 1.1,
        "decelerated at {:.2} g on tyres with {:.2} of friction",
        b.peak_g,
        v.tyre.peak_friction_x
    );
}

#[test]
fn halving_the_friction_roughly_doubles_the_stopping_distance() {
    let dry = wmds_dynamics::braking(&car(), 100.0).distance;
    let wet = {
        let mut v = car();
        v.tyre.peak_friction_x *= 0.5;
        v.tyre.peak_friction_y *= 0.5;
        wmds_dynamics::braking(&v, 100.0).distance
    };
    let ratio = wet / dry;
    assert!(
        (1.7..2.4).contains(&ratio),
        "halving the grip changed the stopping distance by {ratio:.2} times, not about two"
    );
}

#[test]
fn the_cornering_limit_barely_depends_on_mass() {
    // A heavier car has more grip and more to hold up, and the two nearly cancel. What is left
    // is the load sensitivity of the tyre, which costs the heavier car a little.
    let light = {
        let mut v = car();
        v.mass = 900.0;
        v.sprung_mass = 780.0;
        wmds_dynamics::steady_state(&v, 30.0).max_lateral_g
    };
    let heavy = {
        let mut v = car();
        v.mass = 1800.0;
        v.sprung_mass = 1560.0;
        wmds_dynamics::steady_state(&v, 30.0).max_lateral_g
    };
    let change = (heavy - light).abs() / light;
    assert!(
        change < 0.15,
        "doubling the mass changed the cornering limit by {:.0} percent, which is too much",
        change * 100.0
    );
    assert!(
        heavy < light,
        "the heavier car should corner slightly less hard, not more"
    );
}

#[test]
fn a_step_steer_response_is_as_quick_as_a_car() {
    // A passenger car reaches most of its steady yaw rate in a few tenths of a second. Much
    // slower and the model has too much inertia or not enough tyre; much quicker and it has no
    // dynamics in it at all.
    let v = car();
    let r = wmds_dynamics::step_steer(&v, 80.0 / 3.6, 2.0);
    assert!(
        r.steady_yaw_rate.abs() > 1e-3,
        "the car did not turn at all"
    );
    assert!(
        (0.05..0.6).contains(&r.response_time),
        "took {:.0} ms to reach nine tenths of {:.3} rad/s of yaw; a car takes one to four \
         tenths of a second",
        r.response_time * 1e3,
        r.steady_yaw_rate
    );
}

#[test]
fn the_steady_yaw_rate_is_close_to_what_the_geometry_says() {
    // At a gentle steer angle the car should turn at close to the rate a bicycle model gives,
    // because the tyres are nowhere near their limit and the geometry is what decides.
    let v = car();
    let speed = 60.0 / 3.6;
    let steer_deg = 1.0;
    let r = wmds_dynamics::step_steer(&v, speed, steer_deg);
    let ackermann = speed * steer_deg.to_radians() / v.wheelbase;
    let ratio = r.steady_yaw_rate.abs() / ackermann;
    assert!(
        (0.6..1.1).contains(&ratio),
        "yaw rate was {:.3} rad/s against {ackermann:.3} from the geometry alone, a ratio of \
         {ratio:.2}. A real car is a little below one because the tyres slip.",
        r.steady_yaw_rate
    );
}

#[test]
fn acceleration_is_limited_by_grip_or_by_power_and_says_which() {
    let v = car();
    let a = wmds_dynamics::acceleration(&v);
    let t = a.to_100.expect("a car with 1400 Nm at the wheels reaches 100 km/h");
    assert!(
        (3.0..20.0).contains(&t),
        "0 to 100 km/h in {t:.1} s is not a road car"
    );
    assert!(a.top_speed > 100.0);
}

#[test]
fn a_vehicle_with_nothing_in_it_still_says_what_it_assumed() {
    let v = car();
    // The test vehicle declares no inputs, so nothing is claimed.
    assert!(v.assumptions().is_empty());
    let listed = SimVehicle {
        inputs: vec![wmds_dynamics::Input {
            name: "roll stiffness",
            value: 1.0,
            unit: "Nm/rad",
            source: Source::Assumed("there are no springs in the library"),
        }],
        ..car()
    };
    let a = listed.assumptions();
    assert_eq!(a.len(), 1);
    assert!(a[0].contains("no springs"));
}
