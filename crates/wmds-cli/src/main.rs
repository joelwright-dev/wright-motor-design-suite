//! `wmds` command line.
//!
//! ```text
//! wmds lib validate <file-or-dir>...        parse and resolve primitives, report problems
//! wmds lib show <file> [--set k=v]...       print resolved params, ports, geometry, costs
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wmds_model::{Overrides, ResolvedMassProps, resolve};

#[derive(Parser)]
#[command(name = "wmds", version, about = "Wright Motor Design Suite")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Primitive library commands
    Lib {
        #[command(subcommand)]
        cmd: LibCmd,
    },
}

#[derive(Subcommand)]
enum LibCmd {
    /// Parse and resolve every `*.prim.kdl` under the given paths
    Validate { paths: Vec<PathBuf> },
    /// Resolve one primitive and print its parameters, ports, geometry and costs
    Show {
        file: PathBuf,
        /// Parameter or variant override, e.g. `--set span=400mm --set hand=right`
        #[arg(long = "set", value_name = "NAME=VALUE")]
        sets: Vec<String>,
    },
}

fn main() -> ExitCode {
    miette::set_hook(Box::new(|_| {
        Box::new(miette::MietteHandlerOpts::new().unicode(false).build())
    }))
    .ok();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Lib {
            cmd: LibCmd::Validate { paths },
        } => validate(&paths),
        Cmd::Lib {
            cmd: LibCmd::Show { file, sets },
        } => show(&file, &sets),
    }
}

fn collect_prim_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
                entries.sort();
                for e in entries {
                    walk(&e, out);
                }
            }
        } else if p.to_string_lossy().ends_with(".prim.kdl") {
            out.push(p.to_path_buf());
        }
    }
    for p in paths {
        walk(p, &mut out);
    }
    out
}

fn load(file: &Path) -> Result<wmds_schema::PrimitiveDef, miette::Report> {
    let src = std::fs::read_to_string(file)
        .map_err(|e| miette::miette!("cannot read {}: {e}", file.display()))?;
    let name = file
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    wmds_schema::parse_primitive(&name, &src).map_err(miette::Report::new)
}

fn validate(paths: &[PathBuf]) -> ExitCode {
    let paths: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from("library")]
    } else {
        paths.to_vec()
    };
    let files = collect_prim_files(&paths);
    if files.is_empty() {
        eprintln!("no .prim.kdl files found under {:?}", paths);
        return ExitCode::from(2);
    }
    let mut failures = 0;
    for f in &files {
        match load(f).and_then(|def| {
            resolve(&def, &Overrides::default())
                .map(|r| (def, r))
                .map_err(|e| miette::miette!("{e}"))
        }) {
            Ok((def, r)) => {
                println!(
                    "ok    {}  ({} v{}, {} params, {} ports)",
                    f.display(),
                    def.id,
                    def.version,
                    r.params.len(),
                    r.ports.len()
                );
            }
            Err(e) => {
                failures += 1;
                println!("FAIL  {}", f.display());
                eprintln!("{e:?}");
            }
        }
    }
    println!("{} file(s), {} failure(s)", files.len(), failures);
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn show(file: &Path, sets: &[String]) -> ExitCode {
    let def = match load(file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e:?}");
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
        let doc = p.doc.clone().unwrap_or_default();
        println!("  {:<14} {:<16} {:<8} {}", p.name, v.to_string(), kind, doc);
    }
    println!("\nports");
    for p in &r.ports {
        let o = p.origin;
        println!(
            "  {:<14} {:<22} at ({}, {}, {})  axis ({:.2}, {:.2}, {:.2}){}",
            p.name,
            p.port_type,
            o[0],
            o[1],
            o[2],
            p.axis[0],
            p.axis[1],
            p.axis[2],
            p.load_rating
                .map(|q| format!("  rating {q}"))
                .unwrap_or_default()
        );
        if !p.params.is_empty() {
            let ps: Vec<String> = p.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
            println!("  {:<14} {}", "", ps.join(" "));
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
            println!("\nmassprops: computed from geometry (kernel not yet available in this build)")
        }
        ResolvedMassProps::Declared { mass, cg, .. } => {
            println!(
                "\nmassprops: declared mass {mass}{}",
                cg.map(|c| format!(", cg ({}, {}, {})", c[0], c[1], c[2]))
                    .unwrap_or_default()
            );
        }
    }
    println!("\nmanufacturing");
    for m in &r.manufacturing {
        println!(
            "  {:<22} scale {:<10} fixed {:<14} per unit {:<14} exports {}",
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
    ExitCode::SUCCESS
}
