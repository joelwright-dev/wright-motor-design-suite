//! The build pack has to be something a person could actually follow.
//!
//! These tests check the properties that make it so: every part appears, every joint becomes a
//! step, no step asks for a part that has not been put down yet, and nothing is quietly dropped.

use std::path::PathBuf;

use wmds_geom::MeshKernel;
use wmds_mfg::{BuildPack, StepKind};
use wmds_model::{Library, Overrides, ResolvedAssembly, resolve_assembly};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn reference() -> (Library, ResolvedAssembly, Vec<wmds_geom::PartMass>) {
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
    let built = wmds_geom::build_assembly(&k, &asm);
    let density_of = |m: &str| lib.density(m);
    let masses = wmds_geom::assembly_masses(&k, &built, &density_of);
    (lib, asm, masses)
}

#[test]
fn every_part_in_the_vehicle_reaches_the_bill_of_materials() {
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 1);
    let counted: usize = pack.bom.iter().map(|b| b.quantity).sum();
    assert_eq!(
        counted,
        asm.instances.len(),
        "the bill of materials lists {counted} pieces but the vehicle has {}",
        asm.instances.len()
    );
    for line in &pack.bom {
        assert!(line.quantity > 0);
        assert!(!line.source_id.is_empty());
    }
}

#[test]
fn every_joint_becomes_exactly_one_step() {
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 1);
    let mut seen: Vec<&str> = pack
        .steps
        .iter()
        .filter_map(|s| s.mate.as_deref())
        .collect();
    seen.sort();
    let before = seen.len();
    seen.dedup();
    assert_eq!(before, seen.len(), "a joint appears in two steps");
    assert_eq!(
        seen.len(),
        asm.mates.len(),
        "there are {} joints but {} steps mentioning one; a joint with no step is a bolt \
         nobody is told to fit",
        asm.mates.len(),
        seen.len()
    );
}

#[test]
fn no_step_asks_for_something_that_is_not_there_yet() {
    // The property that makes the instructions followable. Walk them in order, keeping track of
    // what is on the bench, and check every step only touches things already on it.
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 1);
    let by_id: std::collections::HashMap<&str, &wmds_model::ResolvedMate> =
        asm.mates.iter().map(|m| (m.id.as_str(), m)).collect();

    let mut on_the_bench: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut checked = 0;
    for step in &pack.steps {
        match &step.mate {
            None => {
                // A "lay out X to build on" step. Take the name back out of the sentence by
                // trusting the model instead: every root unit counts as present.
                for u in &asm.mateable {
                    if matches!(
                        u.placed_by,
                        wmds_model::PlacedBy::Root | wmds_model::PlacedBy::Free(_)
                    ) {
                        on_the_bench.insert(u.unit.clone());
                    }
                }
                for i in &asm.instances {
                    if matches!(
                        i.placed_by,
                        wmds_model::PlacedBy::Root | wmds_model::PlacedBy::Free(_)
                    ) {
                        on_the_bench.insert(i.id.clone());
                    }
                }
            }
            Some(id) => {
                let m = by_id[id.as_str()];
                let have_a = on_the_bench.contains(&m.a);
                let have_b = on_the_bench.contains(&m.b);
                assert!(
                    have_a || have_b,
                    "step {} joins {} to {} and neither is on the bench yet",
                    step.number,
                    m.a,
                    m.b
                );
                if step.kind == StepKind::AlsoBolt {
                    assert!(
                        have_a && have_b,
                        "step {} is a second fixing but {} is not on the bench",
                        step.number,
                        if have_a { &m.b } else { &m.a }
                    );
                }
                on_the_bench.insert(m.a.clone());
                on_the_bench.insert(m.b.clone());
                checked += 1;
            }
        }
    }
    assert!(checked > 50, "only {checked} joining steps were checked");
}

#[test]
fn sub_assemblies_are_built_before_they_are_fitted() {
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 1);
    let first_vehicle_step = pack
        .steps
        .iter()
        .position(|s| s.group.is_none())
        .expect("the vehicle itself has steps");
    let last_sub_step = pack
        .steps
        .iter()
        .rposition(|s| s.group.is_some())
        .expect("there are sub-assemblies");
    assert!(
        last_sub_step < first_vehicle_step,
        "a corner is still being built after the vehicle assembly has started"
    );
}

#[test]
fn the_fastener_schedule_counts_every_bolt_including_the_ones_inside_corners() {
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 1);
    let from_schedule: u32 = pack.fasteners.iter().map(|f| f.quantity).sum();
    let from_model: u32 = asm
        .mates
        .iter()
        .filter_map(|m| m.fasteners.as_ref())
        .map(|f| f.quantity)
        .sum();
    assert_eq!(from_schedule, from_model);
    assert!(
        from_schedule > 60,
        "only {from_schedule} fasteners; the joints inside the suspension corners are probably \
         being dropped again"
    );
}

#[test]
fn the_manufacturing_route_follows_the_volume() {
    // The upright is cast above 200 off and machined below it. That choice is the whole point
    // of declaring methods with scale ranges.
    let (lib, asm, masses) = reference();
    let one_off = BuildPack::build(&asm, &lib, &masses, 1);
    let production = BuildPack::build(&asm, &lib, &masses, 5000);

    let route = |p: &BuildPack, id: &str| -> Option<String> {
        p.bom
            .iter()
            .find(|b| b.source_id == id)
            .and_then(|b| b.plan.as_ref())
            .map(|m| m.method.clone())
    };
    assert_eq!(
        route(&one_off, "suspension/uprights/upright-dw").as_deref(),
        Some("machined"),
        "a single vehicle should not be paying for casting tooling"
    );
    assert_eq!(
        route(&production, "suspension/uprights/upright-dw").as_deref(),
        Some("cast-aluminium"),
        "at five thousand off the casting should win"
    );
    // And the cost should reflect it.
    let (unit_one, tooling_one) = one_off.cost();
    let (unit_many, tooling_many) = production.cost();
    assert!(unit_many.unwrap() < unit_one.unwrap());
    assert!(tooling_many > tooling_one);
}

#[test]
fn the_cut_list_gives_real_lengths() {
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 1);
    assert!(!pack.cuts.is_empty(), "nothing to cut, which cannot be right");
    for c in &pack.cuts {
        assert!(
            c.length > 0.001 && c.length < 6.0,
            "{} {} is {:.3} m long, which is not a piece of tube",
            c.source_id,
            c.body,
            c.length
        );
        assert!(c.quantity > 0);
    }
    // The chassis rails are the longest thing in the vehicle and must be in here.
    assert!(
        pack.cuts
            .iter()
            .any(|c| c.source_id.contains("rail") && c.length > 1.0),
        "the chassis rails are missing from the cut list"
    );
}

#[test]
fn the_pack_renders_without_losing_anything() {
    let (lib, asm, masses) = reference();
    let pack = BuildPack::build(&asm, &lib, &masses, 100);
    let md = wmds_mfg::write_markdown(&pack);
    for line in &pack.bom {
        assert!(
            md.contains(&line.source_id),
            "{} is missing from the document",
            line.source_id
        );
    }
    let steps = pack
        .steps
        .iter()
        .filter(|s| matches!(s.kind, StepKind::Join | StepKind::AlsoBolt))
        .count();
    assert!(steps > 50);
    let text = wmds_mfg::write_text(&pack);
    assert!(text.contains("assembly"));
    assert!(text.contains("cut list"));
}
