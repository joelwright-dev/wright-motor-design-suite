# 13 - Status against the brief

The founding brief, requirement by requirement, against what actually exists. This document
exists because the work drifted into polishing one corner while most of the brief sat untouched,
and nothing in the repository made that visible at a glance.

Updated 13 September 2026. Percentages are deliberately absent; "started" and "not started" are
harder to fool yourself with.

## The brief

> WMDS must contain a library of primitives used in every vehicle, ensure vehicles comply with
> regulations, analyse crash and driving dynamics with realistic simulators for both, allow easy
> construction of new primitives and MCDS without additional programming, and support complex
> mounting and modularity between components. It must cover design and manufacturing, making it
> easy to export instructions for assembly and manufacture of parts. All components must be
> incorporated: chassis, body, drivetrains, functions.
>
> MCDS must be versatile, lightweight, durable, cheap, easy to manufacture at multiple scales,
> and easy to assemble, such that anyone who can assemble a flatpack shelf can assemble a
> complete vehicle.

## Where each pillar actually stands

| # | Pillar | State | The gap |
|---|--------|-------|---------|
| 1 | Library of primitives for every vehicle | **Thin** | 17 primitives. No body, interior, seats, lights, glazing, springs, dampers, anti-roll bars, driveshafts, hubs as separate parts, fuel system, internal combustion powertrain, cooling, HVAC, wiring, or pedals. |
| 2 | Regulatory compliance | **Half** | The engine is real and honest. The Australian Design Rules pack is a five-rule unverified template. Nobody has read the actual standards against it. |
| 3 | Realistic driving dynamics simulator | **Started** | A transient four-wheel model with a Magic Formula tyre, load transfer split by roll stiffness, and a friction ellipse. Skidpad, step steer, braking, acceleration and a double lane change. No suspension kinematics and no springs, so roll stiffness is assumed and the report says so. |
| 4 | Realistic crash simulator | **Not started** | Nothing at all. No solver, no deck export, no material cards for crash. |
| 5 | Build primitives without programming | **Started** | The application edits a part: dimensions, shapes and mounting points, with the geometry rebuilding as you go, and writes the file. Chassis systems still have no editor. |
| 6 | Build new MCDS chassis without programming | **Not started** | Chassis systems are data rather than code, which is the hard half, but there is no editor for them. |
| 7 | Complex mounting and modularity | **Good** | Typed ports, expression compatibility rules, a placement solver over the mate graph, closure checking, variant mirroring, sub-assemblies. The strongest part of the suite. |
| 8 | Design surface | **Started** | Vehicles and parts are both edited in the application. A joint can be moved to any other port that fits, and a part can be placed explicitly. Still no undo. |
| 9 | Manufacturing export | **Started** | STEP, plus a bill of materials, a cut list and a manufacturing route chosen for the build volume with costs rolled up. No nesting and no per-part drawings. |
| 10 | Assembly instructions | **Started** | `wmds build` writes numbered steps in an order that is buildable by construction, sub-assemblies first, each with its fasteners and torque. No pictures yet, which a flatpack needs. |
| 11 | All components incorporated | **No** | Chassis, suspension, steering, braking, battery and drive unit exist. Body, interior and everything a person touches do not. |

## MCDS itself

| Requirement | State |
|---|-------|
| Versatile: different styles, purposes, sizes | Two length configurations and two widths, generated from data. Front and central sections only; the rear section kind is defined but no vehicle uses one. |
| Lightweight | Rails and cross-members are modelled; the floor, inserts, joint fittings and brackets are not, so the chassis mass is optimistic and says so. |
| Durable | No fatigue model, no corrosion model, no structural analysis at all. |
| Cheap | Cost figures exist per manufacturing method and are provisional. Nothing rolls them up into a vehicle cost. |
| Easy to manufacture at several scales | Each primitive declares methods with scale ranges and costs. Nothing selects between them or reports which would be used. |
| Assemblable by anyone who can build a flatpack shelf | Fasteners, torques and kit-versus-factory stages are in the model. No instructions are produced. |

## The order the gaps get closed

Chosen so each one unblocks the next, and so the parts of the brief with nothing behind them
stop having nothing behind them.

1. ~~Primitive authoring in the application~~ (pillar 5). Done for parts; chassis systems are
   still next.
2. ~~Joint editing and free placement~~ (pillar 8). Done.
3. ~~Manufacturing and assembly output~~ (pillars 9, 10). Done as text and Markdown; drawings
   and pictures are what is missing.
4. ~~Driving dynamics~~ (pillar 3). Done, except that it has no springs or suspension
   kinematics to read, which is what the next item unblocks.
5. **Springs, dampers and anti-roll bars in the library**, so roll stiffness stops being an
   assumption and becomes a result of the design.
6. **Crash** (pillar 4). Explicit finite element is the only honest answer for a real crash
   result. The plan is a deck exporter for OpenRadioss plus a lumped-mass nonlinear-spring model
   for early-phase work, clearly labelled as the screening tool it is.
7. **The rest of the library** (pillars 1, 11). Body, interior, springs and dampers,
   driveshafts, lights, glazing, pedals, wiring.
8. **The Australian Design Rules pack read against the actual standards** (pillar 2).

## Rules for this document

It is updated when a pillar moves, not when a task finishes. A pillar moves to **Started** when
something real exists behind it and to **Good** when it would survive someone competent using it
in anger. Nothing moves because it is nearly done.
