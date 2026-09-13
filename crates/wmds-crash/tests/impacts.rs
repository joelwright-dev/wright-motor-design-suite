//! Does the crash model obey conservation of energy, and does it respond the way a structure
//! does?
//!
//! The model is built here rather than read from the library, so a change to the reference
//! vehicle cannot quietly move the answers.

use wmds_crash::{CrashModel, Element, Slice, Standard};

/// A car-like chain: a light nose, a crush zone, and a heavy compartment behind it.
fn car(force_kn: f64, stroke_mm: f64, zones: usize) -> CrashModel {
    let mut slices = Vec::new();
    let mut elements = Vec::new();
    let step = stroke_mm / 1e3 / 0.7;
    for i in 0..zones {
        slices.push(Slice {
            x: -1.0 + step * i as f64,
            mass: 20.0,
            contents: vec![format!("nose {i}")],
            occupied: false,
        });
        elements.push(Element {
            name: format!("zone {i}"),
            force: force_kn * 1e3,
            peak_force: force_kn * 1e3 * 1.5,
            stroke: stroke_mm / 1e3,
            capacity: force_kn * 1e3 * stroke_mm / 1e3,
            validated: true,
            contributions: vec![("test/steel".into(), force_kn * 1e3)],
        });
    }
    slices.push(Slice {
        x: -1.0 + step * zones as f64,
        mass: 1000.0,
        contents: vec!["compartment".into()],
        occupied: true,
    });
    CrashModel {
        vehicle: "test/car".into(),
        slices,
        elements,
        mass: 1000.0 + 20.0 * zones as f64,
        compartment_front: -1.0 + step * zones as f64,
        notes: Vec::new(),
    }
}

#[test]
fn the_car_stops_and_the_energy_goes_into_the_structure() {
    // 150 kN over 600 mm is 90 kJ, against 121 kJ at 56 km/h into a barrier for this mass, so
    // it should use most of the stroke and not quite bottom out on capacity alone.
    let m = car(220.0, 150.0, 4);
    let r = wmds_crash::run(&m, Standard::FullFrontal);
    let speed = Standard::FullFrontal.speed_kph() / 3.6;
    let expected = 0.5 * r.mass * speed * speed;
    assert!(
        (r.energy - expected).abs() / expected < 1e-6,
        "the energy going in should be half m v squared"
    );
    // Distance from a constant force: energy over force.
    let ideal = r.energy / (220.0 * 1e3);
    assert!(
        (r.crush - ideal).abs() / ideal < 0.35,
        "crushed {:.0} mm; a constant {:.0} kN against {:.0} kJ says about {:.0} mm",
        r.crush * 1e3,
        220.0,
        r.energy / 1e3,
        ideal * 1e3
    );
    assert!(r.duration > 0.01 && r.duration < 0.2, "{:.0} ms", r.duration * 1e3);
}

#[test]
fn a_softer_structure_crushes_further_and_hits_the_occupants_less_hard() {
    // The central trade in a crash structure, and the reason crush length is worth paying for.
    let stiff = wmds_crash::run(&car(400.0, 200.0, 4), Standard::FullFrontal);
    let soft = wmds_crash::run(&car(200.0, 200.0, 4), Standard::FullFrontal);
    assert!(
        soft.crush > stiff.crush,
        "the softer structure should crush further: {:.0} against {:.0} mm",
        soft.crush * 1e3,
        stiff.crush * 1e3
    );
    assert!(
        soft.olc < stiff.olc,
        "the softer structure should be kinder to the occupants: {:.0} against {:.0} g",
        soft.olc,
        stiff.olc
    );
    assert!(
        soft.duration > stiff.duration,
        "and it should take longer to stop"
    );
}

#[test]
fn a_structure_with_nowhere_to_crush_bottoms_out_and_says_so() {
    // 80 mm of total stroke cannot absorb a 56 km/h impact whatever its force.
    let m = car(150.0, 40.0, 2);
    let r = wmds_crash::run(&m, Standard::FullFrontal);
    assert!(
        r.bottomed_out,
        "80 mm of crush should not stop a tonne from 56 km/h; it crushed {:.0} mm of {:.0} mm \
         available",
        r.crush * 1e3,
        80.0
    );
    assert!(r.intrusion > 0.0, "and the compartment should be taking load");
}

#[test]
fn more_crush_length_at_the_same_force_gives_a_gentler_stop() {
    let short = wmds_crash::run(&car(250.0, 100.0, 3), Standard::FullFrontal);
    let long = wmds_crash::run(&car(250.0, 100.0, 8), Standard::FullFrontal);
    assert!(
        long.olc <= short.olc,
        "more crush length should not make it worse: {:.0} against {:.0} g",
        long.olc,
        short.olc
    );
    assert!(!long.bottomed_out, "with 800 mm it should not bottom out");
}

#[test]
fn a_faster_impact_is_harder() {
    let m = car(250.0, 150.0, 5);
    let slow = wmds_crash::run(&m, Standard::Pole);
    let fast = wmds_crash::run(&m, Standard::OffsetFrontal);
    assert!(fast.energy > slow.energy);
    assert!(
        fast.crush > slow.crush,
        "64 km/h should crush further than 32: {:.0} against {:.0} mm",
        fast.crush * 1e3,
        slow.crush * 1e3
    );
    assert!(fast.olc > slow.olc);
}

#[test]
fn an_unvalidated_material_is_called_out() {
    let mut m = car(250.0, 150.0, 4);
    for e in &mut m.elements {
        e.validated = false;
    }
    let r = wmds_crash::run(&m, Standard::FullFrontal);
    assert!(!r.validated);
    assert!(
        r.notes.iter().any(|n| n.contains("not evidence")),
        "the report must say the result is not evidence: {:?}",
        r.notes
    );
}

#[test]
fn a_vehicle_with_no_crush_structure_is_reported_rather_than_crashing_the_program() {
    let mut m = car(250.0, 150.0, 3);
    m.elements.clear();
    let r = wmds_crash::run(&m, Standard::FullFrontal);
    assert!(r.bottomed_out);
    assert!(r.notes.iter().any(|n| n.contains("no crushable structure")));
}
