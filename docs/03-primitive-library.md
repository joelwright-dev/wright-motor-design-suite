# 03 - Primitive Library, Ports and Mounting

## 1. Purpose

The library is what makes WMDS a vehicle design tool rather than a CAD tool. This document defines
the component taxonomy, the primitive definition format, and the port system that makes
components mount to each other and to the chassis.

## 2. Taxonomy

Categories are fixed by the schema; primitives within them are open-ended.

| Category | Sub-categories (initial) | Behaviour model |
|----------|--------------------------|-----------------|
| `chassis` | section, rail, crossmember, section-joint, subframe, crash-structure | stiffness, crush curve |
| `suspension` | control-arm, multilink, strut, trailing-arm, leaf, upright, hub, spring, damper, arb, bump-stop, bush, balljoint | spring curve, damper curve, bush stiffness |
| `steering` | rack, column, intermediate-shaft, tie-rod, ehps/eps unit, wheel | ratio, assist curve |
| `braking` | disc, drum, caliper, pad, master-cylinder, booster, abs-unit, line, hose, park-brake | friction, piston area, pedal ratio |
| `drivetrain` | ice, e-motor, gearbox (manual, auto, cvt, single-speed), clutch, torque-converter, differential, driveshaft, halfshaft, transfer-case, inverter | torque map, efficiency map, ratios |
| `energy` | fuel-tank, filler, pump, line, battery-pack, module, bms, charger, hv-cable, dc-dc | capacity, discharge curve |
| `cooling` | radiator, fan, pump, hose, intercooler, chiller | heat rejection |
| `exhaust` | manifold, catalyst, muffler, pipe | back-pressure |
| `electrical` | lv-battery, fusebox, harness-segment, connector, ecu, sensor | power budget |
| `wheels` | wheel, tyre | tyre model (Pacejka or brush) |
| `body` | panel, structural-panel, bumper-beam, bonnet, roof, floor, firewall, tray, canopy | mass, stiffness contribution |
| `closures` | door, tailgate, bonnet-assembly, hinge, latch | |
| `glazing` | windscreen, side-glass, rear-glass | |
| `lighting` | headlamp, tail-lamp, indicator, plate-lamp, reflector, drl | photometric class |
| `seating` | seat, rail, child-restraint-anchor | |
| `restraints` | belt, retractor, anchorage, airbag-module | |
| `interior` | dash-envelope, console-envelope, hvac-envelope, trim-envelope | mass, cost only |
| `fasteners` | bolt, nut, washer, rivnut, insert, adhesive-spec | grade, torque |

## 3. Primitive definition format

Written in KDL (see doc 02 section 8). A worked example: a lower control arm.

```kdl
primitive "suspension/arms/lca-wishbone-a" version="1.2.0" {
    description "A-arm lower control arm, two inboard bushes, one outboard balljoint"
    category "suspension" sub "control-arm"

    params {
        span        unit="mm" default=380 min=250 max=600 doc="inboard pivot spacing"
        reach       unit="mm" default=320 min=200 max=500 doc="pivot line to balljoint"
        tube_od     unit="mm" default=28  min=20  max=45
        tube_wall   unit="mm" default=2.5 min=1.5 max=4
        sweep_angle unit="deg" default=12 min=0 max=30
        bush_od     unit="mm" default=45
    }

    variants {
        hand "left" "right"          // mirrors geometry about the vehicle XZ plane
    }

    material "steel/e355-tube"

    geometry level="manufacture" {
        tube "front_leg" od=tube_od wall=tube_wall \
            from=(-span/2, 0, 0) to=(0, reach, 0)
        tube "rear_leg"  od=tube_od wall=tube_wall \
            from=( span/2, 0, 0) to=(0, reach, 0)
        cylinder "bj_boss" d=45 h=40 at=(0, reach, 0) axis="z"
        cylinder "bush_f"  d=bush_od h=50 at=(-span/2, 0, 0) axis="x"
        cylinder "bush_r"  d=bush_od h=50 at=( span/2, 0, 0) axis="x"
        union
    }
    geometry level="envelope" { hull }        // convex hull of manufacture level

    massprops computed=true

    ports {
        port "bush_front"  type="bush.pivot"   at=(-span/2, 0, 0) axis="x" \
            params { bush_od=bush_od bolt="M12" } load_rating="20 kN"
        port "bush_rear"   type="bush.pivot"   at=( span/2, 0, 0) axis="x" \
            params { bush_od=bush_od bolt="M12" } load_rating="20 kN"
        port "balljoint"   type="balljoint.taper" at=(0, reach, 0) axis="z" \
            params { taper="1:8" stud="M14" } load_rating="35 kN"
    }

    behaviour none

    manufacturing {
        method "tube-cut-notch-weld" scale="1..1000" {
            export "tube-list"
            export "weld-fixture-drawing"
            cost fixed="45 AUD" per_unit="0.9 AUD/mm * (span + 2*reach)"
        }
        method "cast-aluminium" scale="500..*" {
            export "step"
            cost fixed="18000 AUD" per_unit="60 AUD"
        }
    }

    compliance tags="suspension.arm" "structural"
}
```

### 3.1 Required blocks

`primitive` header, `params` (may be empty), `material` (or per-body assignments), `geometry`,
`massprops`, `ports`, `manufacturing`. A primitive that fails validation is listed in the library
browser with the error and cannot be instantiated (WMDS-02).

### 3.2 Behaviour models

Behaviour blocks are typed by the sub-category. The schema owns the type list; examples:

```kdl
behaviour "ice" {
    torque_map { rpm=(1000 2000 3000 4000 5000 6000) wot_nm=(120 180 210 215 200 170) }
    idle_rpm 800 redline_rpm 6500
    inertia "0.18 kg*m^2"
    fuel "petrol-95" bsfc_map file="bsfc.csv"
}

behaviour "damper" {
    curve compression { v_mps=(0 0.05 0.1 0.3 0.5) f_n=(0 250 400 900 1300) }
    curve rebound     { v_mps=(0 0.05 0.1 0.3 0.5) f_n=(0 400 700 1500 2100) }
    gas_force "150 N"
}

behaviour "tyre" {
    model "pacejka-mf52" file="205-55r16.tir"
    unloaded_radius "316 mm" width "205 mm" mass "9.5 kg"
}
```

### 3.3 Imported geometry

```kdl
geometry level="display" { import "engine-block.step" units="mm" }
massprops declared { mass="142 kg" cg=(0, 60, 180) inertia=(...) }
```

Imported geometry still needs ports and mass properties. Ports are placed by coordinates in the
imported file's frame.

### 3.4 Sub-assemblies as primitives

A saved `.asm.kdl` is loadable as a primitive. Its exposed ports are declared explicitly:

```kdl
assembly "corner/front-macpherson-std" version="0.3.0" {
    instances { ... }
    mates { ... }
    ports {
        export "upright.hub"        as "hub"
        export "strut.top"          as "strut_top"
        export "lca.bush_front"     as "lca_front"
        export "lca.bush_rear"      as "lca_rear"
        export "tierod.inner"       as "tierod_inner"
    }
}
```

## 4. The port system

### 4.1 Port types

Port types live in a registry (`library/ports.kdl`). A port type declares its parameter schema,
its default DOF when mated, and its compatibility rule.

```kdl
port_type "bolt.pattern" {
    params { count="int" pcd="mm" thread="string" centre_bore="mm?" }
    dof "fixed"
    compatible_with "bolt.pattern" when="a.count == b.count and |a.pcd - b.pcd| < 0.2 mm and a.thread == b.thread"
    default_fasteners "bolt" size="thread" grade="8.8" torque="from-table"
}

port_type "bush.pivot" {
    params { bush_od="mm" bolt="string" }
    dof "revolute"
    compatible_with "bush.bracket" when="a.bush_od == b.bush_od and a.bolt == b.bolt"
}

port_type "mcds.section-joint" {                  // see doc 04
    params { width_config="string" rail_section="string" }
    dof "fixed"
    compatible_with "mcds.section-joint" when="a.width_config == b.width_config and a.rail_section == b.rail_section"
    mate_stage "kit"
}

port_type "mcds.grid-mount" {
    params { grid_pitch="mm" bolt="string" }
    dof "fixed"
    compatible_with "mcds.grid-station" when="a.bolt == b.bolt"
    grid true
}

port_type "hose.push-on"   { params { id="mm" } dof "fixed" compatible_with "hose.push-on" when="a.id == b.id" }
port_type "elec.connector" { params { family="string" pins="int" } dof "fixed" compatible_with "elec.connector" when="a.family == b.family and a.pins == b.pins" }
```

The compatibility expression is the whole of the compatibility logic. Adding a new joining
standard is adding a port type, not code (WMDS-11).

### 4.2 Port frames

A port has a frame: origin, primary axis (the mating direction), secondary axis (clocking). Mating
two ports aligns their frames, with the mate DOF then freeing the appropriate motion. A port may
declare `symmetry` (e.g. 4-fold for a 4-stud pattern) so clocking is constrained only modulo that
symmetry.

### 4.3 Grid ports

Chassis sections expose `mcds.grid-station` ports at every grid pitch along each rail. A component
with an `mcds.grid-mount` port specifies its station by index, not by coordinate. When the section
length changes, station indices remain valid up to the new rail length; stations that no longer
exist raise an error listing the affected components (WMDS-12, WMDS-21).

### 4.4 Mates

```kdl
mate "lca_front" {
    a "corner_fl.lca_front"
    b "front_section.lca_bracket_fl"
    dof "revolute"                       // overrides port default only if needed
    fasteners { bolt="M12x1.75x90" grade="10.9" qty=1 nut="nyloc" torque="85 Nm" }
    stage "kit"
}
```

`stage` is `factory` or `kit`. Kit-stage mates must use joining methods on the flatpack allow-list
(threaded fasteners, rivnuts into factory-installed inserts, push-on hoses with clamps, keyed
connectors). Bonded joints, welds, press fits and rivets are factory-stage only. The schema rejects
a kit-stage mate whose fastener spec is not on the allow-list (MCDS-06).

### 4.5 Load ratings

Ports declare a load rating; mates inherit the lower of the two. Tier 0 analytics compute static
and simple dynamic loads at each mate and flag any that exceed rating. Tier 1 and 2 results refine
those loads and replace the flags.

## 5. Materials

```kdl
material "cfrp/pultruded-ud-t700-epoxy" {
    family "cfrp" form "pultrusion"
    density "1550 kg/m^3"
    orthotropic { e1="130 GPa" e2="8 GPa" g12="4.5 GPa" nu12=0.3 }
    strength { xt="2000 MPa" xc="1100 MPa" yt="50 MPa" yc="180 MPa" s="70 MPa" }
    fatigue { curve file="t700-ud-sn.csv" }
    environment { max_service_temp="120 C" uv="coat-required" galvanic="isolate-from-aluminium" }
    cost "38 AUD/kg"
    solver "openradioss" { law="LAW25" params file="t700-law25.kdl" }
    solver "calculix"    { card="*ELASTIC, TYPE=ENGINEERING CONSTANTS" }
}
```

## 6. Adding a primitive: the workflow the docs must make easy

1. Copy the nearest existing primitive file.
2. Edit params, geometry, ports, manufacturing.
3. Run `wmds lib validate path/to/file.prim.kdl`. Fix errors.
4. Run `wmds lib preview path/to/file.prim.kdl` to see it in the viewer with ports displayed.
5. Drop it into `library/<category>/`. The index regenerates on next launch.

No step involves the Rust toolchain. This workflow is the acceptance test for WMDS-03.

## 7. Library governance

* Semantic versioning per primitive. Geometry or port changes bump minor; anything that changes
  mass by more than 1 % or any port frame is a breaking change and bumps major.
* `wmds.lock` in each project pins versions. `wmds lib upgrade` shows diffs.
* Every shipped primitive has an owner and a validation status: `draft`, `reviewed`, `tested`
  (has physical test data attached).
