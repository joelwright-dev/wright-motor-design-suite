# 02 - Architecture

## 1. Principles

1. **Data over code.** Primitives, chassis systems, rule packs, materials, manoeuvres and load
   cases are all definition files. The application is an interpreter for those files. This is the
   only way to satisfy WMDS-03 and WMDS-20.
2. **One model, many views.** The assembly graph (instances + mates) is the single source of truth.
   Geometry, mass properties, kinematics, FE meshes, BOMs and assembly instructions are all
   derived from it. Nothing is authored twice.
3. **Core without GUI.** Every operation is available as a library call and a CLI command. The GUI
   is a client of the core (WMDS-62).
4. **Solvers are plug-ins behind stable interfaces.** Geometry kernels and simulation solvers are
   the riskiest dependencies. Each sits behind a trait so it can be swapped.
5. **Honest numbers.** Every derived value carries provenance: which tier, which solver, which
   input hash (WMDS-46, WMDS-47).
6. **Plain text on disk.** Projects are directories of text files that diff and merge (WMDS-61).

## 2. Module map

The implementation is a Rust workspace (see doc 08 for the language decision). Crate boundaries
below are also the conceptual boundaries; keep them even if the crate layout changes.

```
wmds/
  crates/
    wmds-units      Quantities with units; compile-time dimensional safety where practical.
    wmds-expr       Expression language used inside definition files (parameters, rule checks).
    wmds-schema     Parsers and validators for every definition file type.
    wmds-model      Core data model: Primitive, Instance, Port, Mate, Assembly, Material, Vehicle.
    wmds-geom       Geometry abstraction (GeomKernel trait) and adapters: native B-rep kernel,
                    mesh CSG kernel, optional OpenCASCADE backend. STEP/STL/glTF/DXF I/O.
    wmds-kinematics Mate graph -> multibody topology, joint frames, DOF analysis, clash detection.
    wmds-massprops  Mass, CG, inertia roll-up from geometry and materials.
    wmds-analytics  Tier 0 vehicle calculations.
    wmds-sim-dyn    Tier 1 real-time vehicle dynamics (own solver or physics-engine adapter).
    wmds-sim-fe     Tier 2 FE pipeline: meshing, solver input generation, result import.
    wmds-rules      Rule pack loader and compliance engine.
    wmds-mfg        BOM, cut/ply/machining exports, cost roll-up.
    wmds-assembly   Build-sequence derivation and instruction generation.
    wmds-project    Project directory format, load/save, versioning, undo journal.
    wmds-cli        Command line front end.
    wmds-app        Desktop GUI: 3D viewport, property editors, library browser, reports.
  library/          Shipped primitive definitions (see doc 03).
  chassis/          Shipped chassis systems (MCDSv1, doc 04).
  rules/            Shipped rule packs (doc 05).
  materials/        Material database.
  loadcases/        Manoeuvres and crash load cases (doc 06).
```

Dependency direction is strictly downward in the list: `wmds-app` depends on everything;
`wmds-model` depends only on units, expr and schema. No cycle is permitted.

## 3. Core data model

### 3.1 Entities

```
Vehicle
  metadata: name, category (ADR MA/MB/MC/NA...), target market(s), rule packs, chassis system
  root: Assembly

Assembly (is-a Primitive when saved to the library)
  instances: [Instance]
  mates:     [Mate]
  params:    [Param]          exposed parameters
  ports:     [PortDecl]       external ports (re-exported from children)

Instance
  id, primitive_ref (name + version), param_values, variant, placement (derived from mates,
  or FreePlacement{transform, justification})

Primitive
  id, version, category, description
  params:     [Param]         name, unit, default, range, expression
  geometry:   GeometrySpec    feature list OR imported file ref
  material:   MaterialRef | per-body assignment
  massprops:  Computed | Declared{mass, cg, inertia}
  ports:      [PortDecl]
  behaviour:  Option<BehaviourModel>   engine map, spring curve, tyre model, battery model...
  mfg:        [MfgMethod]     method, scale range, cost model, export recipe
  compliance: [Tag]           e.g. "lighting.headlamp", "restraint.anchorage"
  variants:   [Variant]

PortDecl
  name, port_type (from a registry), frame (position + orientation in primitive coords, may be
  parametric), params (PCD, thread, diameter, connector family...), load_rating, grid (optional:
  snaps to chassis mount grid), fastener_spec (default fasteners for mates on this port)

Mate
  a: (instance, port), b: (instance, port)
  dof: Fixed | Revolute{axis} | Prismatic{axis} | Spherical | Planar | Custom
  fasteners: FastenerSpec (overrides port default)
  stage: Factory | Kit        who performs this joint (see MCDS-06)
  offset: optional small transform for shim/adjustment

Material
  id, family (metal, CFRP, GFRP, polymer, elastomer, glass...), density,
  elastic (isotropic or orthotropic), strength, fatigue curve, thermal, cost per kg,
  solver cards (per solver: OpenRadioss material law + parameters, implicit FE, ...)

Param
  name, unit, default, min, max, expr (optional), doc
```

### 3.2 Placement is derived

An instance has no independent position. Its placement is solved from the mate graph: pick a root
(the chassis central section), then walk mates outward, composing port frames. This makes
placement always consistent and is what lets a chassis length change ripple through the vehicle
(WMDS-21). Free placement exists only as an escape hatch and is flagged in reports.

### 3.3 Expressions

A small, pure expression language with units:

```
length = 2 * track_width - 120 mm
rate   = if (spring.type == "coil") then spring.k else 0 N/mm
ok     = headlamp.centre.z >= 500 mm and headlamp.centre.z <= 1200 mm
```

It is used in primitive parameters, port frames, rule checks and cost models. It has no loops, no
side effects, and no I/O. Evaluation order is dependency-sorted; cycles are errors.

## 4. Geometry subsystem

### 4.1 The kernel trait

```
trait GeomKernel {
    fn build(&self, spec: &GeometrySpec, params: &ParamEnv) -> Result<Solid>;
    fn boolean(&self, op: BoolOp, a: &Solid, b: &Solid) -> Result<Solid>;
    fn tessellate(&self, s: &Solid, tol: Tolerance) -> Mesh;
    fn mass_props(&self, s: &Solid, density: Density) -> MassProps;
    fn export_step(&self, s: &Solid) -> Result<Bytes>;
    fn import_step(&self, b: &[u8]) -> Result<Solid>;
    fn section(&self, s: &Solid, plane: Plane) -> Vec<Wire>;   // for DXF, ply outlines
}
```

Two implementations are planned (doc 08): a native Rust B-rep kernel for parametric feature
geometry and a mesh-based CSG kernel as fallback for robustness. An OpenCASCADE-backed adapter is
the reserve option for STEP import fidelity. The rest of WMDS never touches a kernel type directly.

### 4.2 GeometrySpec

A feature list, evaluated in order:

```
sketch(plane, profile) -> extrude | revolve | sweep(path) | loft
box, cylinder, tube, plate, channel, i-beam, hat-section   (library shapes)
fillet, chamfer, shell, pattern(linear|circular), mirror
union, subtract, intersect
import(step|stl, file)
```

Profiles are parametric 2D wires: lines, arcs, splines, with dimension constraints. This is
deliberately a subset of what a full CAD sketcher offers; it is enough for chassis rails,
brackets, arms, tanks and envelopes. Anything harder is imported.

### 4.3 Level of detail

Every primitive may declare up to three geometry levels: `envelope` (bounding shape for packaging
and clash), `display` (what the designer sees) and `manufacture` (full detail). Simulation meshes
are generated from `manufacture` for structural parts and from `envelope` for masses.

## 5. Simulation subsystem

See doc 06. Architecturally:

* `wmds-kinematics` turns the mate graph into a multibody topology. This is the one bridge
  between design and simulation; it must be exact.
* `wmds-sim-dyn` owns a `VehicleModel` built from that topology plus behaviour models. It runs
  either in an internal fixed-step integrator or through a physics-engine adapter.
* `wmds-sim-fe` owns meshing and solver I/O. Solvers are external processes. WMDS writes the deck,
  runs the solver, parses results. Solver adapters implement:

```
trait FeSolver {
    fn write_deck(&self, model: &FeModel, case: &LoadCase, dir: &Path) -> Result<()>;
    fn run(&self, dir: &Path, opts: &RunOpts) -> Result<RunHandle>;
    fn read_results(&self, dir: &Path) -> Result<FeResults>;
}
```

## 6. Compliance subsystem

See doc 05. `wmds-rules` loads rule packs, evaluates applicability against the vehicle metadata,
evaluates check expressions against a query API over the model, requests evidence from
simulations when needed, and produces a `ComplianceReport`. It is incremental: rules subscribe to
the model paths they read, so a parameter change re-evaluates only affected rules (WMDS-33).

## 7. Manufacturing subsystem

See doc 07. `wmds-mfg` walks the assembly, groups instances by manufacturing method, runs the
export recipe declared on each primitive, and rolls up the BOM. `wmds-assembly` derives the build
sequence from the mate graph and renders instructions.

## 8. File formats

### 8.1 Definition files

All definition files use one text format. **ASSUMPTION:** KDL (kdl-lang.org) - it is more readable
than TOML for nested structures, has proper comments, and has a maintained Rust implementation.
TOML is the fallback. Examples in doc 03 are written in KDL. The parser is isolated in
`wmds-schema` so the choice can change before v1 without touching anything else.

| Extension | Content |
|-----------|---------|
| `.prim.kdl` | Primitive definition |
| `.chassis.kdl` | Chassis system definition |
| `.rules.kdl` | Rule pack |
| `.mat.kdl` | Material |
| `.case.kdl` | Manoeuvre or crash load case |
| `.veh.kdl` | Vehicle (root assembly + metadata) |
| `.asm.kdl` | Saved sub-assembly |

### 8.2 Project directory

```
my-vehicle/
  vehicle.veh.kdl
  assemblies/          project-local sub-assemblies
  primitives/          project-local primitive overrides and additions
  results/             simulation results, keyed by input hash (may be git-ignored)
  reports/             generated compliance and manufacturing outputs
  wmds.lock            exact versions of every library primitive, rule pack and material used
```

### 8.3 Library layout

```
library/
  chassis/
  suspension/
    arms/ uprights/ springs/ dampers/ arb/ subframes/
  steering/ braking/ drivetrain/ energy/ cooling/ exhaust/ electrical/
  wheels/ body/ closures/ glazing/ lighting/ seating/ restraints/ interior/
  index.kdl           generated: id -> path -> version
```

## 9. Performance targets

| Operation | Target |
|-----------|--------|
| Parameter change to viewport update, single primitive | < 50 ms |
| Chassis length change, full vehicle regeneration | < 2 s |
| Tier 0 analytics after any change | < 100 ms |
| Full compliance re-evaluation | < 1 s incremental, < 30 s cold |
| Tier 1 dynamics | real-time at 1 kHz physics step |
| Tier 2 crash deck generation | < 5 min for the reference vehicle |
| Project load | < 5 s |

## 10. Testing strategy

* Definition-file round-trip tests: parse, serialise, parse; must be identical.
* Golden-model tests: a reference vehicle whose mass, CG, axle loads and compliance report are
  checked against stored values on every commit.
* Kernel conformance suite: the same geometry specs run through every kernel adapter; volumes and
  areas must agree within tolerance.
* Simulation validation cases: published test data (e.g. a steady-state cornering case with known
  understeer gradient; a crush tube with published force-displacement) reproduced within stated
  error bands. Recorded in doc 06.
* Usability test of generated assembly instructions with non-engineers (WMDS-52).
