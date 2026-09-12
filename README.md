# Wright Motor Design Suite (WMDS)

WMDS is the design, analysis, compliance and manufacturing toolchain for vehicles built on the
Modular Chassis Design System (MCDS). It is the software half of a two-part system:

| Part | What it is |
|------|------------|
| **MCDS** | A physical, modular, length-adjustable chassis platform. One universal chassis, many vehicle types. |
| **WMDS** | The software that lets a designer assemble a complete vehicle from a library of parametric primitives, check it against road-vehicle regulations, simulate how it drives and crashes, and export everything needed to manufacture and assemble it. |

## Status

Phase 0 (kernel spike) in progress. See [docs/09-roadmap.md](docs/09-roadmap.md).

Working today:

* `wmds-units`: quantities with runtime dimensional checking (`380 mm`, `20 kN`, `1550 kg/m^3`).
* `wmds-expr`: the expression language used inside definition files.
* `wmds-schema`: KDL parser and validator for `.prim.kdl` primitive definitions.
* `wmds-model`: parameter resolution (dependency order, unit coercion, ranges, variants), port
  frames, geometry feature arguments, cost expressions.
* `wmds` CLI: `wmds lib validate <paths>` and `wmds lib show <file> --set name=value`.

Not yet: geometry kernel, viewport, mass properties from geometry, STEP export.

## Building

Requires a stable Rust toolchain (rustup) and, on Windows, Visual Studio Build Tools with the
C++ workload.

```bash
cargo test --workspace
cargo run -- lib validate library
cargo run -- lib show library/suspension/arms/lca-wishbone-a.prim.kdl --set span=420mm --set hand=right
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
| 09 | [Roadmap](docs/09-roadmap.md) | Phased delivery plan |

## Conventions used in the documents

* Requirements are numbered `WMDS-nn` and `MCDS-nn` and are referenced by that ID everywhere else.
* Anything marked **OPEN** is a decision still to be made. Anything marked **ASSUMPTION** is a
  decision made provisionally in the docs so that work can continue; it should be confirmed or overturned.
* SI units internally. Millimetres for geometry, kilograms, newtons, seconds.
