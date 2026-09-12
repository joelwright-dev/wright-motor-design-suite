# 10 - Progress notes, 12 to 13 September 2026

What was built overnight, what it does, what building it found, and what needs your decision.

## What to run first

From the repository root. Add `--no-default-features` to any of these to skip the OpenCASCADE
build and use the mesh kernel, which compiles in seconds instead of half an hour and is slightly
less accurate.

```bash
cargo run -p wmds-app -- vehicles/reference-city-ev/reference-city-ev.veh.kdl
```

That opens the viewer on the reference vehicle: chassis, battery, drive unit and both front
corners, with a parts list, masses and the centre of gravity. Then:

```bash
cargo run -- veh show vehicles/reference-city-ev/reference-city-ev.veh.kdl --build
cargo run -- check vehicles/reference-city-ev/reference-city-ev.veh.kdl
cargo run -- veh show vehicles/reference-city-ev/reference-city-ev.veh.kdl --build --step car.step
```

The last of those fuses all 31 parts and writes a 2.6 MB STEP file that opens in NX or FreeCAD.

To watch the modular chassis do the thing it exists to do, ask for a different one. Nothing in
the library changes; only the numbers in the command:

```bash
cargo run -- chassis show mcds-v1 --config full-length --width wide --section front=1500mm --section central=2800mm --section rear=2200mm --build
```

## What was built

**Mates and the placement solver.** Components connect through typed ports, and where a part sits
is solved by walking the mate graph outward from one fixed part. Nothing is positioned by hand.
This is what makes a chassis change ripple through a vehicle instead of leaving parts floating
where someone last dragged them.

**The port registry** (`library/ports.kdl`). Twenty port types, each declaring its parameters, its
degree of freedom, and an expression deciding what it is compatible with. Adding a new joining
standard is editing that file. Bolting a high voltage connector to a chassis rail is refused, and
a test proves it.

**The MCDSv1 chassis generator.** `chassis/mcds-v1/mcds-v1.chassis.kdl` describes the platform:
section kinds and their length ranges, width options, rail sections, the mount grid. A generic
generator reads it and produces rails, cross-members, section-joint ports and every grid station.
There is no MCDS-specific code anywhere; a second chassis family is another file.

**Front suspension, and mirroring.** A double wishbone corner assembly: four brackets bolted to
grid stations, two wishbones, an upright, a wheel and a tyre. This is the first assembly with a
closed kinematic loop, since the upright is reached through the lower arm and the upper arm then
has to arrive at exactly the same place. A part can now declare that one of its variants is its
mirrored hand, so the right corner is the left one reflected rather than a second file to keep in
step.

**The compliance engine.** Rule packs are data. `wmds check` says what passes, what fails, what
needs a simulation and what needs a physical test. The design decision that matters: a rule the
engine cannot evaluate reports Undecided, never Pass. Silently passing a check the software could
not make would be the worst thing this feature could do.

**The material database.** Fifteen materials with density, elastic constants, strengths,
environmental notes, cost and per-solver cards. Every mass in the reference vehicle now comes from
it. Where something still has to guess a density, the report names the material.

**The viewer** opens whole vehicles, lists every part with position and mass, marks which masses
were declared rather than derived, and draws the centre of gravity.

## Four bugs that building a real vehicle exposed

Unit tests had not found any of these. Assembling an actual car did.

**Hyphenated words parsed as subtraction.** `family="amphenol-surlok"` was read as one name minus
another. Text that parses as an expression, cannot be evaluated, and contains no numbers is now
treated as text. Anything containing a number still errors, so a misspelled parameter in
`"(spann / 2, 0, 0)"` is still caught rather than silently becoming a string.

**Ports without a clocking direction were rotated arbitrarily.** A port declares a mating axis,
which leaves rotation about that axis undefined. The battery pack came out rotated ninety degrees.
Ports now declare `clock=`, and a rigid mate whose ports do not now warns.

**Declared mass was ignored.** Envelope models are drawn as boxes for packaging and declare their
real mass separately. The roll-up was using the box volume and a density, which made a 40 kWh
battery weigh whatever a 1400 by 900 by 150 mm block of plastic weighs. Declared mass now wins.

**An M14 ball joint stud in an M12 socket.** The port registry caught this one on its own, while
the corner was being assembled, and refused to place anything through the bad joint. The stud size
is now a parameter, since an upper arm uses a smaller joint than a lower. This is the system doing
exactly what it was built to do.

## Decisions waiting for you

**1. Where should the battery sit?** The reference vehicle bolts the pack to the top of the rails,
because that is what a grid station is: a mounting point on the rail top surface. For an electric
car you almost certainly want the pack in the floor, between the rails, for centre of gravity
height. That needs either a second port type for a between-rails mount or a pack that straddles
the rails. It is a platform design question, not a software one.

**2. The ADR rule pack needs the standards read against it.**
`rules/adr-icv-template.rules.kdl` is a template. Every rule is deliberately unverified and every
limit is a placeholder, so the report shouts NOT VERIFIED. Turning it into a real pack means
sitting down with the Australian Design Rules and VSB 14. Doc 05 also flags an open question that
needs regulatory advice: whether a customer-assembled MCDS vehicle is an individually constructed
vehicle or needs low-volume type approval, and whether that differs by state.

**3. Provisional numbers that want engineering.** The chassis file marks every one: the 100 mm
grid pitch, the section length ranges, the rail wall thicknesses. So does the material database:
every entry is provisional unless its `source` line says otherwise, and none of the composites is
calibrated. So do the internal rules: the 15 percent chassis mass fraction, the 700 mm centre of
gravity limit.

**4. Cross-member spacing.** The generator puts one at each section joint and spaces the rest
evenly, never more than 600 mm apart. On the 1100 mm front section that gives three. Whether that
is right is a structural question.

## Where the numbers stand

Reference vehicle, from `wmds veh show --build`:

| Quantity | Value | Confidence |
|----------|-------|------------|
| Chassis | 27.9 kg | Rails and cross-members only. No inserts, joint fittings or floor. |
| Front suspension, both corners | 44.8 kg | Brackets, arms, uprights, wheels and tyres |
| Battery | 250 kg | Declared, from 40 kWh at a provisional 160 Wh/kg pack level |
| Drive unit | 50 kg | Declared, from 110 kW at a provisional 2.2 kW/kg |
| Modelled total | 402.1 kg | Every density from the material database |
| Declared point masses | 588 kg | Estimates for what is not yet modelled |
| Kerb, roughly | 993 kg | |
| Centre of gravity | 596, -4, 61 mm | Modelled parts; the compliance check folds in kerb point masses |
| Front track | 1260 mm | |

Compliance: nine rules pass, one needs a simulation that Phase 5 will provide, one needs a
physical test that no simulation can replace.

The two geometry kernels disagree by about half a percent on mass, and the disagreement is the
expected one. The mesh kernel fuses parts by concatenating triangles, so where two solids overlap
(the arm tubes meeting at the ball joint boss, for instance) the shared volume is counted twice.
OpenCASCADE does the boolean properly and removes it. The mesh kernel reads 404.7 kg against
OpenCASCADE's 402.1 kg, which is the overlap. Trust the OpenCASCADE figure; the mesh kernel is
for looking at things quickly.

The lateral centre of gravity comes out 4 mm off centre on a vehicle that is geometrically
symmetric. That is tessellation asymmetry between a shape and its mirror image rather than a real
offset, and it is well inside the 30 mm the internal rule allows, but it is worth chasing when
somebody next touches the mirroring code.

Treat all of it as a scaffold that is the right shape rather than as a mass estimate.

## What I would do next

1. **Rear suspension and a rear axle.** That gives wheelbase, which unlocks axle loads, weight
   distribution and static stability factor, which is most of Tier 0.
2. **Steering and brakes.** The corner already has a steering arm socket waiting for a tie rod.
3. **Tier 0 analytics** as a proper crate, once the geometry above exists to feed it.
4. **The bill of materials.** Everything needed is already in the model: parts, quantities,
   materials, costs per manufacturing method and fasteners on every mate. It is mostly a matter
   of walking the assembly and writing it out.
