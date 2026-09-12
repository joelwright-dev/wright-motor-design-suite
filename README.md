# Wright Motor Design Suite (WMDS)

WMDS is the design, analysis, compliance and manufacturing toolchain for vehicles built on the
Modular Chassis Design System (MCDS). It is the software half of a two-part system:

| Part | What it is |
|------|------------|
| **MCDS** | A physical, modular, length-adjustable chassis platform. One universal chassis, many vehicle types. |
| **WMDS** | The software that lets a designer assemble a complete vehicle from a library of parametric primitives, check it against road-vehicle regulations, simulate how it drives and crashes, and export everything needed to manufacture and assemble it. |

## Status

Phase 0 complete, Phase 1 and the compliance engine well under way. See
[docs/09-roadmap.md](docs/09-roadmap.md).

### What works today

**Design.** Primitives, assemblies and whole vehicles are defined in text files. Components
connect only through typed ports, and where a part sits is solved from the mate graph rather
than stored, so a change to the chassis moves everything mounted on it.

**MCDSv1.** The chassis platform is a data file. A generator turns a configuration choice into
rails, cross-members, section joints and a mount grid. There is no chassis-specific code.

**Compliance.** Rule packs are data. `wmds check` reports what passes, what fails, what needs a
simulation and what needs a physical test, and never reports a rule it could not evaluate as a
pass.

**Geometry.** Two kernels behind one trait: OpenCASCADE for real solids, STEP and STL, and a
pure-Rust mesh kernel for fast previews with no C++ toolchain.

### Try it

```bash
cargo run -- lib validate
cargo run -- chassis show mcds-v1 --config 2/3-length --width narrow --section front=1100mm --section central=1900mm --build
cargo run -- veh show vehicles/reference-city-ev/reference-city-ev.veh.kdl --build
cargo run -- check vehicles/reference-city-ev/reference-city-ev.veh.kdl
cargo run -p wmds-app -- vehicles/reference-city-ev/reference-city-ev.veh.kdl
```

Add `--no-default-features` to any of those to skip the OpenCASCADE build and use the mesh
kernel instead. That build is fast and needs no C++ toolchain, but it has no boolean operations,
so overlapping bodies are counted twice. Every report says which kernel produced its numbers.

### Crates

| Crate | What it does |
|-------|--------------|
| `wmds-units` | Quantities with runtime dimensional checking (`380 mm`, `20 kN`, `1550 kg/m^3`) |
| `wmds-expr` | The expression language used inside definition files |
| `wmds-schema` | KDL parsers for primitives, assemblies, vehicles, chassis systems, port types and rule packs |
| `wmds-model` | Parameter resolution, transforms, port frames, the mate graph and placement solver, the chassis generator |
| `wmds-geom` | The `GeomKernel` trait, meshes, mass properties, the feature interpreter, whole-assembly build |
| `wmds-geom-occt` | OpenCASCADE implementation of the kernel |
| `wmds-rules` | Compliance rule evaluation and reporting |
| `wmds-cli` | The `wmds` command line |
| `wmds-app` | Desktop viewer |

**Materials.** A material database with density, elastic constants, strengths, environmental
notes and per-solver cards. Every mass in the reference vehicle now comes from it; anything that
still has to guess a density says so by name.

### Not yet

Steering, brakes, rear suspension, body and interior primitives. Simulation of any kind.
Manufacturing and assembly export. Project save and undo.

## Building

Requires a stable Rust toolchain (rustup), CMake, and, on Windows, Visual Studio Build Tools
with the C++ workload. The first build compiles OpenCASCADE from source and takes 30 minutes or
more; later builds are incremental.

**Build output location.** `.cargo/config.toml` sends build output to `C:/Users/joelw/wmds-build`
rather than a `target/` folder in the project. Two reasons: MSBuild cannot build OpenCASCADE
when the build directory path is long, which a deep OneDrive path is (it fails with `FTK1011`
file-tracker errors), and it keeps OneDrive from syncing thousands of object files. On another
machine, edit that path or override it with the `CARGO_TARGET_DIR` environment variable.

**`cargo` not found after installing Rust.** The rustup installer adds `%USERPROFILE%\.cargo\bin`
to the user PATH, but programs already running keep the environment they started with. Restart
the terminal application (not just the tab), or for the current session:

```powershell
$env:Path += ";$env:USERPROFILE\.cargo\bin"
```

```bash
cargo test --workspace
cargo run -- lib validate library
cargo run -- lib show library/suspension/arms/lca-wishbone-a.prim.kdl --set span=420mm --set hand=right
cargo run -- lib show library/suspension/arms/lca-wishbone-a.prim.kdl --build --step arm.step
```

For a fast build without the geometry kernel (parsing, resolution and analytics only):

```bash
cargo build --no-default-features
```

## Documents

Read them in order; each one builds on the last.

| # | Document | Purpose |
|---|----------|---------|
| 00 | [Overview](docs/00-overview.md) | Vision, scope, users, glossary |
| 01 | [Requirements](docs/01-requirements.md) | Numbered, testable requirements for WMDS and MCDS |
| 02 | [Architecture](docs/02-architecture.md) | Module layout, core data model, file formats |
| 03 | [Primitive Library](docs/03-primitive-library.md) | The component taxonomy, the primitive definition format, and the port/mount system |
| 04 | [MCDSv1 Specification](docs/04-mcds-v1-spec.md) | The first chassis platform as WMDS must model it |
| 05 | [Compliance](docs/05-compliance.md) | How regulation checking works |
| 06 | [Simulation](docs/06-simulation.md) | Driving dynamics and crash analysis |
| 07 | [Manufacturing and Assembly Export](docs/07-manufacturing-export.md) | BOMs, cut files, ply books, flatpack-style assembly instructions |
| 08 | [Technology Research](docs/08-technology-research.md) | Language and tooling evaluation, with a recommendation |
| 09 | [Roadmap](docs/09-roadmap.md) | Phased delivery plan and current progress |
| 10 | [Progress notes](docs/10-overnight-progress.md) | What was built on 12 to 13 September, what it found, and what needs a decision |

## Conventions used in the documents

* Requirements are numbered `WMDS-nn` and `MCDS-nn` and are referenced by that ID everywhere else.
* Anything marked **OPEN** is a decision still to be made. Anything marked **ASSUMPTION** is a
  decision made provisionally in the docs so that work can continue; it should be confirmed or overturned.
* SI units internally. Millimetres for geometry, kilograms, newtons, seconds.
