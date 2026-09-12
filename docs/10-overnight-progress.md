# 10 - Progress notes, 12 to 13 September 2026

What was built while you were asleep, what it does, what it found, and what needs your decision.

## What to run first

From the repository root. Add `--no-default-features` to any of these to skip the OpenCASCADE
build and use the mesh kernel, which is much faster to compile and slightly less accurate.

```bash
cargo run -- veh show vehicles/reference-city-ev/reference-city-ev.veh.kdl --build
cargo run -- check vehicles/reference-city-ev/reference-city-ev.veh.kdl
cargo run -p wmds-app -- vehicles/reference-city-ev/reference-city-ev.veh.kdl
```

The last one opens the viewer on the reference vehicle. The first two print the vehicle and its
compliance report.

To see the modular chassis do the thing it exists to do, change a section length and watch
everything behind the joint plane stay put:

```bash
cargo run -- chassis show mcds-v1 --config full-length --width wide --section front=1500mm --section central=2800mm --section rear=2200mm --build
```

## What was built

**Mates and the placement solver.** Components connect through typed ports. One part is held
fixed and every other part's position is solved by walking the mate graph outward. Nothing is
positioned by hand. This is what makes a chassis change ripple through a vehicle instead of
leaving parts floating where someone last dragged them.

**The port registry** (`library/ports.kdl`). Twenty port types, each declaring its parameters,
its degree of freedom, and an expression deciding what it is compatible with. Adding a new
joining standard is editing this file. Bolting a high voltage connector to a chassis rail is
refused, and there is a test proving it.

**The MCDSv1 chassis generator.** `chassis/mcds-v1/mcds-v1.chassis.kdl` describes the platform:
section kinds and their length ranges, width options, rail sections, the mount grid. A generic
generator reads it and produces rails, cross-members, section-joint ports and every grid station.
There is no MCDS-specific code anywhere. A second chassis family is another file.

**The reference vehicle.** A small EV on a 2/3-length narrow chassis, with the battery and drive
unit bolted to grid stations. Everything not yet modelled is carried as labelled point masses, so
the mass figure is honest rather than absent.

**The compliance engine.** Rule packs are data. `wmds check` says what passes, what fails, what
needs a simulation and what needs a physical test. The reference vehicle currently passes nine
rules, needs one simulation that Phase 5 will provide, and needs one physical test that no
simulation can replace.

**The viewer** now opens whole vehicles, listing every part with its position and mass, marking
which masses were declared rather than derived, and drawing the centre of gravity.

## Three bugs the reference vehicle exposed

Building a real vehicle found things unit tests had not.

**Hyphenated words parsed as subtraction.** `family="amphenol-surlok"` was read as one name minus
another. Text that parses as an expression, cannot be evaluated, and contains no numbers is now
treated as text. Anything containing a number still errors, so a mistyped parameter in
`"(spann / 2, 0, 0)"` is still caught rather than silently becoming a string.

**Ports without a clocking direction were rotated arbitrarily.** A port declares a mating axis,
but that leaves rotation about the axis undefined. The battery pack came out rotated ninety
degrees. Ports now declare `clock=`, and a rigid mate whose ports do not warns.

**Declared mass was ignored.** Envelope models are drawn as boxes for packaging and declare their
real mass separately. The mass roll-up was using the box volume and a placeholder density, which
made a 40 kWh battery weigh whatever a 1400 by 900 by 150 mm block of plastic weighs. Declared
mass now wins, and reports say which is which.

## Decisions waiting for you

**1. Where should the battery sit?** The reference vehicle bolts the pack to the top of the
rails, because that is what a grid station is: a mounting point on the rail top surface. For an
EV you almost certainly want the pack in the floor, between the rails, for centre of gravity
height. That needs either a second port type for a between-rails mount or a different pack
geometry that straddles the rails. It is a platform design question, not a software one.

**2. The ADR rule pack needs the standards read against it.** `rules/adr-icv-template.rules.kdl`
is a template. Every rule in it is deliberately unverified and every limit is a placeholder, so
the report shouts NOT VERIFIED at you. Turning it into a real pack means sitting down with the
Australian Design Rules and VSB 14 and writing what they actually require. Doc 05 also flags an
open question that needs regulatory advice: whether a customer-assembled MCDS vehicle is an
individually constructed vehicle or needs low-volume type approval, and whether that differs by
state.

**3. Provisional numbers that want engineering.** The chassis file marks every one. Grid pitch of
100 mm, section length ranges, rail wall thicknesses, the 15 percent chassis mass fraction, the
700 mm centre of gravity limit. They exist so the software has something to check against; none
of them is derived from anything yet.

**4. Cross-member spacing looks wrong at short lengths.** The generator puts one cross-member at
each section joint and then spaces the rest evenly, never further apart than 600 mm. On the
1100 mm front section that gives three. Whether that is right is a structural question.

## Where the numbers stand

For the reference vehicle, from `wmds veh show --build`:

| Quantity | Value | Confidence |
|----------|-------|------------|
| Chassis mass | 27.9 kg | Rails and cross-members only. No inserts, joint fittings, brackets or floor. |
| Battery | 250 kg | Declared, from 40 kWh at a provisional 160 Wh/kg pack level |
| Drive unit | 50 kg | Declared, from 110 kW at a provisional 2.2 kW/kg |
| Modelled total | 327.9 kg | |
| Declared point masses | 673 kg | Estimates for everything not yet modelled |
| Kerb, roughly | 841 kg | |
| Centre of gravity | 912, 0, 78 mm | Modelled parts only; the check folds in kerb point masses |

Treat all of it as a scaffold that is the right shape, not as a mass estimate.

## What I would do next

1. **A suspension corner.** Upright, hub, arms, spring and damper, wheel and tyre, assembled as a
   sub-assembly with exported ports. It is the hardest remaining test of the mate graph, because
   a corner is a closed kinematic loop rather than a tree, and it unlocks wheelbase, track, axle
   loads and everything in Tier 0 that depends on them.
2. **The materials database.** Every density in the system is currently a placeholder keyed off a
   material name prefix. Real material files would make every mass figure meaningful.
3. **Tier 0 analytics.** Axle loads, weight distribution, static stability factor, braking
   distribution. Most of it needs the suspension corner first.
