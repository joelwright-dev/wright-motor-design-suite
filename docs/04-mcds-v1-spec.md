# 04 - MCDSv1 Specification

This document describes the first Modular Chassis Design System platform as WMDS must model it.
It is a software-facing specification: it fixes the concepts, interfaces and parameters the
software needs, and records engineering targets so that compliance and analytics have something to
check against. Detailed structural engineering of the chassis is a separate workstream that will
use WMDS itself.

## 1. Concept

MCDSv1 is a carbon-fibre-reinforced-polymer (CFRP) ladder frame built from up to three
longitudinal sections joined end to end:

```
   +-----------+   +------------------------+   +--------------+
   |   FRONT   |===|        CENTRAL         |===|     REAR     |
   | powertrain|   |         cabin          |   | tray / cargo |
   +-----------+   +------------------------+   +--------------+
        ^ section joint                  ^ section joint
```

* **Front section** carries the engine or EV drive unit, front suspension, steering, cooling,
  front crash structure.
* **Central section** carries the cabin: floor, seats, restraints, battery pack (EV), fuel tank
  (ICE), side-impact structure.
* **Rear section** carries rear suspension, rear drive (if any), tray, load area, SUV body, rear
  crash structure, tow hitch.

Two length configurations:

| Configuration | Sections | Typical vehicles |
|---------------|----------|------------------|
| **2/3-length** | front + central | city car, small hatch, roadster |
| **Full-length** | front + central + rear | sedan, wagon, SUV, ute, light truck |

**ASSUMPTION:** the brief describes these as "2/3 width" and "full width" in one place and as
lengths in the next sentence. This spec treats the two standard configurations as **lengths**
(section count) and treats **width** as a separate, independent option (section 3.3). If width
configurations were intended instead, the port parameter `width_config` already covers it and
only this paragraph changes.

Every section of a given type conforms to the same section-joint interface, so any front section
mates to any central section, and any central to any rear (MCDS-07).

## 2. Coordinate system and datums

* Origin: intersection of the vehicle centreline, the front section-joint plane, and the rail
  top surface. This point exists in every configuration.
* X positive rearward, Y positive to the left, Z positive up (ISO 8855 vehicle axes).
* **Grid stations** are numbered from the front joint plane: station 0 at the joint, positive
  rearward into the central section, negative forward into the front section. The rear joint
  plane is at station `L_central / pitch`.

## 3. Parameters

### 3.1 Platform constants (fixed for all MCDSv1 vehicles)

| Parameter | Value | Note |
|-----------|-------|------|
| Mount grid pitch | **100 mm** | **ASSUMPTION.** Coarser is cheaper (fewer inserts); finer is more flexible. 100 mm matches common component spacing and keeps insert count reasonable. Revisit after the first three vehicle layouts. |
| Rail cross-section family | closed rectangular box, pultruded | constant section along each rail is what makes pultrusion (the cheap CFRP process) applicable |
| Section-joint interface | see section 4 | single definition, all sections |
| Rail top surface | datum Z = 0 | body and floor reference |

### 3.2 Section parameters

| Parameter | Front | Central | Rear | Unit |
|-----------|-------|---------|------|------|
| Length | 900 to 1500 | 1600 to 2800 | 800 to 2200 | mm, on 100 mm pitch |
| Rail spacing (inner) | from width config | from width config | from width config | mm |
| Rail height | 120 or 160 | 120 or 160 | 120 or 160 | mm |
| Rail width | 60 or 80 | 60 or 80 | 60 or 80 | mm |
| Cross-member count | derived: one per 400 to 600 mm, plus one at each joint | | | |
| Kick-up | optional, at rear of central section, 0 to 150 mm | | | mm |

Ranges are **ASSUMPTION** placeholders to be replaced once the first packaging studies are done
in WMDS. They exist so the software has bounds to validate against from day one.

### 3.3 Width configurations

| Config | Inner rail spacing | Overall vehicle width target | Use |
|--------|--------------------|------------------------------|-----|
| `narrow` | 900 mm | 1600 to 1750 mm | city car, kei-class, roadster |
| `standard` | 1050 mm | 1750 to 1900 mm | hatch, sedan, small SUV |
| `wide` | 1200 mm | 1900 to 2100 mm | large SUV, ute, light truck |

Width is a property of the whole vehicle: all sections must share it (enforced by the section-joint
compatibility rule).

## 4. Section-joint interface

This is the most important definition in MCDS. It is defined once, in
`chassis/mcds-v1/ports.kdl`, and every section implements it.

```kdl
port_type "mcds.section-joint" {
    params {
        width_config  "string"     // narrow | standard | wide
        rail_section  "string"     // e.g. "160x80"
        generation    "int"        // 1
    }
    dof "fixed"
    compatible_with "mcds.section-joint" \
        when="a.width_config == b.width_config and a.rail_section == b.rail_section and a.generation == b.generation"
    mate_stage "kit"
    symmetry 1
}
```

Physically the joint is, per rail: a bonded-in metallic end fitting (factory stage) on each
section, mated with a bolted flange or sleeve joint (kit stage). The end fitting carries the
bending moment and shear across the joint; the bolts carry the tension and clamp.
**OPEN:** flange (easier for a flatpack assembler, adds height) vs internal sleeve (cleaner, needs
alignment tooling). The software models both as the same port; the choice affects only the
end-fitting primitive.

Joint design loads are recorded on the port as `load_rating` and are checked by Tier 0 beam
analysis and Tier 2 FE.

## 5. Structure and materials

### 5.1 Rails

Pultruded CFRP box section. Pultrusion is chosen because it is the lowest-cost continuous CFRP
process, produces a constant cross-section (which is exactly what a ladder rail is), and can be
cut to length on the 100 mm grid with no change in tooling. This is the mechanism by which the
platform satisfies "cheap" and "length adjustable" at the same time (MCDS-04, MCDS-08).

**ASSUMPTION:** layup is predominantly unidirectional 0 degree fibres for bending stiffness with
±45 degree layers for torsion and bolt bearing. The exact schedule is an engineering output, held
in the rail primitive's ply book.

### 5.2 Cross-members

Options, all modelled as primitives: pultruded CFRP tube, aluminium extrusion, or steel tube.
Cross-members attach to rails through bonded-in metallic inserts at grid stations. Kit-stage
attachment is bolted.

### 5.3 Inserts and the mount grid

Every grid station on a rail is a potential insert location. Inserts are metallic (aluminium or
stainless), bonded in at the factory, threaded (M10 or M12, **OPEN**). A rail is manufactured with
inserts only at the stations the vehicle design uses, plus a standard set (joints, cross-members).
Isolation between aluminium inserts and carbon is mandatory (galvanic corrosion, MCDS-03).

### 5.4 Crash structure

CFRP is stiff and strong but brittle. Crushing CFRP can absorb a great deal of energy per kilogram
when it is designed to crush progressively, and it can absorb almost none when it fractures
instead. This is the central engineering risk of a CFRP ladder frame and it shapes the software
requirements:

* Front and rear sections carry **dedicated, replaceable crash structures** (crush cans or crash
  boxes) forward of and behind the rail ends. **ASSUMPTION:** these are aluminium extrusion or
  progressive-crush CFRP tubes with triggers, bolted to the rail end fittings so they are
  replaceable after an impact without replacing the rail.
* The central section rails are designed **not** to be the primary energy absorber; they form the
  survival cell with the side-impact structure.
* Tier 2 crash simulation must use a composite material law with progressive damage
  (OpenRadioss LAW25 CRASURV or equivalent); an elastic material card is not acceptable for
  crash load cases on CFRP parts. This is a hard rule in the crash pipeline (doc 06).

### 5.5 Durability

Design life target: **ASSUMPTION** 300 000 km / 20 years, with a fatigue spectrum derived from
Tier 1 ride simulation over ISO 8608 class C to E road profiles. Environmental: UV protection of
exposed CFRP is mandatory (coating or cover), stone-impact protection on underside, galvanic
isolation at every metallic interface. Each of these becomes a compliance-style check in an
internal rule pack (`rules/wright-internal`).

## 6. Engineering targets (placeholders)

These are targets, not specifications. WMDS will report against them so the numbers are honest
from the first prototype onward.

| Target | Value | Requirement |
|--------|-------|-------------|
| Bare chassis mass, full-length standard width, 160x80 rails | ≤ 120 kg | MCDS-02 |
| Bare chassis mass, 2/3-length narrow | ≤ 70 kg | MCDS-02 |
| Torsional stiffness, full-length standard | ≥ 12 kN·m/deg with floor bonded, ≥ 6 without | ride and handling |
| Chassis unit cost at 100 units/yr | ≤ 6 000 AUD | MCDS-04 |
| Chassis unit cost at 5 000 units/yr | ≤ 2 500 AUD | MCDS-04 |
| Kit assembly time for chassis, two people, hand tools | ≤ 4 hours | MCDS-06 |
| Design life | 300 000 km / 20 years | MCDS-03 |

## 7. Chassis definition file

The chassis system is data (WMDS-20). Sketch of `chassis/mcds-v1/mcds-v1.chassis.kdl`:

```kdl
chassis "mcds-v1" version="0.1.0" {
    grid_pitch "100 mm"
    axes "iso8855"
    datum "front-joint-plane, rail-top, centreline"

    width_configs {
        narrow   inner_rail_spacing="900 mm"
        standard inner_rail_spacing="1050 mm"
        wide     inner_rail_spacing="1200 mm"
    }

    rail_sections {
        "120x60" h="120 mm" w="60 mm" wall="6 mm" material="cfrp/pultruded-ud-t700-epoxy"
        "160x80" h="160 mm" w="80 mm" wall="7 mm" material="cfrp/pultruded-ud-t700-epoxy"
    }

    section_kinds {
        front   length_min="900 mm"  length_max="1500 mm" joints="rear"
        central length_min="1600 mm" length_max="2800 mm" joints="front rear"
        rear    length_min="800 mm"  length_max="2200 mm" joints="front"
    }

    configurations {
        "2/3-length"  sections="front central"
        "full-length" sections="front central rear"
    }

    // A section is a generator: given kind, length, width_config, rail_section it produces
    // rails, cross-members, joint fittings, and grid-station ports.
    section_generator "mcds-v1/section-generator"     // a primitive with a `generate` geometry block
}
```

A concrete section (e.g. `front-ice-std-1200`) is a primitive that references the generator with
fixed parameters and adds its specific brackets, mounts and crash structure.

## 8. Open engineering questions (tracked, not blocking)

1. Flange vs sleeve section joint (section 4).
2. Insert thread size and material (section 5.3).
3. Whether the floor is structural (bonded, contributes to torsion) or non-structural (bolted).
   Affects the kit-stage rule: a bonded floor must be factory stage.
4. Whether a CFRP rail can be made cheaply enough at 100 units/yr to beat an aluminium extrusion
   of equal stiffness. WMDS cost models should be able to answer this once populated; the
   platform definition allows a rail material swap without changing the interface.
5. Repair philosophy after a crash: replace crash structure only (target) vs replace section.
6. Tolerance stack-up across three sections and how the kit assembler aligns them without a jig.
   Candidate: joint fittings with self-locating tapers.
