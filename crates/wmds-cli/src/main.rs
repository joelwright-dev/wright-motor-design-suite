//! `wmds` command line.
//!
//! ```text
//! wmds lib validate [paths...]              parse and resolve every definition file
//! wmds lib show <file> [--set k=v]...       one primitive: params, ports, geometry, costs
//! wmds chassis show <system> [options]      generate a chassis and describe it
//! wmds veh show <file.veh.kdl>              a whole vehicle: chassis, parts, mates, mass
//! ```

mod report;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wmds_model::{Library, Overrides, resolve};

#[derive(Parser)]
#[command(name = "wmds", version, about = "Wright Motor Design Suite")]
struct Cli {
    /// Project root holding `library/` and `chassis/`. Defaults to the working directory.
    #[arg(long, global = true, value_name = "DIR")]
    project: Option<PathBuf>,
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
    /// Chassis systems
    Chassis {
        #[command(subcommand)]
        cmd: ChassisCmd,
    },
    /// Vehicles and assemblies
    Veh {
        #[command(subcommand)]
        cmd: VehCmd,
    },
}

#[derive(Subcommand)]
enum LibCmd {
    /// Parse and resolve every definition file under the given paths
    Validate { paths: Vec<PathBuf> },
    /// Resolve one primitive and print its parameters, ports, geometry and costs
    Show {
        file: PathBuf,
        /// Parameter or variant override, e.g. `--set span=400mm --set hand=right`
        #[arg(long = "set", value_name = "NAME=VALUE")]
        sets: Vec<String>,
        /// Build the geometry and report volume, mass and bounds
        #[arg(long)]
        build: bool,
        /// Write the built geometry as STEP (implies --build)
        #[arg(long, value_name = "FILE")]
        step: Option<PathBuf>,
        /// Write the built geometry as binary STL (implies --build)
        #[arg(long, value_name = "FILE")]
        stl: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ChassisCmd {
    /// List the chassis systems in the project
    List,
    /// Generate a chassis and describe it
    Show {
        /// Chassis system id, e.g. `mcds-v1`
        system: String,
        #[arg(long, default_value = "full-length")]
        config: String,
        #[arg(long, default_value = "standard")]
        width: String,
        #[arg(long, value_name = "NAME")]
        rail: Option<String>,
        /// Section length, e.g. `--section front=1100mm`. Repeat for each section.
        #[arg(long = "section", value_name = "KIND=LENGTH")]
        sections: Vec<String>,
        /// Build the geometry and report mass
        #[arg(long)]
        build: bool,
        /// Write the whole chassis as STEP
        #[arg(long, value_name = "FILE")]
        step: Option<PathBuf>,
        /// Write the whole chassis as binary STL
        #[arg(long, value_name = "FILE")]
        stl: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum VehCmd {
    /// Resolve a vehicle or assembly file and report what it contains
    Show {
        file: PathBuf,
        #[arg(long)]
        build: bool,
        #[arg(long, value_name = "FILE")]
        step: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        stl: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    miette::set_hook(Box::new(|_| {
        Box::new(miette::MietteHandlerOpts::new().unicode(false).build())
    }))
    .ok();
    let cli = Cli::parse();
    let project = cli.project.clone().unwrap_or_else(|| PathBuf::from("."));
    match cli.cmd {
        Cmd::Lib { cmd: LibCmd::Validate { paths } } => validate(&project, &paths),
        Cmd::Lib { cmd: LibCmd::Show { file, sets, build, step, stl } } => {
            report::show_primitive(&file, &sets, build || step.is_some() || stl.is_some(), step.as_deref(), stl.as_deref())
        }
        Cmd::Chassis { cmd: ChassisCmd::List } => list_chassis(&project),
        Cmd::Chassis { cmd: ChassisCmd::Show { system, config, width, rail, sections, build, step, stl } } => {
            report::show_chassis(&project, &system, &config, &width, rail.as_deref(), &sections, build || step.is_some() || stl.is_some(), step.as_deref(), stl.as_deref())
        }
        Cmd::Veh { cmd: VehCmd::Show { file, build, step, stl } } => {
            report::show_vehicle(&project, &file, build || step.is_some() || stl.is_some(), step.as_deref(), stl.as_deref())
        }
    }
}

fn load_library(project: &Path) -> Result<Library, ExitCode> {
    match Library::load(project) {
        Ok(lib) => {
            for (f, e) in &lib.failures {
                eprintln!("warning: {}: {e}", f.display());
            }
            Ok(lib)
        }
        Err(e) => {
            eprintln!("error: {e}");
            Err(ExitCode::from(2))
        }
    }
}

fn list_chassis(project: &Path) -> ExitCode {
    let lib = match load_library(project) {
        Ok(l) => l,
        Err(c) => return c,
    };
    if lib.chassis.is_empty() {
        println!("no chassis systems found (looked in {}/chassis)", project.display());
        return ExitCode::from(2);
    }
    for (id, c) in &lib.chassis {
        println!("{id} v{}  {}", c.version, c.description);
        println!("  grid pitch {}", c.grid_pitch);
        println!("  widths: {}", c.width_configs.keys().cloned().collect::<Vec<_>>().join(", "));
        println!("  rail sections: {}", c.rail_sections.keys().cloned().collect::<Vec<_>>().join(", "));
        for (name, sections) in &c.configurations {
            println!("  configuration {name}: {}", sections.join(" + "));
        }
        for (name, k) in &c.section_kinds {
            println!("    {name:<8} {} to {}", k.length_min, k.length_max);
        }
    }
    ExitCode::SUCCESS
}

fn collect_files(paths: &[PathBuf], suffixes: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(p: &Path, out: &mut Vec<PathBuf>, suffixes: &[&str]) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
                entries.sort();
                for e in entries {
                    walk(&e, out, suffixes);
                }
            }
        } else {
            let name = p.to_string_lossy();
            if suffixes.iter().any(|s| name.ends_with(s)) {
                out.push(p.to_path_buf());
            }
        }
    }
    for p in paths {
        walk(p, &mut out, suffixes);
    }
    out
}

fn validate(project: &Path, paths: &[PathBuf]) -> ExitCode {
    let paths: Vec<PathBuf> = if paths.is_empty() {
        vec![project.join("library"), project.join("chassis")]
    } else {
        paths.to_vec()
    };
    let files = collect_files(&paths, &[".prim.kdl", ".asm.kdl", ".veh.kdl", ".chassis.kdl"]);
    if files.is_empty() {
        eprintln!("no definition files found under {paths:?}");
        return ExitCode::from(2);
    }
    let ports = project.join("library").join("ports.kdl");
    let mut failures = 0;
    if ports.is_file() {
        let src = std::fs::read_to_string(&ports).unwrap_or_default();
        match wmds_schema::parse_port_types("ports.kdl", &src) {
            Ok(p) => println!("ok    {}  ({} port types)", ports.display(), p.len()),
            Err(e) => {
                failures += 1;
                println!("FAIL  {}", ports.display());
                eprintln!("{:?}", miette::Report::new(e));
            }
        }
    }
    for f in &files {
        let name = f.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(e) => {
                failures += 1;
                println!("FAIL  {}  ({e})", f.display());
                continue;
            }
        };
        let outcome: Result<String, String> = if name.ends_with(".prim.kdl") {
            wmds_schema::parse_primitive(&name, &src)
                .map_err(|e| format!("{:?}", miette::Report::new(e)))
                .and_then(|d| {
                    resolve(&d, &Overrides::default())
                        .map(|r| format!("{} v{}, {} params, {} ports", d.id, d.version, r.params.len(), r.ports.len()))
                        .map_err(|e| e.to_string())
                })
        } else if name.ends_with(".chassis.kdl") {
            wmds_schema::parse_chassis(&name, &src)
                .map(|d| format!("{} v{}, {} configurations", d.id, d.version, d.configurations.len()))
                .map_err(|e| format!("{:?}", miette::Report::new(e)))
        } else {
            wmds_schema::parse_assembly(&name, &src)
                .map(|d| format!("{} v{}, {} instances, {} mates", d.id, d.version, d.instances.len(), d.mates.len()))
                .map_err(|e| format!("{:?}", miette::Report::new(e)))
        };
        match outcome {
            Ok(summary) => println!("ok    {}  ({summary})", f.display()),
            Err(e) => {
                failures += 1;
                println!("FAIL  {}", f.display());
                eprintln!("{e}");
            }
        }
    }
    println!("{} file(s), {failures} failure(s)", files.len());
    if failures == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
