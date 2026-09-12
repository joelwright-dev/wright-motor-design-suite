//! Every definition file in the library must survive being written back out.
//!
//! This is the test that makes an editor trustworthy. If a part can be opened, written and
//! opened again without changing, then editing it in the application cannot quietly lose
//! anything the file said. Running it over the real library rather than a fixture means every
//! feature anyone actually uses is covered, including the ones added after this was written.

use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn files(dir: &Path, suffix: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, suffix, out);
        } else if p.to_string_lossy().ends_with(suffix) {
            out.push(p);
        }
    }
}

#[test]
fn every_primitive_in_the_library_round_trips() {
    let root = project_root();
    let mut paths = Vec::new();
    files(&root.join("library"), ".prim.kdl", &mut paths);
    paths.sort();
    assert!(paths.len() >= 10, "found only {} primitives", paths.len());

    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(path).expect("readable");
        let original = wmds_schema::parse_primitive(&name, &src)
            .unwrap_or_else(|e| panic!("{name} does not parse: {e:?}"));

        let written = wmds_schema::write_primitive(&original);
        let reparsed = wmds_schema::parse_primitive(&name, &written).unwrap_or_else(|e| {
            panic!("{name} was written in a form it cannot read back:\n{written}\n{e:?}")
        });

        // Compare what the model actually holds, not the text, because comments and layout are
        // allowed to differ.
        assert_eq!(reparsed.id, original.id, "{name}: id");
        assert_eq!(reparsed.version, original.version, "{name}: version");
        assert_eq!(
            reparsed.description, original.description,
            "{name}: description"
        );
        assert_eq!(reparsed.category, original.category, "{name}: category");
        assert_eq!(reparsed.sub, original.sub, "{name}: sub");
        assert_eq!(reparsed.material, original.material, "{name}: material");
        assert_eq!(
            reparsed.params.len(),
            original.params.len(),
            "{name}: lost a parameter"
        );
        for (a, b) in reparsed.params.iter().zip(&original.params) {
            assert_eq!(a.name, b.name, "{name}: parameter order changed");
            assert_eq!(a.unit, b.unit, "{name}: parameter {} unit", a.name);
            assert_eq!(
                a.expr.is_some(),
                b.expr.is_some(),
                "{name}: parameter {} stopped being derived",
                a.name
            );
        }
        assert_eq!(
            reparsed.variants.len(),
            original.variants.len(),
            "{name}: lost a variant"
        );
        for (a, b) in reparsed.variants.iter().zip(&original.variants) {
            assert_eq!(a.name, b.name, "{name}: variant name");
            assert_eq!(a.options, b.options, "{name}: variant {} options", a.name);
            assert_eq!(
                a.mirror_when, b.mirror_when,
                "{name}: variant {} mirror_when; a handed part that forgets this stops mirroring",
                a.name
            );
            assert_eq!(a.mirror_plane, b.mirror_plane, "{name}: mirror_plane");
        }
        assert_eq!(
            reparsed.geometry.len(),
            original.geometry.len(),
            "{name}: lost a geometry level"
        );
        for (a, b) in reparsed.geometry.iter().zip(&original.geometry) {
            assert_eq!(a.level, b.level, "{name}: level name");
            assert_eq!(
                a.features.len(),
                b.features.len(),
                "{name}: level {} lost a feature",
                a.level
            );
            for (fa, fb) in a.features.iter().zip(&b.features) {
                assert_eq!(fa.op, fb.op, "{name}: feature operation");
                assert_eq!(
                    fa.name, fb.name,
                    "{name}: feature name; a boolean that names its bodies breaks without it"
                );
                assert_eq!(
                    fa.args.keys().collect::<Vec<_>>(),
                    fb.args.keys().collect::<Vec<_>>(),
                    "{name}: feature {} arguments",
                    fa.op
                );
            }
        }
        assert_eq!(
            reparsed.ports.len(),
            original.ports.len(),
            "{name}: lost a port"
        );
        for (a, b) in reparsed.ports.iter().zip(&original.ports) {
            assert_eq!(a.name, b.name, "{name}: port name");
            assert_eq!(a.port_type, b.port_type, "{name}: port {} type", a.name);
            assert_eq!(
                a.params.keys().collect::<Vec<_>>(),
                b.params.keys().collect::<Vec<_>>(),
                "{name}: port {} parameters",
                a.name
            );
            assert_eq!(a.grid, b.grid, "{name}: port {} grid flag", a.name);
        }
        assert_eq!(
            reparsed.manufacturing.len(),
            original.manufacturing.len(),
            "{name}: lost a manufacturing method"
        );
        assert_eq!(
            reparsed.compliance_tags, original.compliance_tags,
            "{name}: compliance tags; the rules read these"
        );

        // Writing twice must give identical text, or every save churns the diff.
        let again = wmds_schema::write_primitive(&reparsed);
        assert_eq!(written, again, "{name}: writing is not idempotent");
    }
}

#[test]
fn every_assembly_in_the_library_round_trips() {
    let root = project_root();
    let mut paths = Vec::new();
    files(&root.join("library"), ".asm.kdl", &mut paths);
    files(&root.join("vehicles"), ".veh.kdl", &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no assemblies found");

    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(path).expect("readable");
        let original = wmds_schema::parse_assembly(&name, &src)
            .unwrap_or_else(|e| panic!("{name} does not parse: {e:?}"));
        let written = wmds_schema::write_assembly(&original);
        let reparsed = wmds_schema::parse_assembly(&name, &written).unwrap_or_else(|e| {
            panic!("{name} was written in a form it cannot read back:\n{written}\n{e:?}")
        });
        assert_eq!(reparsed.instances.len(), original.instances.len(), "{name}: instances");
        assert_eq!(reparsed.mates.len(), original.mates.len(), "{name}: mates");
        assert_eq!(reparsed.exports.len(), original.exports.len(), "{name}: exports");
        assert_eq!(
            reparsed.point_masses.len(),
            original.point_masses.len(),
            "{name}: point masses"
        );
        assert_eq!(reparsed.params.len(), original.params.len(), "{name}: params");
        assert_eq!(reparsed.root, original.root, "{name}: root");
        let again = wmds_schema::write_assembly(&reparsed);
        assert_eq!(written, again, "{name}: writing is not idempotent");
    }
}
