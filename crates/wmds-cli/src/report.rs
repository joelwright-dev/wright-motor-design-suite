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

pub fn show_primitive(
    file: &Path,
    sets: &[String],
    build: bool,
    step: Option<&Path>,
    stl: Option<&Path>,
) -> ExitCode {
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", file.display());
            return ExitCode::FAILURE;
        }
    };
    let name = file
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
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
    println!(
        "  category: {}{}",
        def.category,
        def.sub
            .as_ref()
            .map(|s| format!(" / {s}"))
            .unwrap_or_default()
    );
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
        println!(
            "  {:<14} {:<18} {:<8} {}",
            p.name,
            v.to_string(),
            kind,
            p.doc.clone().unwrap_or_default()
        );
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
            p.load_rating
                .map(|q| format!("  rating {q}"))
                .unwrap_or_default()
        );
        if !p.params.is_empty() {
            println!(
                "  {:<14} {}",
                "",
                p.params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
    println!("\ngeometry");
    for lvl in &r.geometry {
        println!("  level {}", lvl.level);
        for f in &lvl.features {
            let args: Vec<String> = f.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            println!(
                "    {:<10} {:<12} {}",
                f.op,
                f.name.clone().unwrap_or_default(),
                args.join(" ")
            );
        }
    }
    match &r.massprops {
        ResolvedMassProps::Computed => {
            println!("\nmassprops: computed from geometry (pass --build to compute)")
        }
        ResolvedMassProps::Declared { mass, cg, .. } => println!(
            "\nmassprops: declared mass {mass}{}",
            cg.map(|c| format!(
                ", cg ({:.1}, {:.1}, {:.1}) mm",
                mm(c[0]),
                mm(c[1]),
                mm(c[2])
            ))
            .unwrap_or_default()
        ),
    }
    println!("\nmanufacturing");
    for m in &r.manufacturing {
        println!(
            "  {:<24} scale {:<10} fixed {:<12} per unit {:<14} exports {}",
            m.method,
            m.scale.clone().unwrap_or_default(),
            m.cost_fixed
                .map(|q| q.to_string())
                .unwrap_or_else(|| "-".into()),
            m.cost_per_unit
                .map(|q| q.to_string())
                .unwrap_or_else(|| "-".into()),
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
    let density = r
        .material
        .as_deref()
        .and_then(wmds_geom::placeholder_density);
    for (level, solid) in &built.levels {
        let Ok(mp) = k.mass_props(solid) else {
            continue;
        };
        let bounds = k
            .tessellate(solid, 1e-3)
            .ok()
            .and_then(|m| m.bounds())
            .map(|(lo, hi)| {
                format!(
                    "{:.1} x {:.1} x {:.1} mm",
                    (hi[0] - lo[0]) * 1e3,
                    (hi[1] - lo[1]) * 1e3,
                    (hi[2] - lo[2]) * 1e3
                )
            })
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
            println!(
                "  {:<12} mass {:.3} kg at {d} kg/m3 (placeholder density)",
                "",
                mp.volume * d
            );
        }
    }
    export(&k, &built, step, stl)
}

fn export<K: GeomKernel>(
    k: &K,
    built: &wmds_geom::BuiltGeometry<K::Solid>,
    step: Option<&Path>,
    stl: Option<&Path>,
) -> ExitCode {
    let Some((level, solid)) = built.best() else {
        eprintln!("no geometry level was built");
        return ExitCode::FAILURE;
    };
    for (path, what) in [(step, "STEP"), (stl, "STL")] {
        let Some(p) = path else { continue };
        let r = if what == "STEP" {
            k.write_step(solid, p)
        } else {
            k.write_stl(solid, p)
        };
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
        eprintln!(
            "unknown chassis system `{system}`. Known: {}",
            lib.chassis.keys().cloned().collect::<Vec<_>>().join(", ")
        );
        return ExitCode::FAILURE;
    };
    let rail_section = rail
        .map(|s| s.to_string())
        .unwrap_or_else(|| def.rail_sections.keys().next().cloned().unwrap_or_default());

    // Section lengths: use what the caller gave, otherwise the middle of each allowed range
    // rounded to the grid, so the command is useful with no options at all.
    let mut section_lengths: IndexMap<String, wmds_expr::Expr> = IndexMap::new();
    let wanted = def.configuration(config).cloned().unwrap_or_default();
    for kind in &wanted {
        let sk = &def.section_kinds[kind];
        let mid = (sk.length_min.value + sk.length_max.value) / 2.0;
        let pitch = def.grid_pitch.value;
        let snapped = (mid / pitch).round() * pitch;
        section_lengths.insert(
            kind.clone(),
            wmds_expr::Expr::Num(Quantity::new(snapped, Dim::LENGTH)),
        );
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
    build_and_report(&chassis.assembly, Some(&lib), step, stl, false)
}

fn print_chassis(chassis: &wmds_model::GeneratedChassis, req: &ChassisRef, list_parts: bool) {
    println!("{}", chassis.assembly.id);
    println!(
        "  configuration {}  width {}  rail {}",
        req.configuration, req.width, req.rail_section
    );
    println!(
        "  overall length {:.0} mm, width across rails {:.0} mm",
        mm(chassis.length),
        mm(chassis.width)
    );
    println!(
        "  grid pitch {:.0} mm, stations {} to {}",
        mm(chassis.grid_pitch),
        chassis.station_range.0,
        chassis.station_range.1
    );
    println!("\nsections");
    for (kind, x0, x1, len) in wmds_model::chassis::describe_sections(chassis) {
        println!(
            "  {kind:<8} x {:>7.0} to {:>7.0} mm   length {:>6.0} mm",
            mm(x0),
            mm(x1),
            mm(len)
        );
    }
    if list_parts {
        println!("\nparts ({})", chassis.assembly.instances.len());
        for i in &chassis.assembly.instances {
            let t = i.placement.translation;
            println!(
                "  {:<26} {:<32} at ({:>7.0}, {:>7.0}, {:>7.0}) mm",
                i.id,
                i.source_id,
                t[0] * 1e3,
                t[1] * 1e3,
                t[2] * 1e3
            );
        }
    } else {
        println!(
            "  {} chassis parts, included in the list below",
            chassis.assembly.instances.len()
        );
    }
    println!(
        "\nmount points: {} grid stations and section joints exported",
        chassis.assembly.exports.len()
    );
}

// --------------------------------------------------------------------------------- vehicles

pub fn show_vehicle(
    project: &Path,
    file: &Path,
    build: bool,
    step: Option<&Path>,
    stl: Option<&Path>,
    bounds: bool,
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
        println!(
            "  category {}  markets {}",
            v.category,
            if v.markets.is_empty() {
                "-".into()
            } else {
                v.markets.join(", ")
            }
        );
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
        println!(
            "  {:<28} {:<30} ({:>7.0}, {:>7.0}, {:>7.0}) mm  {}",
            i.id,
            i.source_id,
            t[0] * 1e3,
            t[1] * 1e3,
            t[2] * 1e3,
            how
        );
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
            println!(
                "  {:<22} {}.{} <-> {}.{}  {} {}{}",
                m.id,
                m.a,
                m.a_port,
                m.b,
                m.b_port,
                m.dof.name(),
                m.stage.name(),
                f
            );
            if status != "ok" {
                println!("  {:<22} {status}", "");
            }
        }
    }

    if !asm.point_masses.is_empty() {
        println!("\npoint masses");
        for p in &asm.point_masses {
            println!(
                "  {:<20} {:>8} at ({:.0}, {:.0}, {:.0}) mm  [{}]",
                p.id,
                p.mass.to_string(),
                mm(p.at[0]),
                mm(p.at[1]),
                mm(p.at[2]),
                p.state
            );
        }
    }

    for w in &asm.warnings {
        println!("\nwarning: {w}");
    }
    for e in &asm.errors {
        println!("\nERROR: {e}");
    }

    if !build {
        return if asm.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let code = build_and_report(&asm, Some(&lib), step, stl, bounds);
    if asm.is_ok() { code } else { ExitCode::FAILURE }
}

fn build_and_report(
    asm: &ResolvedAssembly,
    lib: Option<&Library>,
    step: Option<&Path>,
    stl: Option<&Path>,
    bounds: bool,
) -> ExitCode {
    let k = kernel();
    println!("\nbuilding geometry with {KERNEL_NAME}");
    let built = wmds_geom::build_assembly(&k, asm);
    for (id, e) in &built.failures {
        println!("  FAIL {id}: {e}");
    }
    if bounds {
        print_bounds(&k, &built);
    }
    let density_of = |m: &str| lib.and_then(|l| l.density(m));
    let masses = wmds_geom::assembly_masses(&k, &built, &density_of);
    let (total, cg, unknown) = wmds_geom::roll_up(&masses);

    println!("\nmass by part");
    let mut by_source: IndexMap<String, (usize, f64, wmds_geom::DensitySource)> = IndexMap::new();
    for m in &masses {
        let part = built.parts.iter().find(|p| p.id == m.id);
        let source = part.map(|p| p.source_id.clone()).unwrap_or_default();
        let e = by_source
            .entry(source)
            .or_insert((0, 0.0, wmds_geom::DensitySource::None));
        e.0 += 1;
        e.1 += m.mass.unwrap_or(0.0);
        e.2 = m.density_source;
    }
    for (source, (count, mass, how)) in &by_source {
        println!(
            "  {:<40} x{:<4} {:>8.2} kg   {}",
            source,
            count,
            mass,
            how.label()
        );
    }
    println!(
        "\ntotal mass {total:.1} kg at cg ({:.0}, {:.0}, {:.0}) mm",
        cg[0] * 1e3,
        cg[1] * 1e3,
        cg[2] * 1e3
    );
    if unknown > 0 {
        println!("  {unknown} part(s) have no density and are not counted");
    }
    // Say plainly which numbers rest on a guess rather than on the material database.
    let mut guessed: Vec<&str> = masses
        .iter()
        .filter(|m| m.density_source == wmds_geom::DensitySource::Placeholder)
        .filter_map(|m| m.material.as_deref())
        .collect();
    guessed.sort_unstable();
    guessed.dedup();
    if guessed.is_empty() {
        println!("  every density came from the material database");
    } else {
        println!(
            "  densities guessed from the name for: {}",
            guessed.join(", ")
        );
        println!("  add those to materials/ to stop guessing");
    }

    let mut point_total = 0.0;
    for p in &asm.point_masses {
        point_total += p.mass.value;
    }
    if point_total > 0.0 {
        println!("  plus {point_total:.1} kg of declared point masses");
    }

    print_tier0(&tier0_of(&k, asm, &built, &masses));

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
            let r = if what == "STEP" {
                k.write_step(&solid, p)
            } else {
                k.write_stl(&solid, p)
            };
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

// ------------------------------------------------------------------------------- analytics

/// Everything Tier 0 needs, gathered from a built assembly.
fn tier0_of<K: GeomKernel>(
    k: &K,
    asm: &ResolvedAssembly,
    built: &wmds_geom::BuiltAssembly<K::Solid>,
    masses: &[wmds_geom::PartMass],
) -> wmds_analytics::Tier0 {
    let mut points: Vec<wmds_analytics::MassPoint> = masses
        .iter()
        .filter_map(|m| {
            m.mass.map(|kg| wmds_analytics::MassPoint {
                mass: kg,
                at: m.centroid,
            })
        })
        .collect();
    // Kerb point masses belong in the axle loads; the laden ones are a separate load case and
    // are left out so "kerb" means kerb.
    for p in asm.point_masses.iter().filter(|p| p.state == "kerb") {
        points.push(wmds_analytics::MassPoint {
            mass: p.mass.value,
            at: [p.at[0].value, p.at[1].value, p.at[2].value],
        });
    }

    let rolling = wmds_analytics::rolling_stock(asm);
    let whole = wmds_geom::assembly_mesh(k, built, 2e-3);
    let bounds = whole.bounds();

    // Ground clearance means the lowest thing that is not a wheel, so the tyres are excluded.
    let mut structure_low: Option<f64> = None;
    for p in &built.parts {
        if p.source_id.starts_with("wheels/") {
            continue;
        }
        if let Ok(m) = k.tessellate(&p.solid, 2e-3) {
            if let Some((lo, _)) = m.bounds() {
                structure_low = Some(structure_low.map_or(lo[2], |z: f64| z.min(lo[2])));
            }
        }
    }
    wmds_analytics::compute(&points, &rolling, bounds, structure_low)
}

fn print_tier0(t: &wmds_analytics::Tier0) {
    println!(
        "
Tier 0 (closed form, from mass and geometry)"
    );
    if let Some(o) = t.overall() {
        println!(
            "  overall            {:.0} x {:.0} x {:.0} mm",
            o[0] * 1e3,
            o[1] * 1e3,
            o[2] * 1e3
        );
    }
    if let Some(wb) = t.wheelbase {
        println!("  wheelbase          {:.0} mm", wb * 1e3);
    }
    for (i, a) in t.axles.iter().enumerate() {
        let name = match (i, t.axles.len()) {
            (0, 2) => "front".to_string(),
            (1, 2) => "rear".to_string(),
            (n, _) => format!("axle {}", n + 1),
        };
        println!(
            "  {name:<18} x {:>7.0} mm   track {:>6.0} mm   {} wheels   {:>7.1} kg  ({:.0}%)",
            a.x * 1e3,
            a.track * 1e3,
            a.wheel_count,
            a.load,
            a.load_fraction * 100.0
        );
    }
    if let (Some(f), Some(r)) = (t.front_overhang, t.rear_overhang) {
        println!(
            "  overhangs          front {:.0} mm, rear {:.0} mm",
            f * 1e3,
            r * 1e3
        );
    }
    if let Some(h) = t.cg_height {
        println!("  cg height          {:.0} mm above the ground", h * 1e3);
    }
    if let Some(c) = t.ground_clearance {
        println!("  ground clearance   {:.0} mm", c * 1e3);
    }
    if let Some(s) = t.static_stability_factor {
        println!(
            "  static stability   {s:.2}   (half track over cg height; higher resists rollover)"
        );
    }
    for n in &t.notes {
        println!("  note: {n}");
    }
}

// -------------------------------------------------------------------------------- compliance

/// Resolve a vehicle, build it, and check it against the rule packs it names.
pub fn check_vehicle(project: &Path, file: &Path, json: Option<&Path>, show_all: bool) -> ExitCode {
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

    // Facts that need geometry: part masses and the overall centre of gravity.
    let k = kernel();
    let built = wmds_geom::build_assembly(&k, &asm);
    let density_of = |m: &str| lib.density(m);
    let masses = wmds_geom::assembly_masses(&k, &built, &density_of);
    let mut facts = wmds_rules::Facts::from_assembly(&asm, &def);
    for p in &mut facts.parts {
        if let Some(m) = masses.iter().find(|m| m.id == p.id) {
            p.mass = m.mass.unwrap_or(0.0);
        }
    }
    let (modelled, _, _) = wmds_geom::roll_up(&masses);
    facts.modelled_mass = modelled;

    // The centre of gravity that matters is the whole kerb vehicle, so fold in the point masses
    // that belong to the kerb state. Leaving them out would flatter the cg height.
    let mut total = 0.0;
    let mut moment = [0.0f64; 3];
    for m in &masses {
        if let Some(mass) = m.mass {
            total += mass;
            for i in 0..3 {
                moment[i] += mass * m.centroid[i];
            }
        }
    }
    for p in asm.point_masses.iter().filter(|p| p.state == "kerb") {
        total += p.mass.value;
        for i in 0..3 {
            moment[i] += p.mass.value * p.at[i].value;
        }
    }
    if total > 0.0 {
        facts.cg = [moment[0] / total, moment[1] / total, moment[2] / total];
    }
    facts.bounds = wmds_geom::assembly_mesh(&k, &built, 1e-3).bounds();

    let t = tier0_of(&k, &asm, &built, &masses);
    facts.tier0 = Some(wmds_rules::Tier0Facts {
        wheelbase: t.wheelbase,
        front_track: t.front_axle().map(|a| a.track),
        rear_track: t.rear_axle().map(|a| a.track),
        front_axle_load: t.front_axle().map(|a| a.load),
        rear_axle_load: t.rear_axle().map(|a| a.load),
        front_fraction: t.front_axle().map(|a| a.load_fraction),
        cg_height: t.cg_height,
        static_stability_factor: t.static_stability_factor,
        ground_clearance: t.ground_clearance,
        front_overhang: t.front_overhang,
        rear_overhang: t.rear_overhang,
    });

    if let (Some(g), Some(req)) = (&generated, &def.chassis) {
        let prefix = format!("{}.", req.id);
        let chassis_mass: f64 = masses
            .iter()
            .filter(|m| m.id.starts_with(&prefix))
            .filter_map(|m| m.mass)
            .sum();
        facts.chassis = Some(wmds_rules::ChassisFacts {
            system: req.system.clone(),
            configuration: req.configuration.clone(),
            width_config: req.width.clone(),
            rail_section: req.rail_section.clone(),
            length: g.length.value,
            width: g.width.value,
            grid_pitch: g.grid_pitch.value,
            mass: chassis_mass,
            section_count: g.sections.len(),
        });
    }

    let (packs, failures) = wmds_rules::load_packs(&project.join("rules"));
    for (p, e) in &failures {
        eprintln!("warning: rule pack {p}: {e}");
    }
    let selected: Vec<String> = def
        .vehicle
        .as_ref()
        .map(|v| v.rule_packs.clone())
        .unwrap_or_default();
    if selected.is_empty() {
        println!("This vehicle names no rule packs, so nothing was checked.");
        println!("Add `rule_packs \"wright-internal\"` to the vehicle to turn checking on.");
        println!(
            "Packs available: {}",
            packs
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(2);
    }
    let mut missing = Vec::new();
    for want in &selected {
        if !packs.iter().any(|p| &p.id == want) {
            missing.push(want.clone());
        }
    }
    if !missing.is_empty() {
        // A pack the vehicle asked for and did not get is a hole in the checking, not a detail.
        // Reporting "all clear" while silently skipping a rule pack would be the worst thing
        // this command could do.
        eprintln!("error: rule pack(s) not found: {}", missing.join(", "));
        eprintln!("       the checks in them were not run, so this vehicle is not checked");
        if !failures.is_empty() {
            eprintln!("       one or more packs failed to parse; see the warnings above");
        }
        return ExitCode::FAILURE;
    }

    let report = wmds_rules::evaluate(&packs, &selected, &facts);
    let mut text = wmds_rules::render_text(&report);
    if !show_all {
        text.push_str("\nPass --all to list the rules that do not apply.\n");
    }
    print!("{text}");

    // Anything the resolver itself rejected is a defect in the model rather than in the design,
    // and it would make the compliance result meaningless, so say so loudly.
    if !asm.errors.is_empty() {
        println!(
            "\nThe model itself has {} error(s); fix these before trusting the report above:",
            asm.errors.len()
        );
        for e in &asm.errors {
            println!("  {e}");
        }
    }

    if let Some(p) = json {
        match std::fs::write(p, wmds_rules::render_json(&report)) {
            Ok(()) => println!("\nwrote {}", p.display()),
            Err(e) => eprintln!("could not write {}: {e}", p.display()),
        }
    }

    if report.is_clear() && asm.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Print the extent every part occupies in vehicle coordinates.
///
/// This is the report that settles an argument about a part looking wrong. A handed part drawn
/// the wrong way round has the right origin and the wrong extent, so a position table shows
/// nothing and this shows it immediately. Parts whose names differ only by a left or right
/// marker are paired up and their y extents compared, because that is nearly always the
/// question being asked.
fn print_bounds<K: wmds_geom::GeomKernel>(k: &K, built: &wmds_geom::BuiltAssembly<K::Solid>) {
    use std::collections::BTreeMap;

    let mut extents: BTreeMap<String, ([f64; 3], [f64; 3])> = BTreeMap::new();
    for p in &built.parts {
        if let Ok(m) = k.tessellate(&p.solid, 2e-3)
            && let Some((lo, hi)) = m.bounds()
        {
            extents.insert(p.id.clone(), (lo, hi));
        }
    }

    println!("\nextent of each part, in vehicle coordinates (mm)");
    println!(
        "  {:<34} {:>26} {:>26} {:>26}",
        "part", "x from / to", "y from / to", "z from / to"
    );
    for (id, (lo, hi)) in &extents {
        println!(
            "  {:<34} {:>12.0} {:>12.0} {:>12.0} {:>12.0} {:>12.0} {:>12.0}",
            id,
            lo[0] * 1e3,
            hi[0] * 1e3,
            lo[1] * 1e3,
            hi[1] * 1e3,
            lo[2] * 1e3,
            hi[2] * 1e3
        );
    }

    // Pair left with right and say whether each pair is a mirror image.
    println!("\nleft and right pairs");
    let mut any = false;
    for (id, (lo, hi)) in &extents {
        let Some(right) = mirror_name(id) else { continue };
        let Some((rlo, rhi)) = extents.get(&right) else {
            continue;
        };
        any = true;
        let mirrored = (lo[1] + rhi[1]).abs() < 1e-4 && (hi[1] + rlo[1]).abs() < 1e-4;
        println!(
            "  {:<34} y {:>7.0} to {:>7.0}   {:<34} y {:>7.0} to {:>7.0}   {}",
            id,
            lo[1] * 1e3,
            hi[1] * 1e3,
            right,
            rlo[1] * 1e3,
            rhi[1] * 1e3,
            if mirrored { "mirrored" } else { "NOT MIRRORED" }
        );
    }
    if !any {
        println!("  no parts whose names pair left with right");
    }
}

/// The right-hand name of a part whose name marks it as the left-hand one.
///
/// Purely a naming convention, and deliberately narrow: it matches the markers actually used in
/// this library rather than trying to be clever about every possible spelling.
fn mirror_name(id: &str) -> Option<String> {
    for (l, r) in [
        ("_fl", "_fr"),
        ("_rl", "_rr"),
        ("_left", "_right"),
        ("_l.", "_r."),
        ("_left.", "_right."),
    ] {
        if let Some(pos) = id.find(l) {
            let mut out = id.to_string();
            out.replace_range(pos..pos + l.len(), r);
            return Some(out);
        }
    }
    // A dotted sub-assembly id such as `corner_fl.lower_arm` is handled above by `_fl`.
    None
}

// ------------------------------------------------------------------------------ build pack

/// Produce everything needed to make and assemble one vehicle.
pub fn build_pack(
    project: &Path,
    file: &Path,
    volume: u32,
    markdown: Option<&Path>,
) -> ExitCode {
    let lib = match Library::load(project) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let def = match Library::load_assembly_file(file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut extra = Vec::new();
    if let Some(req) = &def.chassis {
        match wmds_model::generate_chassis(&lib, req) {
            Ok(g) => extra.push((req.id.clone(), g.assembly)),
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

    let k = kernel();
    let built = wmds_geom::build_assembly(&k, &asm);
    let density_of = |m: &str| lib.density(m);
    let masses = wmds_geom::assembly_masses(&k, &built, &density_of);

    let pack = wmds_mfg::BuildPack::build(&asm, &lib, &masses, volume);
    print!("{}", wmds_mfg::write_text(&pack));

    if let Some(path) = markdown {
        let text = wmds_mfg::write_markdown(&pack);
        match std::fs::write(path, text) {
            Ok(()) => println!("\nwrote {}", path.display()),
            Err(e) => {
                eprintln!("error: cannot write {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }

    // A pack for a vehicle that does not resolve is not something to act on.
    if asm.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
