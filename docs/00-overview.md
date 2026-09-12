# 00 - Overview

## 1. Vision

Design a complete road vehicle the way you would configure a product on a website: choose a
chassis length, drop in a drivetrain, pick a body, and have the software tell you what it weighs,
whether it is legal, how it drives, how it crashes, and how to build it.

WMDS exists because the MCDS platform makes that possible in hardware. A modular chassis with
standardised mounting interfaces means a finite library of parts can produce an open-ended range of
vehicles. The software's job is to make that combinatorial space safe to explore: every combination
is checked, simulated and documented automatically.

## 2. What WMDS is

A desktop application, with a scriptable core, that covers the full vehicle lifecycle from concept
to assembly instructions:

1. **Library** - a catalogue of parametric primitives (suspension, engine, steering, fuel, braking,
   drivetrain, electrical, body, interior, chassis sections) with geometry, mass properties,
   material data, behaviour models and mounting interfaces.
2. **Design** - assemble primitives into a vehicle by connecting compatible ports. Parametrise the
   chassis, adjust track, wheelbase, ride height, gearing.
3. **Check** - continuous compliance checking against configurable regulation packs (Australian
   Design Rules first, then FMVSS and UNECE).
4. **Simulate** - driving dynamics (handling, braking, ride, stability) and crash (frontal, side,
   rear, rollover) with a realistic physics basis and honest labelling of fidelity.
5. **Manufacture** - export cut files, ply schedules, CNC-ready geometry, bills of materials, and
   step-by-step assembly instructions in the style of flatpack furniture.
6. **Extend** - new primitives, new chassis systems and new rule packs are added by writing
   definition files, not by writing Rust.

## 3. What WMDS is not

* Not a general-purpose CAD package. It will never compete with SolidWorks or FreeCAD on freeform
  modelling. Freeform work happens elsewhere and is imported as a primitive.
* Not a certification authority. It produces evidence and reports; a human signatory certifies.
* Not a game. The driving simulator prioritises correct physics over graphics.

## 4. Users

| User | Needs |
|------|-------|
| Vehicle designer (Wright Motor Company) | Fast iteration, trustworthy numbers, one source of truth for the whole vehicle |
| Component designer | Add a new engine, axle, or body panel to the library without touching the application code |
| Manufacturing engineer | Cut files, ply books, jig drawings, BOM with supplier part numbers |
| Compliance engineer | Traceable rule-by-rule report with evidence and the list of physical tests still required |
| Kit assembler (end customer) | Printable and interactive assembly instructions, fastener kits, torque values, no engineering knowledge assumed |

## 5. Scope boundaries for version 1

In scope: everything structural and functional. Chassis, body-in-white, closures, suspension,
steering, brakes, drivetrain (ICE, EV, hybrid), fuel and cooling, electrical architecture at the
harness-routing level, wheels and tyres, lighting, glazing, seats and restraints.

Simplified: interior trim, HVAC ducting, infotainment, paint. These are modelled as mass, volume and
cost envelopes with mount points, not as detailed geometry.

Out of scope for version 1: ECU software, powertrain calibration, homologation submission tooling.

## 6. Glossary

| Term | Meaning |
|------|---------|
| **Primitive** | A parametric, reusable component definition in the library. An engine model, a control arm, a chassis rail. |
| **Instance** | A primitive placed in a vehicle with specific parameter values. |
| **Port** | A named, typed mounting interface on a primitive (bolt pattern, hose fitting, electrical connector, structural joint). Ports are how things connect. |
| **Mate** | A connection between two compatible ports. |
| **Assembly** | A tree of instances joined by mates. A vehicle is the root assembly. |
| **MCDS** | Modular Chassis Design System - the physical chassis platform family. |
| **MCDSv1** | The first platform: carbon-fibre, length-adjustable ladder frame. See doc 04. |
| **Section** | One of the three longitudinal chassis modules: front, central, rear. |
| **Mount grid** | The standard pitch of mounting locations along a chassis rail. Every port on the chassis sits on the grid. |
| **Rule pack** | A versioned set of compliance rules for one jurisdiction or standard. |
| **Evidence** | The thing that proves a rule is met: a calculation, a simulation result, a physical test certificate, or a declaration. |
| **Fidelity tier** | Level of simulation detail: Tier 0 analytic, Tier 1 real-time multibody, Tier 2 finite element. |
| **ICV** | Individually Constructed Vehicle, the Australian registration category for one-off and kit-built vehicles. |
| **ADR** | Australian Design Rules, the national vehicle standards. |
| **BOM** | Bill of materials. |
| **Ply book** | The layer-by-layer layup schedule for a composite part. |
