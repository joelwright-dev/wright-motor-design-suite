//! The editing operations, separated from the user interface that calls them.
//!
//! Every operation works on an `AssemblyDef`, which is the thing that gets saved. The model is
//! then re-resolved from scratch. That is deliberate: there is one path from a definition to a
//! placed vehicle, and the editor takes it like everything else, so what you see is always what
//! the file says.

use indexmap::IndexMap;
use wmds_expr::Expr;
use wmds_model::Library;
use wmds_schema::{
    AssemblyDef, FastenerDef, InstanceDef, InstanceSource, MateDef, PortRef, Stage,
};

/// A thing that can be added to a vehicle.
#[derive(Clone, Debug)]
pub struct CatalogueEntry {
    pub id: String,
    pub kind: EntryKind,
    pub category: String,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Primitive,
    Assembly,
}

/// Everything in the library that can be placed, grouped by category.
pub fn catalogue(lib: &Library) -> Vec<CatalogueEntry> {
    let mut out: Vec<CatalogueEntry> = Vec::new();
    for (id, p) in &lib.primitives {
        out.push(CatalogueEntry {
            id: id.clone(),
            kind: EntryKind::Primitive,
            category: p.category.clone(),
            description: p.description.clone(),
        });
    }
    for (id, a) in &lib.assemblies {
        // A vehicle is not a component; only sub-assemblies belong in the catalogue.
        if a.kind == wmds_schema::AssemblyKind::Vehicle {
            continue;
        }
        out.push(CatalogueEntry {
            id: id.clone(),
            kind: EntryKind::Assembly,
            category: id.split('/').next().unwrap_or("assembly").to_string(),
            description: a.description.clone(),
        });
    }
    out.sort_by(|a, b| (a.category.as_str(), a.id.as_str()).cmp(&(b.category.as_str(), b.id.as_str())));
    out
}

/// A short, unique instance id derived from what is being added.
pub fn unique_id(def: &AssemblyDef, source: &str) -> String {
    let base = source.rsplit('/').next().unwrap_or(source).replace('-', "_");
    if !def.instances.iter().any(|i| i.id == base) {
        return base;
    }
    for n in 2.. {
        let candidate = format!("{base}_{n}");
        if !def.instances.iter().any(|i| i.id == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Add an instance. Returns its id.
pub fn add_instance(def: &mut AssemblyDef, entry: &CatalogueEntry) -> String {
    let id = unique_id(def, &entry.id);
    def.instances.push(InstanceDef {
        id: id.clone(),
        source: match entry.kind {
            EntryKind::Primitive => InstanceSource::Primitive(entry.id.clone()),
            EntryKind::Assembly => InstanceSource::Assembly(entry.id.clone()),
        },
        version: None,
        params: IndexMap::new(),
        variants: IndexMap::new(),
        placement: None,
    });
    id
}

/// Remove an instance and every mate that touched it, so the file never keeps a dangling
/// reference that would fail to load.
pub fn remove_instance(def: &mut AssemblyDef, id: &str) -> usize {
    def.instances.retain(|i| i.id != id);
    let before = def.mates.len();
    def.mates.retain(|m| m.a.instance != id && m.b.instance != id);
    def.exports.retain(|e| e.source.instance != id);
    if def.root.as_deref() == Some(id) {
        def.root = None;
    }
    before - def.mates.len()
}

pub fn remove_mate(def: &mut AssemblyDef, id: &str) {
    def.mates.retain(|m| m.id != id);
}

/// Set or clear one parameter override on an instance.
pub fn set_param(def: &mut AssemblyDef, instance: &str, name: &str, value: Option<String>) {
    let Some(i) = def.instances.iter_mut().find(|i| i.id == instance) else {
        return;
    };
    match value {
        Some(text) => {
            let e = match wmds_expr::parse(&text) {
                Ok(parsed) => Expr::TextOr(text, Box::new(parsed)),
                Err(_) => Expr::Str(text),
            };
            i.params.insert(name.to_string(), e);
        }
        None => {
            i.params.shift_remove(name);
        }
    }
}

pub fn set_variant(def: &mut AssemblyDef, instance: &str, name: &str, option: &str) {
    if let Some(i) = def.instances.iter_mut().find(|i| i.id == instance) {
        i.variants.insert(
            name.to_string(),
            Expr::TextOr(option.to_string(), Box::new(Expr::Str(option.to_string()))),
        );
    }
}

/// A unique mate id built from the two things being joined.
pub fn unique_mate_id(def: &AssemblyDef, a: &PortRef, b: &PortRef) -> String {
    let base = format!("{}_{}", a.instance, b.instance);
    if !def.mates.iter().any(|m| m.id == base) {
        return base;
    }
    for n in 2.. {
        let candidate = format!("{base}_{n}");
        if !def.mates.iter().any(|m| m.id == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Join two ports. The caller has already checked they are compatible.
pub fn add_mate(def: &mut AssemblyDef, a: PortRef, b: PortRef, fasteners: Option<FastenerDef>) -> String {
    let id = unique_mate_id(def, &a, &b);
    def.mates.push(MateDef {
        id: id.clone(),
        a,
        b,
        dof: None,
        stage: Some(Stage::Kit),
        offset: None,
        clock: None,
        fasteners,
    });
    id
}

/// A sensible starting fastener for a joint, so a new mate is not silently missing the
/// information the assembly instructions and the bill of materials both need.
pub fn default_fastener(bolt: Option<&str>) -> FastenerDef {
    let size = bolt.unwrap_or("M10").to_string();
    FastenerDef {
        kind: "bolt".into(),
        size,
        grade: "8.8".into(),
        quantity: 1,
        torque: None,
        nut: None,
        washer: None,
        thread_locker: None,
    }
}

/// A new, empty vehicle to start from.
pub fn new_vehicle(id: &str) -> AssemblyDef {
    AssemblyDef {
        kind: wmds_schema::AssemblyKind::Vehicle,
        id: id.to_string(),
        version: "0.1.0".into(),
        description: String::new(),
        params: Vec::new(),
        instances: Vec::new(),
        mates: Vec::new(),
        root: None,
        exports: Vec::new(),
        point_masses: Vec::new(),
        vehicle: Some(wmds_schema::VehicleMeta {
            category: "MA".into(),
            markets: vec!["AU".into()],
            rule_packs: vec!["wright-internal".into()],
        }),
        chassis: None,
    }
}

/// Give a vehicle a chassis, or change the one it has.
pub fn set_chassis(
    def: &mut AssemblyDef,
    system: &str,
    configuration: &str,
    width: &str,
    rail_section: &str,
    sections: &IndexMap<String, f64>,
) {
    let mut lengths: IndexMap<String, Expr> = IndexMap::new();
    for (kind, mm) in sections {
        let text = format!("{mm:.0} mm");
        let q = wmds_units::Quantity::from_unit(*mm, "mm").unwrap_or(wmds_units::Quantity::dimensionless(0.0));
        lengths.insert(kind.clone(), Expr::TextOr(text, Box::new(Expr::Num(q))));
    }
    def.chassis = Some(wmds_schema::ChassisRef {
        system: system.to_string(),
        configuration: configuration.to_string(),
        width: width.to_string(),
        rail_section: rail_section.to_string(),
        section_lengths: lengths,
        id: "chassis".to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn veh() -> AssemblyDef {
        new_vehicle("test/car")
    }

    fn entry(id: &str) -> CatalogueEntry {
        CatalogueEntry {
            id: id.into(),
            kind: EntryKind::Primitive,
            category: "test".into(),
            description: String::new(),
        }
    }

    #[test]
    fn added_instances_get_readable_unique_ids() {
        let mut d = veh();
        assert_eq!(add_instance(&mut d, &entry("wheels/wheel-steel")), "wheel_steel");
        assert_eq!(add_instance(&mut d, &entry("wheels/wheel-steel")), "wheel_steel_2");
        assert_eq!(add_instance(&mut d, &entry("wheels/wheel-steel")), "wheel_steel_3");
        assert_eq!(d.instances.len(), 3);
    }

    #[test]
    fn removing_an_instance_takes_its_mates_with_it() {
        let mut d = veh();
        let a = add_instance(&mut d, &entry("a/one"));
        let b = add_instance(&mut d, &entry("b/two"));
        add_mate(
            &mut d,
            PortRef { instance: a.clone(), port: "p".into() },
            PortRef { instance: b.clone(), port: "q".into() },
            None,
        );
        d.root = Some(a.clone());
        assert_eq!(d.mates.len(), 1);

        let removed = remove_instance(&mut d, &a);
        assert_eq!(removed, 1);
        assert!(d.mates.is_empty(), "a dangling mate would make the file unloadable");
        assert!(d.root.is_none(), "the root must not point at something that is gone");
        assert_eq!(d.instances.len(), 1);
    }

    #[test]
    fn parameters_can_be_set_and_cleared() {
        let mut d = veh();
        let a = add_instance(&mut d, &entry("a/one"));
        set_param(&mut d, &a, "span", Some("420 mm".into()));
        let i = &d.instances[0];
        assert_eq!(i.params.len(), 1);
        assert!(matches!(&i.params["span"], Expr::TextOr(t, _) if t == "420 mm"));

        set_param(&mut d, &a, "span", None);
        assert!(d.instances[0].params.is_empty());
    }

    #[test]
    fn an_edited_vehicle_still_writes_and_reloads() {
        // The editor is only useful if what it builds can be saved and opened again.
        let mut d = veh();
        d.description = "Built in the editor".into();
        let a = add_instance(&mut d, &entry("energy/battery/pack-modular"));
        set_param(&mut d, &a, "energy", Some("36 kWh".into()));
        set_variant(&mut d, &a, "hand", "left");
        let b = add_instance(&mut d, &entry("drivetrain/motor/drive-unit"));
        add_mate(
            &mut d,
            PortRef { instance: a.clone(), port: "hv_out".into() },
            PortRef { instance: b.clone(), port: "hv_in".into() },
            Some(default_fastener(Some("M12"))),
        );
        let mut sections = IndexMap::new();
        sections.insert("front".to_string(), 1100.0);
        sections.insert("central".to_string(), 1900.0);
        set_chassis(&mut d, "mcds-v1", "2/3-length", "narrow", "120x60", &sections);

        let text = wmds_schema::write_assembly(&d);
        let back = wmds_schema::parse_assembly("t.veh.kdl", &text)
            .map_err(|e| format!("the editor produced a file it cannot read:\n{text}\n{e:?}"))
            .unwrap();
        assert_eq!(back.instances.len(), 2);
        assert_eq!(back.mates.len(), 1);
        assert_eq!(back.description, "Built in the editor");
        assert_eq!(back.chassis.as_ref().unwrap().configuration, "2/3-length");
        assert_eq!(back.chassis.as_ref().unwrap().section_lengths.len(), 2);
    }
}

#[cfg(test)]
mod end_to_end {
    //! The claim the editor has to earn: a vehicle assembled entirely by clicking, with no file
    //! edited by hand, resolves and places its parts like any other.

    use super::*;
    use std::path::PathBuf;
    use wmds_model::{Overrides, PlacedBy, ports_compatible, resolve_assembly};

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Resolve the way the application does: generate the chassis, then place everything.
    fn build(lib: &Library, def: &AssemblyDef) -> wmds_model::ResolvedAssembly {
        let mut extra = Vec::new();
        if let Some(req) = &def.chassis {
            let g = wmds_model::generate_chassis(lib, req).expect("chassis generates");
            extra.push((req.id.clone(), g.assembly));
        }
        resolve_assembly(lib, def, &Overrides::default(), extra).expect("resolves")
    }

    #[test]
    fn a_vehicle_built_by_clicking_resolves_and_places() {
        let lib = Library::load(&project_root()).expect("library loads");

        // 1. New vehicle, then a chassis.
        let mut def = new_vehicle("test/clicked");
        let mut sections = IndexMap::new();
        sections.insert("front".to_string(), 1100.0);
        sections.insert("central".to_string(), 1900.0);
        set_chassis(&mut def, "mcds-v1", "2/3-length", "narrow", "120x60", &sections);

        // 2. Add a part from the catalogue, the way the Add button does.
        let cat = catalogue(&lib);
        let battery = cat
            .iter()
            .find(|e| e.id.contains("battery"))
            .expect("the library has a battery")
            .clone();
        let id = add_instance(&mut def, &battery);

        // Before anything is bolted, the part is in the vehicle but not positioned. The editor
        // says so rather than pretending it is at the origin on purpose.
        let asm = build(&lib, &def);
        let placed = asm.instance(&id).expect("the battery is in the model");
        assert_eq!(placed.placed_by, PlacedBy::Unreached);

        // 3. Choose a port on the battery, and ask what it can bolt to. This is exactly what the
        // joint picker does, and it is the part that must not offer a lie.
        let bat_ports = asm
            .mateable
            .iter()
            .find(|u| u.unit == id)
            .expect("the battery offers ports");
        let foot = bat_ports.ports.first().expect("at least one mounting foot").clone();

        let mut options: Vec<(String, String)> = Vec::new();
        for u in &asm.mateable {
            if u.unit == id {
                continue;
            }
            for p in &u.ports {
                if ports_compatible(
                    &lib,
                    &foot.port_type,
                    &foot.params,
                    "a",
                    &p.port_type,
                    &p.params,
                    "b",
                )
                .is_ok()
                {
                    options.push((u.unit.clone(), p.name.clone()));
                }
            }
        }
        assert!(
            !options.is_empty(),
            "a battery mounting foot must be able to bolt to the chassis grid; \
             it is a {} port and nothing in the vehicle accepted it",
            foot.port_type
        );
        let (unit, port) = options[0].clone();
        assert_eq!(unit, "chassis", "the first thing it should fit is the chassis");

        // 4. Bolt them together and rebuild.
        add_mate(
            &mut def,
            PortRef { instance: unit.clone(), port: port.clone() },
            PortRef { instance: id.clone(), port: foot.name.clone() },
            Some(default_fastener(Some("M12"))),
        );
        let asm = build(&lib, &def);
        let placed = asm.instance(&id).expect("the battery is still there");
        assert!(
            matches!(placed.placed_by, PlacedBy::Mate(_)),
            "after one joint the battery must be positioned by it, not left floating: {:?}",
            placed.placed_by
        );
        assert!(asm.errors.is_empty(), "errors after a legal joint: {:?}", asm.errors);

        // 5. Save and reopen, which is the only way the work survives.
        let text = wmds_schema::write_assembly(&def);
        let back = wmds_schema::parse_assembly("clicked.veh.kdl", &text)
            .unwrap_or_else(|e| panic!("the editor wrote a file it cannot read: {e:?}\n{text}"));
        let asm2 = build(&lib, &back);
        assert_eq!(
            asm2.instance(&id).unwrap().placement.translation,
            asm.instance(&id).unwrap().placement.translation,
            "reopening the saved file must put the part in the same place"
        );
    }

    #[test]
    fn deleting_a_part_leaves_a_vehicle_that_still_loads() {
        let lib = Library::load(&project_root()).expect("library loads");
        let mut def = new_vehicle("test/deleted");
        let mut sections = IndexMap::new();
        sections.insert("front".to_string(), 1100.0);
        sections.insert("central".to_string(), 1900.0);
        set_chassis(&mut def, "mcds-v1", "2/3-length", "narrow", "120x60", &sections);

        let cat = catalogue(&lib);
        let entry = cat.iter().find(|e| e.id.contains("battery")).unwrap().clone();
        let id = add_instance(&mut def, &entry);
        let asm = build(&lib, &def);
        let foot = asm
            .mateable
            .iter()
            .find(|u| u.unit == id)
            .unwrap()
            .ports
            .first()
            .unwrap()
            .name
            .clone();
        add_mate(
            &mut def,
            PortRef { instance: "chassis".into(), port: "station_left_1".into() },
            PortRef { instance: id.clone(), port: foot },
            None,
        );
        assert_eq!(def.mates.len(), 1);

        remove_instance(&mut def, &id);
        let text = wmds_schema::write_assembly(&def);
        let back = wmds_schema::parse_assembly("deleted.veh.kdl", &text)
            .unwrap_or_else(|e| panic!("unloadable after a delete: {e:?}\n{text}"));
        assert!(back.instances.is_empty());
        assert!(back.mates.is_empty(), "the joint to the deleted part must go too");
        let asm = build(&lib, &back);
        assert!(asm.errors.is_empty(), "{:?}", asm.errors);
    }
}
