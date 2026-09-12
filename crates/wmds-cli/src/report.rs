//! The report writers behind the `show` commands.
//!
//! Everything here is deliberately plain text: it has to be readable in a terminal, diffable in
//! a pull request, and greppable. Numbers that come from a placeholder say so.

use std::path::Path;
use std::process::ExitCode;

use indexmap::IndexMap;
use wmds_geom::GeomKernel;
use wmds_model::{Library, Overrides, PlacedBy, ResolvedAssembly, ResolvedMassProps, resolve};
use wmds_schema::ChassisRef;
use wmds_units::{Dim, Quantity};

#[cfg(feature = "occt")]
type Kernel = wmds_geom_occt::OcctKernel;
#[cfg(not(feature = "occt"))]
type Kernel = wmds_geom::MeshKernel;

#[cfg(feature = "occt")]
const KERNEL_NAME: &str = "OpenCASCADE";
#[cfg(not(feature = "occt"))]
const KERNEL_NAME: &str = "mesh kernel (preview: no booleans, overlapping volume counted twice)";

fn kernel() -> Kernel {
    Kernel::default()
}

fn mm(q: Quantity) -> f64 {
    q.to_unit("mm").unwrap_or(q.value * 1000.0)
}

// ------------------------------------------------------------------------------- primitives

pub fn show_primitive(file: &Path, sets: &[String], build: bool, step: Option<&Path>, stl: Option<&Path>) -> ExitCode {
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", file.display());
            return ExitCode::FAILURE;
        }
    };
    let name = file.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let def = match wmds_schema::parse_primitive(&name, &src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            return ExitCode::FAILURE;
        }
    };
    let overrides = match Overrides::parse_pairs(sets) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let r = match resolve(&def, &overrides) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!("{} v{}", r.id, r.version);
    if !def.description.is_empty() {
        println!("  {}", def.description);
    }
    println!("  category: {}{}", def.category, def.sub.as_ref().map(|s| format!(" / {s}")).unwrap_or_default());
    if let Some(m) = &r.material {
        println!("  material: {m}");
    }
    if !r.variants.is_empty() {
        println!("\nvariants");
        for (k, v) in &r.variants {
            println!("  {k:<14} {v}");
        }
    }
    println!("\nparams");
    for p in &def.params {
        let v = &r.params[&p.name];
        let kind = if p.expr.is_some() { "derived" } else { "" };
        println!("  {:<14} {:<18} {:<8} {}", p.name, v.to_string(), kind, p.doc.clone().unwrap_or_default());
    }
    println!("\nports");
    if r.ports.is_empty() {
        println!("  (none)");
    }
    for p in &r.ports {
        println!(
            "  {:<14} {:<22} at ({:.1}, {:.1}, {:.1}) mm  axis ({:.2}, {:.2}, {:.2}){}",
            p.name,
            p.port_type,
            mm(p.origin[0]),
            mm(p.origin[1]),
            mm(p.origin[2]),
            p.axis[0],
            p.axis[1],
            p.axis[2],
            p.load_rating.map(|q| format!("  rating {q}")).unwrap_or_default()
        );
        if !p.params.is_empty() {
            println!("  {:<14} {}", "", p.params.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "));
        }
    }
    println!("\ngeometry");
    for lvl in &r.geometry {
        println!("  level {}", lvl.level);
        for f in &lvl.features {
            let args: Vec<String> = f.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            println!("    {:<10} {:<12} {}", f.op, f.name.clone().unwrap_or_default(), args.join(" "));
        }
    }
    match &r.massprops {
        ResolvedMassProps::Computed => println!("\nmassprops: computed from geometry (pass --build to compute)"),
        ResolvedMassProps::Declared { mass, cg, .. } => println!(
            "\nmassprops: declared mass {mass}{}",
            cg.map(|c| format!(", cg ({:.1}, {:.1}, {:.1}) mm", mm(c[0]), mm(c[1]), mm(c[2]))).unwrap_or_default()
        ),
    }
    println!("\nmanufacturing");
    for m in &r.manufacturing {
        println!(
            "  {:<24} scale {:<10} fixed {:<12} per unit {:<14} exports {}",
            m.method,
            m.scale.clone().unwrap_or_default(),
            m.cost_fixed.map(|q| q.to_string()).unwrap_or_else(|| "-".into()),
            m.cost_per_unit.map(|q| q.to_string()).unwrap_or_else(|| "-".into()),
            m.exports.join(", ")
        );
    }
    if !def.compliance_tags.is_empty() {
        println!("\ncompliance tags: {}", def.compliance_tags.join(", "));
    }

    if !build {
        return ExitCode::SUCCESS;
    }
    let k = kernel();
    let built = match wmds_geom::build_primitive(&k, &r) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("geometry error: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("\nbuilt geometry with {KERNEL_NAME}");
    let density = r.material.as_deref().and_then(wmds_geom::placeholder_density);
    for (level, solid) in &built.levels {
        let Ok(mp) = k.mass_props(solid) else { continue };
        let bounds = k
            .tessellate(solid, 1e-3)
            .ok()
            .and_then(|m| m.bounds())
            .map(|(lo, hi)| format!("{:.1} x {:.1} x {:.1} mm", (hi[0] - lo[0]) * 1e3, (hi[1] - lo[1]) * 1e3, (hi[2] - lo[2]) * 1e3))
            .unwrap_or_default();
        println!(
            "  {:<12} volume {:>9.1} cm3   centroid ({:.1}, {:.1}, {:.1}) mm   bounds {}",
            level,
            mp.volume * 1e6,
            mp.centroid[0] * 1e3,
            mp.centroid[1] * 1e3,
            mp.centroid[2] * 1e3,
            bounds
        );
        if let Some(d) = density {
            println!("  {:<12} mass {:.3} kg at {d} kg/m3 (placeholder density)", "", mp.volume * d);
        }
    }
    export(&k, &built, step, stl)
}

fn export<K: GeomKernel>(k: &K, built: &wmds_geom::BuiltGeometry<K::Solid>, step: Option<&Path>, stl: Option<&Path>) -> ExitCode {
    let Some((level, solid)) = built.best() else {
        eprintln!("no geometry level was built");
        return ExitCode::FAILURE;
    };
    for (path, what) in [(step, "STEP"), (stl, "STL")] {
        let Some(p) = path else { continue };
        let r = if what == "STEP" { k.write_step(solid, p) } else { k.write_stl(solid, p) };
        match r {
            Ok(()) => println!("  wrote {what} ({level} level) to {}", p.display()),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------------- chassis

#[allow(clippy::too_many_arguments)]
pub fn show_chassis(
    project: &Path,
    system: &str,
    config: &str,
    width: &str,
    rail: Option<&str>,
    sections: &[String],
    build: bool,
    step: Option<&Path>,
    stl: Option<&Path>,
) -> ExitCode {
    let lib = match Library::load(project) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    for (f, e) in &lib.failures {
        eprintln!("warning: {}: {e}", f.display());
    }
    let Some(def) = lib.chassis.get(system) else {
        eprintln!("unknown chassis system `{system}`. Known: {}", lib.chassis.keys().cloned().collect::<Vec<_>>().join(", "));
        return ExitCode::FAILURE;
    };
    let rail_section = rail.map(|s| s.to_string()).unwrap_or_else(|| def.rail_sections.keys().next().cloned().unwrap_or_default());

    // Section lengths: use what the caller gave, otherwise the middle of each allowed range
    // rounded to the grid, so the command is useful with no options at all.
    let mut section_lengths: IndexMap<String, wmds_expr::Expr> = IndexMap::new();
    let wanted = def.configuration(config).cloned().unwrap_or_default();
    for kind in &wanted {
        let sk = &def.section_kinds[kind];
        let mid = (sk.length_min.value + sk.length_max.value) / 2.0;
        let pitch = def.grid_pitch.value;
        let snapped = (mid / pitch).round() * pitch;
        section_lengths.insert(kind.clone(), wmds_expr::Expr::Num(Quantity::new(snapped, Dim::LENGTH)));
    }
    for s in sections {
        let Some((kind, len)) = s.split_once('=') else {
            eprintln!("--section must be KIND=LENGTH, e.g. --section front=1100mm");
            return ExitCode::FAILURE;
        };
        match Quantity::parse(len) {
            Ok(q) if q.dim == Dim::LENGTH => {
                section_lengths.insert(kind.trim().to_string(), wmds_expr::Expr::Num(q));
            }
            _ => {
                eprintln!("--section {kind}: `{len}` is not a length");
                return ExitCode::FAILURE;
            }
        }
    }

    let req = ChassisRef {
        system: system.to_string(),
        configuration: config.to_string(),
        width: width.to_string(),
        rail_section,
        section_lengths,
        id: "chassis".to_string(),
    };
    let chassis = match wmds_model::generate_chassis(&lib, &req) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    print_chassis(&chassis, &req, true);
    for w in &chassis.assembly.warnings {
        println!("  warning: {w}");
    }
    if !build {
        return ExitCode::SUCCESS;
    }
    build_and_report(&chassis.assembly, step, stl)
}

fn print_chassis(chassis: &wmds_model::GeneratedChassis, req: &ChassisRef, list_parts: bool) {
    println!("{}", chassis.assembly.id);
    println!("  configuration {}  width {}  rail {}", req.configuration, req.width, req.rail_section);
    println!("  overall length {:.0} mm, width across rails {:.0} mm", mm(chassis.length), mm(chassis.width));
    println!("  grid pitch {:.0} mm, stations {} to {}", mm(chassis.grid_pitch), chassis.station_range.0, chassis.station_range.1);
    println!("\nsections");
    for (kind, x0, x1, len) in wmds_model::chassis::describe_sections(chassis) {
        println!("  {kind:<8} x {:>7.0} to {:>7.0} mm   length {:>6.0} mm", mm(x0), mm(x1), mm(len));
    }
    if list_parts {
        println!("\nparts ({})", chassis.assembly.instances.len());
        for i in &chassis.assembly.instances {
            let t = i.placement.translation;
            println!("  {:<26} {:<32} at ({:>7.0}, {:>7.0}, {:>7.0}) mm", i.id, i.source_id, t[0] * 1e3, t[1] * 1e3, t[2] * 1e3);
        }
    } else {
        println!("  {} chassis parts, included in the list below", chassis.assembly.instances.len());
    }
    println!("\nmount points: {} grid stations and section joints exported", chassis.assembly.exports.len());
}

// --------------------------------------------------------------------------------- vehicles

pub fn show_vehicle(project: &Path, file: &Path, build: bool, step: Option<&Path>, stl: Option<&Path>) -> ExitCode {
    let lib = match Library::load(project) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    for (f, e) in &lib.failures {
        eprintln!("warning: {}: {e}", f.display());
    }
    let def = match Library::load_assembly_file(file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Generate the chassis first, if the vehicle asks for one, and hand it to the resolver as a
    // unit the vehicle's own mates can attach to.
    let mut extra = Vec::new();
    let mut generated = None;
    if let Some(req) = &def.chassis {
        match wmds_model::generate_chassis(&lib, req) {
            Ok(g) => {
                extra.push((req.id.clone(), g.assembly.clone()));
                generated = Some(g);
            }
            Err(e) => {
                eprintln!("error: chassis: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let asm = match wmds_model::resolve_assembly(&lib, &def, &Overrides::default(), extra) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!("{} v{}", asm.id, asm.version);
    if !def.description.is_empty() {
        println!("  {}", def.description);
    }
    if let Some(v) = &def.vehicle {
        println!("  category {}  markets {}", v.category, if v.markets.is_empty() { "-".into() } else { v.markets.join(", ") });
        if !v.rule_packs.is_empty() {
            println!("  rule packs {}", v.rule_packs.join(", "));
        }
    }
    if let (Some(g), Some(req)) = (&generated, &def.chassis) {
        println!();
        print_chassis(g, req, false);
    }

    println!("\nparts ({})", asm.instances.len());
    for i in &asm.instances {
        let t = i.placement.translation;
        let how = match &i.placed_by {
            PlacedBy::Root => "root".to_string(),
            PlacedBy::Mate(m) => format!("mate {m}"),
            PlacedBy::Free(why) => format!("placed: {why}"),
            PlacedBy::Unreached => "NOT PLACED".to_string(),
        };
        println!("  {:<28} {:<30} ({:>7.0}, {:>7.0}, {:>7.0}) mm  {}", i.id, i.source_id, t[0] * 1e3, t[1] * 1e3, t[2] * 1e3, how);
    }

    if !asm.mates.is_empty() {
        println!("\nmates ({})", asm.mates.len());
        for m in &asm.mates {
            let status = match &m.compatible {
                Ok(()) => "ok".to_string(),
                Err(e) => format!("FAIL: {e}"),
            };
            let f = m
                .fasteners
                .as_ref()
                .map(|f| format!("  {}x {} {} {}", f.quantity, f.kind, f.size, f.grade))
                .unwrap_or_default();
            println!("  {:<22} {}.{} <-> {}.{}  {} {}{}", m.id, m.a, m.a_port, m.b, m.b_port, m.dof.name(), m.stage.name(), f);
            if status != "ok" {
                println!("  {:<22} {status}", "");
            }
        }
    }

    if !asm.point_masses.is_empty() {
        println!("\npoint masses");
        for p in &asm.point_masses {
            println!("  {:<20} {:>8} at ({:.0}, {:.0}, {:.0}) mm  [{}]", p.id, p.mass.to_string(), mm(p.at[0]), mm(p.at[1]), mm(p.at[2]), p.state);
        }
    }

    for w in &asm.warnings {
        println!("\nwarning: {w}");
    }
    for e in &asm.errors {
        println!("\nERROR: {e}");
    }

    if !build {
        return if asm.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    let code = build_and_report(&asm, step, stl);
    if asm.is_ok() { code } else { ExitCode::FAILURE }
}

fn build_and_report(asm: &ResolvedAssembly, step: Option<&Path>, stl: Option<&Path>) -> ExitCode {
    let k = kernel();
    println!("\nbuilding geometry with {KERNEL_NAME}");
    let built = wmds_geom::build_assembly(&k, asm);
    for (id, e) in &built.failures {
        println!("  FAIL {id}: {e}");
    }
    let masses = wmds_geom::assembly_masses(&k, &built);
    let (total, cg, unknown) = wmds_geom::roll_up(&masses);

    println!("\nmass by part");
    let mut by_source: IndexMap<String, (usize, f64, bool)> = IndexMap::new();
    for m in &masses {
        let part = built.parts.iter().find(|p| p.id == m.id);
        let source = part.map(|p| p.source_id.clone()).unwrap_or_default();
        let e = by_source.entry(source).or_insert((0, 0.0, false));
        e.0 += 1;
        e.1 += m.mass.unwrap_or(0.0);
        e.2 |= m.from_declaration;
    }
    for (source, (count, mass, declared)) in &by_source {
        let how = if *declared { "declared" } else { "from geometry" };
        println!("  {:<40} x{:<4} {:>8.2} kg   {}", source, count, mass, how);
    }
    println!("\ntotal mass {total:.1} kg at cg ({:.0}, {:.0}, {:.0}) mm", cg[0] * 1e3, cg[1] * 1e3, cg[2] * 1e3);
    if unknown > 0 {
        println!("  {unknown} part(s) have no density yet and are not counted");
    }
    println!("  masses marked \"from geometry\" use placeholder densities; the material database is not built yet");

    let mut point_total = 0.0;
    for p in &asm.point_masses {
        point_total += p.mass.value;
    }
    if point_total > 0.0 {
        println!("  plus {point_total:.1} kg of declared point masses");
    }

    // Export the whole assembly as one solid.
    if step.is_some() || stl.is_some() {
        let mut fused: Option<<Kernel as GeomKernel>::Solid> = None;
        for p in &built.parts {
            fused = Some(match fused {
                None => p.solid.clone(),
                Some(acc) => match k.union(&acc, &p.solid) {
                    Ok(u) => u,
                    Err(e) => {
                        eprintln!("  could not fuse {}: {e}", p.id);
                        acc
                    }
                },
            });
        }
        let Some(solid) = fused else {
            eprintln!("nothing to export");
            return ExitCode::FAILURE;
        };
        for (path, what) in [(step, "STEP"), (stl, "STL")] {
            let Some(p) = path else { continue };
            let r = if what == "STEP" { k.write_step(&solid, p) } else { k.write_stl(&solid, p) };
            match r {
                Ok(()) => println!("  wrote {what} to {}", p.display()),
                Err(e) => {
                    eprintln!("  {what}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    ExitCode::SUCCESS
}
