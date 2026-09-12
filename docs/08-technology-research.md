# 08 - Technology Research and Recommendation

Research date: September 2026. Every external fact below was checked against a current source
(listed in section 9); where something could not be verified it is marked as such.

## 1. Summary recommendation

**Rust for the application, with C++ geometry and meshing libraries behind Rust interfaces, and
open-source FE solvers run as external processes.** Rust-first, not Rust-only.

| Concern | Choice | Fallback |
|---------|--------|----------|
| Language | Rust (stable, 2024 edition) | - |
| Definition file format | KDL v2 via the `kdl` crate | TOML |
| B-rep geometry kernel | OpenCASCADE Technology (OCCT) 8.x through Rust bindings | truck (pure Rust) for simple parametric parts and as the long-term pure-Rust path |
| Mesh CSG, clash, hulls | manifold (pure-Rust port or C++ bindings) | - |
| FE meshing | Gmsh through its C API | - |
| Explicit crash solver | OpenRadioss (external process) | LS-DYNA deck writer (same keyword format) |
| Implicit structural solver | CalculiX (external process) | Code_Aster |
| Real-time dynamics | In-house multibody integrator in Rust | Rapier as interactive backend and cross-check |
| High-fidelity dynamics reference | Project Chrono::Vehicle (export, not embedded) | - |
| GUI | egui/eframe with a custom wgpu 3D viewport | Tauri with a web front end; Bevy for the driving-sim view |
| Browser viewer for instructions | Rust compiled to WebAssembly, wgpu/WebGL rendering of pre-tessellated glTF | three.js |
| Persistence | Plain text files in git; no database | - |

The rest of this document is the evidence.

## 2. Why Rust

The brief asks for software that is easy to run and use on any computer. That drives the choice
more than anything else.

**For Rust**

* Single static binary per platform, no runtime to install. This is the whole of WMDS-60 and it is
  hard to get with Python, Java or .NET.
* Performance headroom for the parts that must be fast in-process: geometry regeneration on a
  chassis length change, Tier 0 analytics at interactive rates, a 1 kHz multibody solver, meshing
  of a full vehicle.
* Memory safety without a garbage collector matters for a long-running desktop tool holding a
  large model.
* Compiles to WebAssembly, which gives the browser-based assembly viewer (doc 07) from the same
  code.
* A mature ecosystem for exactly the pieces WMDS needs: `wgpu` for cross-platform GPU rendering,
  `egui` for tool UI, `nalgebra` for linear algebra, `rapier` for rigid-body physics, `kdl` and
  `serde` for data files, `rayon` for parallelism.
* The FFI story to C and C++ is good (`cxx`, `bindgen`), which is what makes the hybrid approach
  in section 4 workable.

**Against Rust, honestly**

* There is no mature pure-Rust B-rep CAD kernel. Fornjot, the best-known attempt, is discontinued
  (its own repository says it is no longer in development and its goals were not reached). truck
  exists and is progressing, but its STEP support is export-only and it cannot export shapes
  produced by boolean operations (truck-stepio 0.3.0, September 2026). This is the single
  largest technical risk and it is why the recommendation is a C++ kernel behind a Rust trait.
* Compile times are long; a workspace this size will take minutes for a clean build.
* Fewer engineers know it than Python or C++. For a small team that intends to own the code long
  term this matters less than it would for a large hiring plan.

Verdict: the risks are in dependencies, not in the language, and they are the same risks any
language would face. Rust is the right choice for the core.

## 3. Alternatives considered

| Stack | Strengths | Why not |
|-------|-----------|---------|
| **Python + build123d/CadQuery (OCCT) + PySide** | Fastest to a working parametric prototype; OCCT already wrapped; huge scientific ecosystem for analytics | Distribution is heavy (bundled interpreter, hundreds of MB), real-time dynamics and meshing performance need native code anyway, and "runs on any computer" becomes an installer problem. Good for throwaway prototypes, not for the product. |
| **C++ (the FreeCAD model)** | Direct OCCT, Gmsh, Chrono, every solver; maximum ecosystem | Development speed and safety are much worse; build system complexity is the same as the hybrid plan but without the Rust benefits. |
| **TypeScript + three.js + WASM kernel** | Best UI toolkit in existence; instant sharing | Kernel and solvers would still have to be native or WASM; a heavy desktop CAD tool in a browser shell is fighting the platform. Reserve for the viewer only. |
| **FreeCAD workbench (Python add-on)** | Zero kernel work; assembly, meshing, and a CalculiX/OpenRadioss workflow already exist in FreeCAD 1.1 (released March 2026) | WMDS would be a plug-in to someone else's application: data model, UI and release cadence are not ours; the data-driven primitive system and port-based assembly would fight FreeCAD's own model. Worth using FreeCAD as a *reference workflow* and STEP round-trip partner. |
| **Unity or Unreal for the simulator** | Excellent real-time 3D | Wrong tool for CAD; licensing; no advantage over wgpu once the model is ours. |

## 4. Geometry kernel

### 4.1 Candidates

| Kernel | Language | Licence | B-rep | Booleans | STEP in | STEP out | Fillets, shells | Status (Sept 2026) |
|--------|----------|---------|-------|----------|---------|----------|-----------------|--------------------|
| **OpenCASCADE (OCCT)** | C++ | LGPL 2.1 with exception (proprietary use allowed) | full | yes, mature | yes | yes | yes | 8.0.1 released July 2026; used by FreeCAD, KiCad, SALOME |
| **truck** | Rust | Apache 2.0 | yes (NURBS-based) | yes (`truck-shapeops`), described as recent | no | yes, but not for boolean results | limited | active; 0.x versions |
| **Fornjot** | Rust | - | partial | - | - | - | - | discontinued |
| **manifold** | C++ with a pure-Rust port | Apache 2.0 | no (mesh only) | yes, robust, guaranteed-manifold output | no | no | no | active; `manifold-rust` and `manifold-csg` crates updated 2026 |

### 4.2 Decision

OCCT is the only candidate that satisfies STEP import (WMDS-05), robust booleans on parametric
solids, sectioning for DXF and ply outlines, and fillets. It is also what the composite and FE
tool ecosystem (FreeCAD, Gmsh, SALOME) already speaks.

Binding options, in order of preference:

1. `opencascade-rs` (bschwind): existing Rust bindings built on `cxx`. Its README calls it a work
   in progress maintained in spare time. It covers enough to start and can be forked and extended.
2. Own thin `cxx` bridge exposing only the operations in the `GeomKernel` trait (doc 02). Smaller
   surface than full bindings, and the trait already limits what is needed. This is the likely
   end state even if option 1 is the starting point.
3. Out-of-process OCCT service (a small C++ executable exchanging STEP and mesh files). Simplest
   to build, slowest, and a fallback only if linking proves painful on one platform.

truck stays in the picture: the `GeomKernel` trait is implemented for it as well, the conformance
suite (doc 02, section 10) runs against both, and if it reaches STEP import and boolean export it
becomes the pure-Rust default. manifold handles envelope-level CSG and clash detection where
B-rep is unnecessary.

Build cost: OCCT is a large C++ dependency. Mitigation is prebuilt OCCT binaries per platform in
CI (vcpkg or the OCCT release archives) and a `WMDS_OCCT_DIR` override for developers. This is a
known, solved problem for KiCad and FreeCAD.

## 5. Simulation stack

### 5.1 Explicit crash: OpenRadioss

* Open-source (AGPL v3) release of Altair Radioss; stable 2025 release, September 2025;
  continuously developed on GitHub.
* Reads Radioss and LS-DYNA keyword decks, so a deck writer targeting it is close to free for
  LS-DYNA users.
* Composite laws needed for MCDSv1 exist: LAW25 (Tsai-Wu and CRASURV progressive damage, shells
  and solids) and fabric laws LAW19/LAW58.
* An established open workflow exists: FreeCAD or Gmsh for meshing, OpenRadioss to solve,
  ParaView to view. WMDS automates the middle of that workflow.
* AGPL is compatible with running the solver as a separate process and exchanging files. WMDS
  does not link it.

### 5.2 Implicit structural: CalculiX

Mature Abaqus-style solver, GPL, used through PrePoMax (2.6.0, August 2026) by many small
engineering teams. Same process boundary as OpenRadioss.

### 5.3 Meshing: Gmsh

Gmsh 4.15.2 (March 2026) with a stable C API; unofficial Rust bindings exist (`gmsh-sys`, `rgmsh`)
and are thin enough to maintain in-tree if abandoned. Gmsh reads STEP through its own OCCT build,
which is a second reason OCCT is the right kernel: the geometry WMDS meshes is the geometry Gmsh
understands.

### 5.4 Real-time vehicle dynamics

Options:

| Option | Assessment |
|--------|------------|
| **In-house multibody solver in Rust** | Vehicle dynamics is a bounded, well-documented domain (Milliken, Gillespie, Pacejka). A reduced-coordinate tree solver with a few hundred DOF, stiff bushes and a Magic Formula tyre is a few thousand lines. Full control over accuracy, determinism and telemetry. Recommended as the primary Tier 1 engine. |
| **Rapier** | Rust rigid-body engine; the maintainers' 2026 goals are improved multibody accuracy for robotics and GPU physics. Fast to get an interactive prototype driving, and a good cross-check, but a general game/robotics engine is not a validated vehicle dynamics tool. Use as the interactive backend behind the `PhysicsBackend` trait, not as the source of reported numbers. |
| **Project Chrono (Chrono::Vehicle)** | C++ physics engine with a dedicated vehicle module (templates for wheeled and tracked vehicles, tyre models, terrain), version 10.0.0 released March 2026. The most capable open option for high-fidelity vehicle dynamics. No Rust bindings found. Recommended as an **export target**: WMDS writes a Chrono::Vehicle JSON model for validation runs, rather than embedding Chrono. |

### 5.5 Reference open-source vehicle physics

BeamNG and Rigs of Rods demonstrate soft-body vehicle simulation in real time; they are
inspiration for the interactive mode, not dependencies.

## 6. User interface

| Framework | Fit |
|-----------|-----|
| **egui / eframe** | Immediate-mode, pure Rust, renders through wgpu, runs on desktop and WASM. Ideal for property panels, library browsers, trees and tables. Weakest at complex layouts and native look. Recommended. |
| **wgpu (custom viewport)** | The 3D view is a custom renderer regardless of UI framework: meshes, edges, port glyphs, section planes, exploded views, FE result colouring. wgpu targets Vulkan, Metal, DX12 and WebGL/WebGPU from one code base. |
| **Tauri 2** | Rust back end, web front end. Best if the team prefers HTML/CSS for UI. Costs a second language and a web view dependency. Reserve option. |
| **Bevy** | ECS game engine with 3D rendering built in. Attractive for the interactive driving view specifically; release churn is high and its UI is weaker than egui. Possible for the simulator view only, via `bevy_egui`. Decide after the Phase 3 spike. |
| **Iced, Slint, Xilem** | Retained-mode alternatives; Slint is commercial-friendly with a designer tool; Xilem is not production-ready per current commentary. Not recommended for v1. |

## 7. Data format

KDL v2 via the `kdl` crate (6.7.1): supports KDL v2 and v1, and preserves formatting and comments
on edit, which matters for files humans maintain in git. TOML is the fallback if KDL proves
unfamiliar to contributors; the parser is isolated in one crate.

## 8. Risks and how the plan addresses them

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| OCCT bindings incomplete or painful to build on one platform | medium | high | Thin own `cxx` bridge limited to the `GeomKernel` trait; prebuilt binaries; out-of-process fallback |
| Pure-Rust kernel never reaches parity | high | low | OCCT is primary; truck is optional |
| OpenRadioss composite results unreliable without calibration | high | high | Material calibration from coupon and crush-tube tests is a project deliverable; results labelled uncalibrated until then |
| In-house dynamics solver has subtle bugs | medium | high | Validation suite (doc 06 section 5); Rapier and Chrono cross-checks |
| Compile times slow iteration | high | low | Crate boundaries as in doc 02; `cargo check` workflows; CI caching |
| egui insufficient for a rich CAD UI | low | medium | Tauri reserve; most CAD UI is panels, trees and tables, which egui handles |

## 9. Sources

* Fornjot repository, status note: https://github.com/hannobraun/fornjot
* truck README: https://github.com/ricosjp/truck/blob/master/README.md
* truck-stepio 0.3.0 docs: https://docs.rs/truck-stepio/latest/truck_stepio/
* opencascade-rs: https://github.com/bschwind/opencascade-rs
* Open CASCADE Technology (licence, 8.0.1 release): https://en.wikipedia.org/wiki/Open_Cascade_Technology
* manifold: https://github.com/elalish/manifold ; pure-Rust port: https://github.com/larsbrubaker/manifold-rust ; bindings: https://github.com/zmerlynn/manifold-csg
* OpenRadioss: https://openradioss.org/ ; https://en.wikipedia.org/wiki/Radioss ; open-source crash workflow: https://www.all-about-industries.com/how-crash-simulations-succeed-with-open-source-software-a-abddaf667ecb4ebd030a92790a6536ed/
* Radioss composite laws LAW25: https://2021.help.altair.com/2021/hwsolvers/rad/topics/solvers/rad/law25_composite_material_r.htm ; LAW19/LAW58: https://help.altair.com/hwsolvers/rad/topics/solvers/rad/law19_and_law58_fabric_composite_material_r.htm
* Project Chrono vehicle module: https://api.projectchrono.org/vehicle_overview.html ; project: https://projectchrono.org/
* Rapier 2025 review and 2026 goals: https://dimforge.com/blog/2026/01/09/the-year-2025-in-dimforge/
* Gmsh: https://gmsh.info/ ; Rust bindings: https://github.com/mxxo/rgmsh
* PrePoMax 2.6.0: https://prepomax.fs.um.si/version-2-6-0/
* FreeCAD 1.1 release: https://blog.freecad.org/2026/03/25/freecad-version-1-1-released/
* Rust GUI landscape 2026: https://wrenlearnsrust.com/posts/2026-03-11-rust-gui-landscape-2026.html ; https://blog.logrocket.com/state-rust-gui-libraries/
* kdl crate: https://docs.rs/kdl/latest/kdl/
* VSB 14 / NCOP: https://www.infrastructure.gov.au/infrastructure-transport-vehicles/vehicles/vehicle-design-regulation/rvs/bulletins/ncop ; Section LO: https://www.tmr.qld.gov.au/-/media/Safety/Vehicle-standards-and-modifications/Vehicle-modifications/Light-vehicle-modifications/NCOP/11sectionlovehiclestandardscompliance.pdf
* CFRP crashworthiness: https://acs-aus.com/our-work/crashworthiness-and-energy-absorption-of-carbon-fibre-composite-structures/ ; CFRP ladder frame evaluation: https://www.researchgate.net/publication/278407567_Evaluation_of_crashworthiness_of_a_carbon-fibre-reinforced_polymer_CFRP_ladder_frame_in_a_body-on-frame_vehicle
