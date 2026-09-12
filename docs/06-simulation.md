# 06 - Simulation: Driving Dynamics and Crash

## 1. Fidelity tiers

"Realistic" is a spectrum and it is expensive at the top. WMDS uses three tiers and labels every
number with its tier (WMDS-47). The tiers share one source: the assembly graph.

| Tier | What | Time to result | Used for |
|------|------|----------------|----------|
| **0 Analytic** | Closed-form vehicle calculations from mass properties and component parameters | milliseconds | live feedback while designing; compliance calculation rules |
| **1 Multibody** | Real-time rigid multibody vehicle with kinematic suspension from the mate graph, nonlinear springs and dampers, tyre model, drivetrain torque path; lumped-parameter crash pulse | seconds per manoeuvre, real-time interactive | handling, ride, braking, stability, driver-in-the-loop, crash pre-screen, load extraction for fatigue |
| **2 Finite element** | Implicit FE for stiffness and modes; explicit nonlinear FE for crash | minutes to hours | structural sign-off, crash evidence, joint and insert loads |

Rule: a Tier 2 result never auto-runs on a parameter change; the designer triggers it and the
result is stored against the input hash (WMDS-46).

## 2. Tier 0 - analytics

Implemented in `wmds-analytics`, pure functions over the model. Includes at minimum:

* Mass, CG, inertia tensor; kerb, laden and GVM states from a load-case table.
* Axle loads and weight distribution per load state; front/rear and left/right.
* Static stability factor (half-track / CG height), tip-over angle.
* Ride frequencies per axle from spring rate, motion ratio, sprung mass.
* Roll stiffness distribution and first-order roll gradient.
* Brake force distribution, ideal vs actual, adhesion-limited deceleration, pedal effort.
* Drivetrain: wheel torque per gear, tractive effort vs road load, gear-by-gear acceleration,
  top speed, gradeability; EV range from a drive cycle (WLTC) using motor and battery maps.
* Steering: Ackermann geometry, turning circle, steering ratio.
* Chassis as beam: rail bending and torsional stiffness from section properties and material;
  first bending and torsion modes from a two-rail-plus-cross-members stick model. Reported as
  estimates only.
* Port load estimates: static loads plus 3 g bump, 1 g brake, 1 g corner cases resolved through
  the mate graph as a static frame.

## 3. Tier 1 - real-time vehicle dynamics

### 3.1 Model derivation

`wmds-kinematics` converts the mate graph into a multibody tree:

* Bodies: chassis (all sections rigidly joined, or flexible via Tier 2 modes later), each
  suspension link, upright, wheel, steering rack, engine (as mass), payload masses.
* Joints: from mate DOF (revolute bushes, spherical balljoints, prismatic strut sliders).
* Force elements: springs, dampers, anti-roll bars, bump stops, bush stiffness from behaviour
  models.
* Tyres: Pacejka Magic Formula (MF 5.2 or 6.1 `.tir` files) or a brush model when no `.tir` is
  available; combined slip; simple relaxation length.
* Drivetrain: engine or motor torque map, clutch or torque converter, gear ratios, differential
  (open, locked, LSD with preload and ramp), driveline compliance as one torsional spring.
* Brakes: pedal to line pressure via master cylinder and booster, caliper torque, simple ABS.
* Steering: rack ratio, column compliance, optional assist.
* Driver: closed-loop path follower and speed controller for manoeuvres; human input for
  interactive driving.
* Road: flat with friction map, ISO 8608 stochastic profiles for ride, defined tracks for
  manoeuvres.

### 3.2 Solver

**ASSUMPTION:** an in-house reduced-coordinate (or constraint-based) multibody integrator in Rust
at 1 kHz with implicit handling of stiff bush and tyre elements, plus a `PhysicsBackend` trait so
that Rapier (Rust rigid-body engine) can be used for interactive driving and as a cross-check.
Rationale is in doc 08. Vehicle dynamics is a well-documented domain and a purpose-built solver is
smaller and more controllable than adapting a game engine, but Rapier gets an interactive
prototype running sooner.

### 3.3 Standard manoeuvres (shipped as `.case.kdl`)

| Case | Standard | Outputs |
|------|----------|---------|
| Constant radius | ISO 4138 | understeer gradient, roll gradient, sideslip |
| Step steer | ISO 7401 | yaw rate rise time, overshoot, lateral acceleration response |
| Sine with dwell | FMVSS 126 / UN R13-H ESC | yaw stability ratios, lateral displacement |
| Double lane change | ISO 3888-1/2 | pass/fail at speed, peak roll, tyre load transfer |
| Straight-line braking | ADR 31 style | stopping distance, MFDD, pedal force, stability |
| Ride | ISO 8608 road classes A-E at set speeds | sprung mass RMS acceleration, damper velocities, load histories |
| Hill start / gradeability | | |
| Fatigue extraction | mixed road profile | mate load histories for Tier 2 fatigue |

### 3.4 Interactive mode

The 3D viewport hosts the same model with keyboard or gamepad input. Telemetry overlay: speed,
slip angles, tyre loads, roll, G-G diagram. It is a design tool, not a game; there is no scoring.

### 3.5 Crash pre-screen (Tier 1)

A one-dimensional lumped-mass model: chassis sections and crash structures as masses connected by
nonlinear crush springs (force-displacement curves from the crash-structure primitives, which in
turn come from Tier 2 component tests or supplier data), occupant as a mass on a restraint spring.
Outputs crash pulse, peak g, ride-down, crush distance. Runs in under a second. Used to size crash
structures before committing to Tier 2.

## 4. Tier 2 - finite element

### 4.1 Pipeline

```
assembly graph
  -> select load-path parts (structural tag) and mass envelopes (non-structural)
  -> geometry at 'manufacture' level for structural parts
  -> mid-surface extraction for thin-wall CFRP and sheet parts; solids for castings/inserts
  -> meshing (Gmsh via API): shells 5 to 10 mm, solids where needed, mesh quality report
  -> material cards from the material database per solver
  -> connections: bonded (tied contact), bolted (beam elements with pretension), inserts
  -> load case: boundary conditions, barrier, initial velocity, gravity, occupant masses
  -> solver deck written by the FeSolver adapter
  -> external solver run (local, or on a remote machine over SSH)
  -> results parsed: displacements, intrusion at defined points, section forces, energy per part,
     pulse at defined accelerometer nodes, failure/damage per element
  -> summary stored against input hash; full results kept in results/
```

### 4.2 Solvers

| Purpose | Solver | Licence | Notes |
|---------|--------|---------|-------|
| Explicit crash | **OpenRadioss** | AGPL | Open-source release of Altair Radioss; reads LS-DYNA keyword decks as well as Radioss format; composite laws LAW25 (Tsai-Wu / CRASURV progressive damage) and fabric laws; the open-source crash workflow of choice. First adapter. |
| Implicit static and modal | **CalculiX** (ccx) | GPL | Mature, Abaqus-like input; used for torsional stiffness, bending, modes, bolt loads. Second adapter. |
| Alternative implicit | Code_Aster | GPL | Reserve. |
| Commercial | LS-DYNA, Abaqus, Radioss | commercial | Deck writers can be added later; keyword compatibility with OpenRadioss makes LS-DYNA nearly free. |

Solvers are external processes invoked through the `FeSolver` trait (doc 02). WMDS bundles none
of them; it detects installed solvers and offers an install guide. AGPL and GPL solvers run as
separate processes and exchange files, which keeps WMDS licensing independent.

### 4.3 Crash load cases (shipped)

Defined by the rule packs that need them; shipped as `.case.kdl` under `loadcases/crash/`:

* Full-width frontal, rigid barrier, 48 km/h and 56 km/h.
* Offset deformable barrier, 40 % overlap, 64 km/h.
* Small-overlap rigid, 25 %, 64 km/h (screening).
* Side, moving deformable barrier, 50 km/h.
* Pole side, 32 km/h.
* Rear, moving barrier, 50 km/h (fuel system integrity).
* Roof crush, quasi-static, 1.5 and 3 x kerb mass.
* Rollover, dolly or corkscrew (screening).

Each defines barrier, velocity, mass state, accelerometer nodes, intrusion measurement points
(defined relative to seat reference point), and pass thresholds used for internal screening.

### 4.4 Composite modelling rules

* Every CFRP part in a crash load path uses a progressive-damage law (OpenRadioss LAW25 CRASURV
  or later equivalent) with parameters from the material database. The internal rule pack fails a
  crash case that uses an elastic card for CFRP.
* Crush initiators (triggers) must be present in the geometry of any CFRP crash structure.
* Bonded joints get a cohesive or tied-with-failure definition; joint failure is reported.
* Material parameters for pultruded rails are to be calibrated against coupon and crush-tube tests
  as they become available; until then results are labelled "uncalibrated material".

### 4.5 Structural cases

Torsional stiffness (rear fixed, front couple), bending (four-point), modal (free-free, first six
flexible modes), section-joint local loads from Tier 1 manoeuvre extremes, insert pull-out and
bearing, fatigue from Tier 1 load histories with the material S-N data.

## 5. Validation plan

Simulation without validation is decoration. Each tier ships with validation cases whose expected
values are stored and re-checked in CI:

| Tier | Case | Source of truth |
|------|------|-----------------|
| 0 | Mass and CG of a test assembly of known solids | hand calculation |
| 0 | Brake distribution for a published example | textbook (Gillespie, Milliken) |
| 1 | Steady-state cornering of a bicycle model | analytic solution |
| 1 | Quarter-car ride over a step | analytic solution |
| 1 | Published vehicle constant-radius test | manufacturer or literature data |
| 2 | Aluminium crush tube force-displacement | published test |
| 2 | CFRP crush tube | ACS-A or literature; then own coupon tests |
| 2 | Chassis torsional stiffness | physical measurement of first MCDSv1 prototype |

Once a physical MCDSv1 prototype exists, its measured mass, torsional stiffness and a low-speed
sled or drop test become the anchor cases and the simulation is tuned to them.

## 6. What is deliberately not simulated in v1

Aerodynamics (a drag coefficient and frontal area are parameters, not computed), thermal
management beyond steady-state heat rejection, NVH beyond first modes, occupant injury criteria
requiring dummy models (intrusion and pulse are proxies; dummies are a later addition once the
pipeline is trusted).
