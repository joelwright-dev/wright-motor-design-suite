//! A right-hand part must be the mirror image of the left, in the metal as well as on paper.
//!
//! Ports being mirrored is not enough: if the solid is not mirrored with them, every handed part
//! on one side of the car is drawn reaching the wrong way while the numbers all look right. That
//! is invisible in a parts table and obvious the moment anyone looks at the vehicle from above.

use std::path::PathBuf;

use wmds_geom::{MeshCache, MeshKernel, build_assembly_cached};
use wmds_model::{Library, Overrides, resolve_assembly};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn every_handed_part_is_drawn_mirrored_about_the_centreline() {
    let root = project_root();
    let lib = Library::load(&root).expect("library loads");
    let def = Library::load_assembly_file(
        &root.join("vehicles/reference-city-ev/reference-city-ev.veh.kdl"),
    )
    .expect("the reference vehicle loads");
    let mut extra = Vec::new();
    if let Some(req) = &def.chassis {
        let g = wmds_model::generate_chassis(&lib, req).expect("chassis generates");
        extra.push((req.id.clone(), g.assembly));
    }
    let asm = resolve_assembly(&lib, &def, &Overrides::default(), extra).expect("resolves");

    let k = MeshKernel::default();
    let mut cache = MeshCache::new();
    let built = build_assembly_cached(&k, &asm, &mut cache, 2e-4, &|_| Some(2700.0));

    // Compare each left corner part against its right twin. The bounding box of the right part
    // must be the left one reflected in y: lo and hi swap and change sign.
    let mut checked = 0;
    for (left_unit, right_unit) in [("corner_fl", "corner_fr"), ("corner_rl", "corner_rr")] {
        for part in [
            "lower_arm",
            "upper_arm",
            "upright",
            "bracket_lower_front",
            "caliper",
            "wheel",
            "disc",
        ] {
            let l = built.bounds.get(&format!("{left_unit}.{part}"));
            let r = built.bounds.get(&format!("{right_unit}.{part}"));
            let (Some(l), Some(r)) = (l, r) else { continue };
            checked += 1;
            let tol = 1e-4;
            assert!(
                (l.0[1] + r.1[1]).abs() < tol && (l.1[1] + r.0[1]).abs() < tol,
                "{part}: the right-hand solid is not the mirror of the left.\n  \
                 {left_unit}.{part} spans y {:.1} to {:.1} mm\n  \
                 {right_unit}.{part} spans y {:.1} to {:.1} mm\n  \
                 expected the right to span y {:.1} to {:.1} mm",
                l.0[1] * 1e3,
                l.1[1] * 1e3,
                r.0[1] * 1e3,
                r.1[1] * 1e3,
                -l.1[1] * 1e3,
                -l.0[1] * 1e3
            );
            // x and z must be identical, not mirrored.
            assert!(
                (l.0[0] - r.0[0]).abs() < tol && (l.0[2] - r.0[2]).abs() < tol,
                "{part}: the two sides differ in x or z, which no left/right mirror should do"
            );
        }
    }
    assert!(checked >= 8, "only {checked} parts were compared");
}

#[test]
fn the_tie_rods_mirror_too() {
    let root = project_root();
    let lib = Library::load(&root).expect("library loads");
    let def = Library::load_assembly_file(
        &root.join("vehicles/reference-city-ev/reference-city-ev.veh.kdl"),
    )
    .expect("loads");
    let mut extra = Vec::new();
    if let Some(req) = &def.chassis {
        let g = wmds_model::generate_chassis(&lib, req).expect("chassis generates");
        extra.push((req.id.clone(), g.assembly));
    }
    let asm = resolve_assembly(&lib, &def, &Overrides::default(), extra).expect("resolves");
    let k = MeshKernel::default();
    let mut cache = MeshCache::new();
    let built = build_assembly_cached(&k, &asm, &mut cache, 2e-4, &|_| Some(7850.0));

    let l = built.bounds.get("tie_rod_l").expect("left tie rod");
    let r = built.bounds.get("tie_rod_r").expect("right tie rod");
    assert!(
        (l.0[1] + r.1[1]).abs() < 1e-4 && (l.1[1] + r.0[1]).abs() < 1e-4,
        "the tie rods are not mirror images: left spans y {:.1} to {:.1}, right spans {:.1} to {:.1}",
        l.0[1] * 1e3,
        l.1[1] * 1e3,
        r.0[1] * 1e3,
        r.1[1] * 1e3
    );
}
