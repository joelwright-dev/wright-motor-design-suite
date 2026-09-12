# 09 - Roadmap

Phases are ordered so that each one produces something usable and retires the biggest remaining
risk. Durations assume one to two full-time developers and are estimates.

## Phase 0 - Kernel spike (4 to 6 weeks)

Goal: prove the riskiest dependency before building on it.

* Rust workspace skeleton with the crate boundaries from doc 02.
* `GeomKernel` trait; OCCT adapter via `opencascade-rs` or a thin `cxx` bridge; builds on
  Windows, macOS and Linux in CI.
* KDL parser for a minimal primitive: params, `tube`/`box`/`cylinder`, union, one port.
* egui window with a wgpu viewport showing the tessellated primitive and its port frames.
* Mass properties and STEP export of the result.
* Exit criterion: the control-arm example from doc 03 loads from its file, renders, reports mass,
  exports STEP that opens in FreeCAD. If OCCT cannot be made to build reliably on all three
  platforms in this phase, switch to the out-of-process fallback before Phase 1.

## Phase 1 - Model, library and MCDSv1 (10 to 14 weeks)

* Full primitive schema, port registry, mates with DOF and fasteners, placement solver.
* Chassis system loader; MCDSv1 section generator producing rails, cross-members, joint fittings
  and grid-station ports from parameters.
* Library browser, property editor, undo/redo, project save/load, `wmds.lock`.
* Tier 0 analytics live in the UI.
* Initial library: enough primitives to assemble one complete reference vehicle (a 2/3-length
  narrow EV hatch) with envelopes where detail is not yet available.
* CLI: `wmds lib validate`, `wmds lib preview`, `wmds analyse`.
* Exit criterion: changing the central section length on the reference vehicle regenerates the
  chassis and keeps every mounted component attached (WMDS-21 acceptance test).

## Phase 2 - Compliance (6 to 8 weeks)

* Rule engine, query API, incremental evaluation, viewport highlighting.
* `wright-internal` pack and the first ADR ICV pack with at least the dimensional, lighting,
  mirror, seating and restraint-anchorage rules that are pure calculation.
* Report export (PDF, JSON); `wmds check` in CI.
* Regulatory review of the pack with an approved signatory; `verified_by` populated.
* Exit criterion: the reference vehicle has a complete report with no unverified rules in the
  calculation class.

## Phase 3 - Tier 1 dynamics (10 to 12 weeks)

* Mate graph to multibody tree; in-house integrator; Pacejka tyre; drivetrain and brake models.
* Standard manoeuvres from doc 06 with results plotting.
* Interactive driving in the viewport, with Rapier as an optional backend to compare.
* Validation cases 0 and 1 from doc 06 section 5 passing in CI.
* Lumped-mass crash pre-screen.
* Exit criterion: understeer gradient and stopping distance of the reference vehicle reported
  with Tier 1 provenance and within validation bands on the analytic cases.

## Phase 4 - Manufacturing and assembly (8 to 10 weeks)

* BOM and cost roll-up; export recipes for flat-cut, tube, pultrusion-cut, composite-layup,
  machined, additive, purchased.
* Build-sequence derivation; instruction generator; PDF and WASM interactive viewer.
* First usability test of generated instructions on a chassis section with two non-engineers.
* Exit criterion: a complete kit BOM and instruction set for the reference vehicle chassis;
  usability test completed and findings logged.

## Phase 5 - Tier 2 FE (10 to 14 weeks)

* Gmsh integration; mid-surface extraction; material cards; connection modelling.
* CalculiX adapter: torsional stiffness, bending, modes.
* OpenRadioss adapter: full-width frontal and side cases; result import and summary.
* Validation: aluminium crush tube against published data.
* Exit criterion: the round-trip acceptance test in WMDS-43 passes on the reference vehicle.

## Phase 6 - Prototype correlation (runs alongside physical MCDSv1 prototype)

* Coupon and crush-tube tests on the chosen pultruded rail material; material cards calibrated.
* Measured chassis mass and torsional stiffness against WMDS predictions.
* Low-speed sled or drop test against Tier 2.
* Every result recorded as a validation case in CI. Remove "uncalibrated" labels as each is met.

## Ongoing

* Library growth is continuous and mostly non-programming work (doc 03 section 6).
* Rule packs for additional ADR categories, then UNECE and FMVSS.
* Re-evaluate truck as a pure-Rust kernel at each of its releases; switch when the conformance
  suite passes and STEP import exists.

## Immediate next steps

1. Put this documentation under version control (`git init` in this folder) and review it.
2. Resolve the ASSUMPTION items in doc 04 that affect the software early: length vs width
   configurations, grid pitch, joint style.
3. Start Phase 0.
