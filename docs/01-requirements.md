# 01 - Requirements

Requirements are written to be testable. Each has an ID, a priority (M = must for v1, S = should,
C = could) and, where useful, an acceptance test.

## Part A - WMDS (software)

### A1. Primitive library

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-01 | M | The suite ships with a library of parametric primitives covering, at minimum: chassis sections, suspension (arms, uprights, springs, dampers, anti-roll bars, subframes), steering (rack, column, tie rods), braking (calipers, discs, master cylinder, lines, booster), drivetrain (ICE, electric motor, gearbox, differential, driveshafts, half-shafts), fuel and energy storage (tanks, battery packs, lines), cooling, exhaust, electrical (harness segments, fuse boxes, battery), wheels and tyres, body panels, closures, glazing, lighting, seats, restraints, interior envelopes. | Library index lists a primitive in every category with at least one working example. |
| WMDS-02 | M | Every primitive carries: parametric geometry, mass and inertia (computed or declared), material assignment, ports, behaviour model where applicable, cost estimate, manufacturing method, and compliance tags. | Loading a primitive missing a required field fails with a clear error naming the field. |
| WMDS-03 | M | Primitives are defined in human-readable definition files. Creating a new primitive requires no changes to application code and no recompilation. | A user with the docs and a text editor can add a new anti-roll bar primitive and see it in the library within one session. |
| WMDS-04 | M | Primitive definitions support parameters with units, ranges, defaults, and expressions referencing other parameters. | A control-arm length expressed as a function of track width updates when track changes. |
| WMDS-05 | M | Primitives can wrap imported geometry (STEP, STL) for parts modelled elsewhere, while still exposing ports and mass properties. | An imported engine block STEP file becomes a primitive with four mount ports. |
| WMDS-06 | S | The library supports versioning of primitives so a vehicle records exactly which version of each part it used. | Opening a vehicle after a primitive changes shows a diff and offers upgrade or pin. |
| WMDS-07 | S | Primitives can declare variants (e.g. left/right hand, 4-stud/5-stud) without duplicating the definition. | |

### A2. Mounting and modularity

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-10 | M | Every mechanical, fluid, electrical and structural connection between components is expressed as a mate between two typed ports. | No component can be positioned in a vehicle except by mating or by explicit free placement flagged as such. |
| WMDS-11 | M | Port compatibility is rule-based: type, parameters (bolt PCD, thread, hose diameter, connector family), and load rating are checked at mate time. | Mating a 4x100 hub port to a 5x114.3 wheel port is refused with the reason. |
| WMDS-12 | M | Ports can be positioned on the chassis mount grid so that a component mounted at grid station N can be re-mounted at station N+k without redefinition. | Moving the fuel tank three stations rearward requires one parameter change. |
| WMDS-13 | M | Mates carry fastener specifications (type, size, grade, torque, quantity, thread locker) so that fasteners flow into the BOM and assembly instructions automatically. | BOM fastener count equals the sum over mates. |
| WMDS-14 | M | Assemblies nest: a sub-assembly (e.g. a complete corner: upright, hub, brake, arms) is itself a primitive with external ports. | |
| WMDS-15 | S | Mates can express degrees of freedom (fixed, revolute, prismatic, spherical) so that the same mate graph drives the kinematic model used by simulation. | Suspension travel animates from the mate graph without a separate kinematic definition. |
| WMDS-16 | S | Clash detection runs across the assembly and reports interferences with the two instances and the overlapping volume. | |

### A3. Chassis systems (MCDS support)

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-20 | M | A chassis system is defined in data files: sections, section joints, rail cross-section, mount grid pitch, length ranges, width options, material and layup. | MCDSv1 loads from definition files with zero chassis-specific application code. |
| WMDS-21 | M | The designer can set section lengths within their allowed range and the software regenerates rails, cross-members, joints, the mount grid and every dependent mate. | Changing central section length from 1800 to 2200 mm keeps all mounted components attached at correct stations. |
| WMDS-22 | M | Any front, central or rear section that conforms to the section-joint interface can be substituted. | A truck rear section and an SUV rear section interchange on the same central section. |
| WMDS-23 | S | Chassis structural properties (bending and torsional stiffness, mass, first modes) are estimated at Tier 0 and updated live as parameters change. | |

### A4. Compliance

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-30 | M | Regulation rule packs are data files, versioned, and selectable per vehicle (e.g. ADR for category MA passenger car, MC off-road, NA light goods). | |
| WMDS-31 | M | Each rule declares what it measures, the check expression, applicability conditions (vehicle category, mass, date), and the evidence class required (calculation, simulation, physical test, declaration). | |
| WMDS-32 | M | The compliance report lists every applicable rule with status: Pass, Fail, Needs Physical Test, Needs Input, Not Applicable, and links each to its evidence. | Report is exportable as PDF and machine-readable JSON. |
| WMDS-33 | M | Compliance runs continuously in the background and failures appear on the affected components in the design view. | Lowering a headlamp below the ADR minimum height highlights the headlamp within a second. |
| WMDS-34 | S | Rules can request simulation results as evidence (e.g. brake performance, occupant compartment intrusion) and trigger those simulations. | |
| WMDS-35 | M | The software never presents itself as certifying a vehicle. Reports state that certification is by an approved signatory. | Wording present on every report. |

### A5. Simulation

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-40 | M | Tier 0 analytics are always available: mass, CG, axle loads, weight distribution, static stability factor, brake force distribution, gear-by-gear acceleration, top speed, range, turning circle. | Results update within 100 ms of any parameter change. |
| WMDS-41 | M | Tier 1 driving dynamics: a real-time multibody vehicle model derived from the assembly (suspension kinematics from mates, spring and damper curves, tyre model, drivetrain torque path) driven through standard manoeuvres (constant radius, step steer, sine with dwell, double lane change, straight-line braking, ride over ISO road profiles). | Outputs understeer gradient, roll gradient, yaw rate response, stopping distance, ride RMS. |
| WMDS-42 | M | Tier 1 is interactive: the designer can drive the vehicle with keyboard or gamepad in a 3D viewport. | |
| WMDS-43 | M | Tier 2 crash: the assembly is meshed and exported to an explicit nonlinear FE solver for frontal, offset frontal, side, rear and rollover load cases defined by the rule packs; results are imported and summarised (intrusion, pulse, energy absorbed per component). | Round trip on the MCDSv1 reference vehicle completes without manual editing of solver input. |
| WMDS-44 | S | Tier 2 structural: static and modal FE of the chassis (torsional stiffness, bending stiffness, first modes) through an implicit solver. | |
| WMDS-45 | S | Tier 1 crash pre-screen: a lumped-mass/spring model gives a crash pulse estimate in seconds for early design iteration. | |
| WMDS-46 | M | Every simulation result records the model version, solver, solver version, and input hash so it can be reproduced. | |
| WMDS-47 | M | Fidelity is labelled on every result. A Tier 0 number is never displayed in a way that could be mistaken for a Tier 2 result. | |

### A6. Manufacturing and assembly export

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-50 | M | Full BOM with hierarchy, quantities, materials, mass, cost, supplier part numbers, and make/buy flag. | CSV and JSON export. |
| WMDS-51 | M | Per-part manufacturing exports according to the part's manufacturing method: DXF for flat cut, STEP for machined, STL/3MF for printed, ply book and mould geometry for composite, tube cut-and-notch list for tubular. | |
| WMDS-52 | M | Assembly instruction generator: derives an ordered build sequence from the mate graph, produces per-step exploded views, fastener callouts, torque values, tools required, and warnings. | A person with no engineering background can follow the output to assemble a chassis section in a usability test. |
| WMDS-53 | M | Instructions export as PDF and as an interactive 3D step-through viewer. | |
| WMDS-54 | S | Jig and fixture generation for chassis section assembly (location pins, bonding fixtures). | |
| WMDS-55 | S | Cost roll-up by manufacturing scale (1, 10, 100, 1000 units) using per-primitive cost models. | |
| WMDS-56 | M | STEP and glTF export of any assembly for use in other tools. | |

### A7. Platform and usability

| ID | Pri | Requirement | Acceptance |
|----|-----|-------------|------------|
| WMDS-60 | M | Runs on Windows, macOS and Linux from a single install with no external runtime (no Python, no JVM, no .NET required). | |
| WMDS-61 | M | All project data is plain-text, diff-friendly files suitable for git. | A vehicle project can be version-controlled and merged. |
| WMDS-62 | M | The core is usable without the GUI (command line and library) so that checks, simulations and exports can run in CI. | `wmds check vehicle.wmds` exits non-zero on a compliance failure. |
| WMDS-63 | S | Undo/redo across all design operations. | |
| WMDS-64 | S | Interactive 3D viewport performance: 60 fps with a full vehicle at display resolution on integrated graphics from 2020 onward. | |
| WMDS-65 | C | Browser build of the viewer and assembly instructions for sharing with customers. | |

## Part B - MCDS (physical platform, as constrained by the software)

These are the properties the physical platform must have. They are recorded here because the
software must be able to model, check and report on each of them. Detailed engineering targets
live in the MCDSv1 spec (doc 04).

| ID | Pri | Requirement | How WMDS supports it |
|----|-----|-------------|----------------------|
| MCDS-01 | M | **Versatile.** One chassis family covers city car through light truck: vehicle categories MA, MB, MC, NA (ADR categories) and equivalents elsewhere. | Section length ranges and section interchange (WMDS-21, WMDS-22); rule packs per category. |
| MCDS-02 | M | **Lightweight.** Chassis mass is a small fraction of kerb mass so that mass budget goes to powertrain, battery and body. Target set in doc 04. | Tier 0 live mass roll-up; chassis mass shown as fraction of kerb mass. |
| MCDS-03 | M | **Durable.** The chassis outlasts every other system: design life target set in doc 04 for fatigue, corrosion, and environmental exposure. | Material data includes fatigue and environmental properties; Tier 2 fatigue load cases. |
| MCDS-04 | M | **Cheap.** Chassis unit cost target set in doc 04, at defined volumes. | Cost models per primitive and per manufacturing method; roll-up by scale (WMDS-55). |
| MCDS-05 | M | **Manufacturable at multiple scales.** Every chassis part can be made by a small workshop (single unit) and by a production line (thousands) using documented processes. | Each primitive declares manufacturing methods and the scale range for each; exports per method. |
| MCDS-06 | M | **Flatpack-assemblable.** A complete vehicle is assembled with hand tools and a documented sequence by a person with no trade training. No welding, no bonding requiring a controlled environment at the assembler's end, no press fits requiring special tooling. | Assembly instruction generator (WMDS-52); mate fastener rules forbid non-flatpack joining methods outside the factory-assembled stage. |
| MCDS-07 | M | **Interchangeable sections.** Front, central and rear sections conform to a single section-joint interface. | Section-joint port type defined once; all sections must implement it. |
| MCDS-08 | M | **Length adjustable.** Sections are available in a range of lengths on a fixed mount-grid pitch. | Mount grid (WMDS-12). |

## Part C - Non-requirements (explicitly excluded)

* Cloud-hosted collaboration. Files in git are the collaboration model for v1.
* Rendering quality beyond what is needed to read the design.
* Native mobile apps.
