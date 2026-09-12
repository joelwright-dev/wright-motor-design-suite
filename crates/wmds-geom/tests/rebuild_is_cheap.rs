//! The editor rebuilds the whole vehicle after every change. This checks that doing so a second
//! time costs nothing in the geometry kernel, which is the only reason that design is affordable.

use std::path::{Path, PathBuf};

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

fn reference(root: &Path) -> (Library, wmds_model::ResolvedAssembly) {
    let lib = Library::load(root).expect("library loads");
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
    (lib, asm)
}

#[test]
fn the_second_build_of_an_unchanged_vehicle_touches_no_geometry() {
    let root = project_root();
    let (lib, asm) = reference(&root);
    let k = MeshKernel::default();
    let densities: std::collections::HashMap<String, f64> = lib
        .materials
        .iter()
        .map(|(id, m)| (id.clone(), m.density_si()))
        .collect();
    let density_of = |m: &str| densities.get(m).copied();

    let mut cache = MeshCache::new();
    let first = build_assembly_cached(&k, &asm, &mut cache, 2e-4, &density_of);
    let built_parts = cache.misses;
    assert!(built_parts > 0, "nothing was built at all");
    assert!(
        built_parts < asm.instances.len(),
        "the reference vehicle has repeated parts ({} instances) so a first build should already \
         reuse some, but it built {built_parts}",
        asm.instances.len()
    );

    let before = cache.misses;
    let second = build_assembly_cached(&k, &asm, &mut cache, 2e-4, &density_of);
    assert_eq!(
        cache.misses, before,
        "rebuilding an unchanged vehicle went back to the geometry kernel; every edit in the \
         editor would pay the full build cost"
    );

    // And the answer must be the same, or the cache is buying speed with correctness.
    assert_eq!(first.mesh.positions.len(), second.mesh.positions.len());
    assert_eq!(first.masses.len(), second.masses.len());
    let total = |m: &[wmds_geom::PartMass]| m.iter().filter_map(|x| x.mass).sum::<f64>();
    assert!((total(&first.masses) - total(&second.masses)).abs() < 1e-9);
}

#[test]
fn changing_one_part_rebuilds_only_that_part() {
    // The claim that makes the editor usable: move one slider, pay for one part.
    let root = project_root();
    let (lib, asm) = reference(&root);
    let k = MeshKernel::default();
    let density_of = |_: &str| Some(2700.0);

    let mut cache = MeshCache::new();
    build_assembly_cached(&k, &asm, &mut cache, 2e-4, &density_of);
    let after_first = cache.misses;

    // Rebuild with the battery made shorter, the way a slider would.
    let def = Library::load_assembly_file(
        &root.join("vehicles/reference-city-ev/reference-city-ev.veh.kdl"),
    )
    .expect("loads");
    let mut edited = def.clone();
    let battery = edited
        .instances
        .iter_mut()
        .find(|i| i.id == "battery")
        .expect("the reference vehicle has a battery");
    battery.params.insert(
        "length".into(),
        wmds_expr::parse("1100 mm").expect("parses"),
    );
    let mut extra = Vec::new();
    if let Some(req) = &edited.chassis {
        let g = wmds_model::generate_chassis(&lib, req).expect("chassis generates");
        extra.push((req.id.clone(), g.assembly));
    }
    let asm2 = resolve_assembly(&lib, &edited, &Overrides::default(), extra).expect("resolves");
    build_assembly_cached(&k, &asm2, &mut cache, 2e-4, &density_of);

    let newly_built = cache.misses - after_first;
    assert_eq!(
        newly_built, 1,
        "changing the battery length rebuilt {newly_built} parts; it should have rebuilt only \
         the battery"
    );
}
