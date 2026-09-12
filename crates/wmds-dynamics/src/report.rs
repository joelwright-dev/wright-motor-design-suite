//! Reporting what the simulation found.

use std::fmt::Write;

use crate::{Handling, Source};

pub fn write_text(h: &Handling) -> String {
    let mut s = String::new();
    let v = &h.vehicle;
    let _ = writeln!(s, "{}  driving dynamics", v.id);
    let _ = writeln!(
        s,
        "  {:.0} kg, wheelbase {:.0} mm, track {:.0}/{:.0} mm, cg {:.0} mm up, {:.0}/{:.0} split",
        v.mass,
        v.wheelbase * 1e3,
        v.front_track * 1e3,
        v.rear_track * 1e3,
        v.cg_height * 1e3,
        v.b / v.wheelbase * 100.0,
        v.a / v.wheelbase * 100.0
    );

    let _ = writeln!(s, "\nsteady state cornering, 30 m radius");
    let _ = writeln!(
        s,
        "  maximum          {:.2} g",
        h.skidpad.max_lateral_g
    );
    let _ = writeln!(
        s,
        "  understeer       {:+.2} degrees of steer per g",
        h.skidpad.understeer_gradient
    );
    let _ = writeln!(s, "  balance          {}", h.balance());
    if h.skidpad.understeer_gradient < 0.5 {
        let _ = writeln!(
            s,
            "  WARNING          a road car should understeer. This one does not: the rear tyres"
        );
        let _ = writeln!(
            s,
            "                   give up before the front ones. Wider rear tyres, more front roll"
        );
        let _ = writeln!(
            s,
            "                   stiffness, or mass moved forward are the usual answers."
        );
    }
    let _ = writeln!(
        s,
        "  body roll        {:.1} degrees at the limit",
        h.skidpad.roll_at_limit
    );
    let _ = writeln!(s, "  limited by       {}", h.skidpad.limit);
    if h.skidpad.lifted_a_wheel {
        let _ = writeln!(
            s,
            "  WARNING          a wheel lifted before the tyres let go, which is the start of a \
             rollover"
        );
    }

    let ride: Vec<&crate::Input> = v
        .inputs
        .iter()
        .filter(|i| i.name.contains("ride frequency"))
        .collect();
    if !ride.is_empty() {
        let _ = writeln!(s, "
springs and bars, read from the vehicle");
        for i in &ride {
            let _ = writeln!(
                s,
                "  {:<16} {:.2} Hz ride frequency",
                i.name.replace(" ride frequency", ""),
                i.value
            );
        }
        let total = v.roll_stiffness_front + v.roll_stiffness_rear;
        let _ = writeln!(
            s,
            "  roll stiffness   {:.0} front, {:.0} rear Nm per degree, a {:.0}/{:.0} split",
            v.roll_stiffness_front.to_radians(),
            v.roll_stiffness_rear.to_radians(),
            v.roll_stiffness_front / total * 100.0,
            v.roll_stiffness_rear / total * 100.0
        );
    }

    let _ = writeln!(s, "\nstep steer, 80 km/h, 2 degrees at the road wheel");
    let _ = writeln!(
        s,
        "  response         {:.0} ms to nine tenths of the final yaw rate",
        h.step.response_time * 1e3
    );
    let _ = writeln!(
        s,
        "  overshoot        {:.0} percent",
        h.step.overshoot * 100.0
    );
    let _ = writeln!(
        s,
        "  settles in       {:.2} s",
        h.step.roll_settling_time
    );

    let _ = writeln!(s, "\nbraking from {:.0} km/h", h.braking.from_speed);
    let _ = writeln!(s, "  distance         {:.1} m", h.braking.distance);
    let _ = writeln!(s, "  best             {:.2} g", h.braking.peak_g);
    let _ = writeln!(s, "  stability        {}", h.braking.note);

    let _ = writeln!(s, "\nacceleration");
    match h.acceleration.to_100 {
        Some(t) => {
            let _ = writeln!(s, "  0 to 100 km/h    {t:.1} s");
        }
        None => {
            let _ = writeln!(s, "  0 to 100 km/h    never reaches it");
        }
    }
    if let Some(t) = h.acceleration.to_60 {
        let _ = writeln!(s, "  0 to 60 km/h     {t:.1} s");
    }
    if let Some(t) = h.acceleration.quarter_mile_time {
        let _ = writeln!(s, "  standing 400 m   {t:.1} s");
    }
    let _ = writeln!(s, "  top speed        {:.0} km/h", h.acceleration.top_speed);
    if h.acceleration.traction_limited {
        let _ = writeln!(
            s,
            "  launch           limited by grip rather than by the motor"
        );
    }

    let _ = writeln!(
        s,
        "\ndouble lane change at {:.0} km/h",
        h.lane_change.entry_speed
    );
    let _ = writeln!(
        s,
        "  worst deviation  {:.2} m from the intended path",
        h.lane_change.max_path_error
    );
    let _ = writeln!(
        s,
        "  peak lateral     {:.2} g, body slip {:.1} degrees",
        h.lane_change.peak_lateral_g, h.lane_change.peak_body_slip
    );
    let _ = writeln!(
        s,
        "  steering wheel   {:.0} degrees at the worst point",
        h.lane_change.peak_steering_wheel
    );
    let _ = writeln!(s, "  verdict          {}", h.lane_change.note);

    if !v.inconsistencies.is_empty() {
        let _ = writeln!(s, "
the model contradicts itself");
        for c in &v.inconsistencies {
            let _ = writeln!(s, "  {c}");
        }
    }

    let assumed: Vec<&crate::Input> = v
        .inputs
        .iter()
        .filter(|i| matches!(i.source, Source::Assumed(_)))
        .collect();
    let from_model = v.inputs.len() - assumed.len();
    let _ = writeln!(
        s,
        "\nwhere the inputs came from: {from_model} measured off the model, {} assumed",
        assumed.len()
    );
    for a in &assumed {
        if let Source::Assumed(why) = a.source {
            let _ = writeln!(
                s,
                "  {:<28} {:>10.3} {:<7} {why}",
                a.name, a.value, a.unit
            );
        }
    }
    let _ = writeln!(
        s,
        "
Every number above is only as good as the tyre coefficients and the assumptions listed."
    );
    let _ = writeln!(
        s,
        "The tyre model has not been measured against a real tyre, which is the largest single"
    );
    let _ = writeln!(
        s,
        "source of error here. There are also no suspension kinematics, so camber and toe do not"
    );
    let _ = writeln!(s, "change as the wheels move.");
    s
}
