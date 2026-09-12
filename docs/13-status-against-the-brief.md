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
| 3 | Realistic driving dynamics simulator | **Not started** | Tier 0 closed-form numbers only: wheelbase, track, axle loads, static stability. No tyre model, no transient solver, no manoeuvres. |
| 4 | Realistic crash simulator | **Not started** | Nothing at all. No solver, no deck export, no material cards for crash. |
| 5 | Build primitives without programming | **Not started** | A new part means writing KDL by hand. This is a named requirement and it is entirely unmet in the application. |
| 6 | Build new MCDS chassis without programming | **Not started** | Chassis systems are data rather than code, which is the hard half, but there is no editor for them. |
| 7 | Complex mounting and modularity | **Good** | Typed ports, expression compatibility rules, a placement solver over the mate graph, closure checking, variant mirroring, sub-assemblies. The strongest part of the suite. |
| 8 | Design surface | **Started** | The editor builds vehicles: chassis, catalogue, joints by picking ports, save. No undo, no way to change an existing joint, no free placement, no primitive editing. |
| 9 | Manufacturing export | **Started** | STEP of the whole vehicle or any part. No bill of materials, no cut list, no nesting, no per-part drawings, no cost roll-up in a usable form. |
| 10 | Assembly instructions | **Not started** | The model holds every fastener, torque and assembly stage. None of it comes out as instructions. This is the flatpack promise and there is nothing behind it yet. |
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

1. **Primitive and chassis authoring in the application** (pillars 5, 6). Until a part can be
   made without writing a file, the library cannot grow except by me, and requirement 5 is
   simply unmet. This also unblocks pillar 1.
2. **Joint editing and free placement in the editor** (pillar 8). Changing where something bolts
   is the most basic design act and it currently requires editing a file.
3. **Manufacturing and assembly output** (pillars 9, 10). Every input already exists in the
   model. Bill of materials, cut lists, fastener schedule, and step-by-step assembly
   instructions ordered by the mate graph.
4. **Driving dynamics** (pillar 3). A real transient model: sprung and unsprung masses,
   suspension rates from the geometry, a Pacejka tyre model, and standard manoeuvres.
5. **Crash** (pillar 4). Explicit finite element is the only honest answer for a real crash
   result. The plan is a deck exporter for OpenRadioss plus a lumped-mass nonlinear-spring model
   for early-phase work, clearly labelled as the screening tool it is.
6. **The rest of the library** (pillars 1, 11). Body, interior, springs and dampers,
   driveshafts, lights, glazing, pedals, wiring.
7. **The Australian Design Rules pack read against the actual standards** (pillar 2).

## Rules for this document

It is updated when a pillar moves, not when a task finishes. A pillar moves to **Started** when
something real exists behind it and to **Good** when it would survive someone competent using it
in anger. Nothing moves because it is nearly done.
