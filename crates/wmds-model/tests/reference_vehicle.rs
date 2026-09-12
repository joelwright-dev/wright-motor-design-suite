//! Golden-model test: the reference vehicle resolves, and its chassis lands where it should.
//!
//! This is the test that fails when a change breaks the thing the suite exists to do. It reads
//! the real library and the real vehicle file rather than fixtures, so a broken definition file
//! fails here too.

use std::path::PathBuf;

use wmds_model::{Library, Overrides, PlacedBy, generate_chassis, resolve_assembly};

fn project_root() -> PathBuf {
    // crates/wmds-model -> repository root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn mm(v: f64) -> f64 {
    v * 1000.0
}

#[test]
fn library_loads_without_failures() {
    let lib = Library::load(&project_root()).expect("library should load");
    assert!(
        lib.failures.is_empty(),
        "definition files failed to load: {:#?}",
        lib.failures
            .iter()
            .map(|(p, e)| format!("{}: {e}", p.display()))
            .collect::<Vec<_>>()
    );
    assert!(!lib.primitives.is_empty(), "no primitives were loaded");
    assert!(
        lib.port_types.contains_key("mcds.grid-station"),
        "port registry is missing the grid station"
    );
    assert!(lib.chassis.contains_key("mcds-v1"), "MCDSv1 is missing");
}

#[test]
fn reference_vehicle_resolves_and_places_everything() {
    let root = project_root();
    let lib = Library::load(&root).expect("library should load");
    let file = root
        .join("vehicles")
        .join("reference-city-ev")
        .join("reference-city-ev.veh.kdl");
    let def = Library::load_assembly_file(&file).expect("reference vehicle should parse");

    let req = def
        .chassis
        .as_ref()
        .expect("the reference vehicle has a chassis");
    let chassis = generate_chassis(&lib, req).expect("chassis should generate");

    // A 1100 mm front plus a 1900 mm central section, with the joint plane at the origin.
    assert!((mm(chassis.length.value) - 3000.0).abs() < 1e-6);
    assert_eq!(chassis.station_range, (-11, 19));
    let (x0, x1) = chassis.sections["front"];
    assert!(
        (mm(x0) + 1100.0).abs() < 1e-6 && x1.abs() < 1e-9,
        "front section should end at the origin"
    );

    let asm = resolve_assembly(
        &lib,
        &def,
        &Overrides::default(),
        vec![(req.id.clone(), chassis.assembly.clone())],
    )
    .expect("vehicle should resolve");

    assert!(
        asm.errors.is_empty(),
        "resolution reported errors: {:#?}",
        asm.errors
    );
    assert!(
        asm.mates.iter().all(|m| m.compatible.is_ok()),
        "a mate was rejected: {:#?}",
        asm.mates
            .iter()
            .filter(|m| m.compatible.is_err())
            .collect::<Vec<_>>()
    );
    assert!(
        asm.instances
            .iter()
            .all(|i| i.placed_by != PlacedBy::Unreached),
        "unplaced parts: {:?}",
        asm.instances
            .iter()
            .filter(|i| i.placed_by == PlacedBy::Unreached)
            .map(|i| &i.id)
            .collect::<Vec<_>>()
    );

    // The battery hangs from four grid stations; check it landed where the geometry says it must.
    // Front feet at station 2 (x = 200 mm) with a 1200 mm foot spacing puts the centre at 800 mm,
    // laterally centred, and sitting on top of the rails at z = +75 mm (half its height).
    let battery = asm.instance("battery").expect("battery instance");
    let t = battery.placement.translation;
    assert!((mm(t[0]) - 800.0).abs() < 0.5, "battery x was {}", mm(t[0]));
    assert!(
        mm(t[1]).abs() < 0.5,
        "battery should be laterally centred, was {}",
        mm(t[1])
    );
    assert!((mm(t[2]) - 75.0).abs() < 0.5, "battery z was {}", mm(t[2]));

    // No mate should be left dangling in space: every warning about closure is a real defect.
    let closure: Vec<&String> = asm
        .warnings
        .iter()
        .filter(|w| w.contains("does not close"))
        .collect();
    assert!(closure.is_empty(), "mates that do not close: {closure:#?}");
}

#[test]
fn changing_a_section_length_moves_everything_downstream() {
    // The central promise of a modular chassis: lengthen a section and the parts mounted behind
    // it follow, because position comes from the mate graph rather than from stored coordinates.
    let root = project_root();
    let lib = Library::load(&root).expect("library should load");
    let file = root
        .join("vehicles")
        .join("reference-city-ev")
        .join("reference-city-ev.veh.kdl");
    let mut def = Library::load_assembly_file(&file).expect("reference vehicle should parse");

    let baseline = {
        let req = def.chassis.as_ref().unwrap();
        let chassis = generate_chassis(&lib, req).unwrap();
        let asm = resolve_assembly(
            &lib,
            &def,
            &Overrides::default(),
            vec![(req.id.clone(), chassis.assembly)],
        )
        .unwrap();
        asm.instance("drive").unwrap().placement.translation
    };

    // Stretch the front section by 300 mm. Station indices are measured from the front joint
    // plane, which does not move, so the drive unit should stay exactly where it was.
    {
        let req = def.chassis.as_mut().unwrap();
        req.section_lengths.insert(
            "front".to_string(),
            wmds_expr::Expr::Num(wmds_units::Quantity::from_unit(1400.0, "mm").unwrap()),
        );
    }
    let req = def.chassis.as_ref().unwrap();
    let chassis = generate_chassis(&lib, req).expect("stretched chassis should generate");
    assert!((mm(chassis.length.value) - 3300.0).abs() < 1e-6);
    let asm = resolve_assembly(
        &lib,
        &def,
        &Overrides::default(),
        vec![(req.id.clone(), chassis.assembly)],
    )
    .unwrap();
    let moved = asm.instance("drive").unwrap().placement.translation;
    for i in 0..3 {
        assert!(
            (moved[i] - baseline[i]).abs() < 1e-9,
            "the drive unit moved when only the front section changed: {baseline:?} -> {moved:?}"
        );
    }
    assert!(asm.errors.is_empty(), "{:#?}", asm.errors);
}

#[test]
fn a_mate_between_incompatible_ports_is_refused() {
    // Bolting a high voltage connector to a chassis grid station is nonsense, and the port
    // registry is what makes it impossible rather than merely unwise.
    let root = project_root();
    let lib = Library::load(&root).expect("library should load");
    let src = r#"
vehicle "test/bad-mate" version="0.0.1" {
    category "MA"
    chassis system="mcds-v1" id="chassis" {
        configuration "2/3-length"
        width "narrow"
        rail_section "120x60"
        section "front" length="1100 mm"
        section "central" length="1900 mm"
    }
    instances {
        instance "battery" primitive="energy/battery/pack-modular"
    }
    mates {
        mate "wrong" a="chassis.station_left_2" b="battery.hv_out"
    }
}
"#;
    let def = wmds_schema::parse_assembly("bad.veh.kdl", src).expect("should parse");
    let req = def.chassis.as_ref().unwrap();
    let chassis = generate_chassis(&lib, req).unwrap();
    let asm = resolve_assembly(
        &lib,
        &def,
        &Overrides::default(),
        vec![(req.id.clone(), chassis.assembly)],
    )
    .unwrap();
    assert!(
        asm.mates.iter().any(|m| m.compatible.is_err()),
        "an incompatible mate was accepted"
    );
    assert!(
        !asm.is_ok(),
        "the assembly should not report itself as sound"
    );
}

#[test]
fn the_front_corners_mirror_and_close() {
    // The corner is the first assembly with a closed kinematic loop: the upright is reached
    // through the lower arm and the upper arm then has to arrive at the same place. It is also
    // the first use of variant mirroring, so the right corner is the left one reflected rather
    // than a second file to keep in step.
    let root = project_root();
    let lib = Library::load(&root).expect("library should load");
    let file = root
        .join("vehicles")
        .join("reference-city-ev")
        .join("reference-city-ev.veh.kdl");
    let def = Library::load_assembly_file(&file).expect("vehicle should parse");
    let req = def.chassis.as_ref().unwrap();
    let chassis = generate_chassis(&lib, req).unwrap();
    let asm = resolve_assembly(
        &lib,
        &def,
        &Overrides::default(),
        vec![(req.id.clone(), chassis.assembly)],
    )
    .unwrap();

    assert!(asm.errors.is_empty(), "{:#?}", asm.errors);
    let closure: Vec<&String> = asm
        .warnings
        .iter()
        .filter(|w| w.contains("does not close"))
        .collect();
    assert!(
        closure.is_empty(),
        "the suspension loop does not close: {closure:#?}"
    );

    // Every left part has a right twin at the mirrored lateral position.
    for part in ["wheel", "tyre", "upright", "lower_arm", "upper_arm"] {
        let l = asm
            .instance(&format!("corner_fl.{part}"))
            .unwrap_or_else(|| panic!("missing corner_fl.{part}"))
            .placement
            .translation;
        let r = asm
            .instance(&format!("corner_fr.{part}"))
            .unwrap_or_else(|| panic!("missing corner_fr.{part}"))
            .placement
            .translation;
        assert!((l[0] - r[0]).abs() < 1e-9, "{part}: x differs");
        assert!(
            (l[1] + r[1]).abs() < 1e-9,
            "{part}: y should be mirrored, got {} and {}",
            mm(l[1]),
            mm(r[1])
        );
        assert!((l[2] - r[2]).abs() < 1e-9, "{part}: z differs");
    }

    // Front track, measured between the wheel mounting faces.
    let l = asm
        .instance("corner_fl.wheel")
        .unwrap()
        .placement
        .translation;
    let track = mm(l[1]) * 2.0;
    assert!(
        (track - 1260.0).abs() < 0.5,
        "front track was {track} mm, expected 1260"
    );

    // The front wheels sit ahead of the chassis front joint plane, where a front axle belongs.
    assert!(mm(l[0]) < -500.0, "front wheel centre at x = {}", mm(l[0]));
}
