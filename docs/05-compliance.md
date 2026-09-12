# 05 - Compliance

## 1. What "comply with regulations" means for software

A vehicle is certified by people, on the basis of evidence. Software cannot certify anything. What
it can do, and what WMDS does:

1. Know which rules apply to this vehicle (category, mass, market, date).
2. For each rule, either check it directly against the model (a headlamp height is a number), run
   or request a simulation that produces the evidence (a brake stopping distance), or state
   plainly that a physical test or a declaration is required and what that test is.
3. Produce a traceable report a signatory can work from.
4. Do all of this continuously, so the designer finds out about a failure while the design is
   still cheap to change (WMDS-33).

## 2. Regulatory context

### 2.1 Australia (primary market)

* **Australian Design Rules (ADRs)** are the national standards, administered under the Road
  Vehicle Standards Act 2018 (RVSA). Vehicle categories relevant to MCDS: MA (passenger car), MB
  (forward-control passenger), MC (off-road passenger), NA (light goods vehicle, up to 3.5 t GVM).
* **Individually Constructed Vehicles (ICVs)** are one-off and kit-built vehicles. They are
  registered by state authorities under **VSB 14 / NCOP** (National Code of Practice for Light
  Vehicle Construction and Modification), Section LO (Vehicle Standards Compliance), which maps
  the applicable ADRs to construction requirements and alternative compliance methods that suit
  low-volume builders. Certification is by an approved signatory or engineer.
* **Low-volume type approval** under the RVSA is the route for producing more than a handful of
  units per year. It is a different evidence burden from ICV.

A design house that ships kits sits between these two regimes, and the assembler of a kit may be
the legal "manufacturer" for registration purposes in some states. **OPEN, needs regulatory
advice:** which regime applies to a customer-assembled MCDS vehicle, per state. The software
handles this by making the regime a rule-pack choice, not by deciding it.

Rule packs to build, in order:

| Pack | Content | Priority |
|------|---------|----------|
| `adr/icv-vsb14-lo` | ADR requirements as applied to ICVs under VSB 14 Section LO | first |
| `adr/ma`, `adr/mc`, `adr/na` | full ADR sets per category for low-volume approval | second |
| `wright-internal` | company design rules (mass targets, durability, kit-stage joining) | with first |
| `unece/…` | UN Regulations for export markets | later |
| `fmvss/…` | US Federal Motor Vehicle Safety Standards | later |

### 2.2 Verification note

The regulatory descriptions above are working understanding, not legal advice. Every rule in a
shipped pack must cite its source document and clause and carry a `verified_by` field with a
person and date. Unverified rules are shown in reports with a warning.

## 3. Rule pack format

```kdl
rulepack "adr/icv-vsb14-lo" version="2026.1" {
    source "VSB 14 Section LO, rev ..." url="..."
    jurisdiction "AU"

    rule "ADR13.headlamp.height" {
        title "Headlamp centre height"
        source "ADR 13/00 clause ..." verified_by="J. Wright" date="2026-09-01"
        applies_when "vehicle.category in [MA, MB, MC, NA]"
        evidence "calculation"
        query {
            lamps = select(instances, tag="lighting.headlamp")
        }
        check "all(lamps, l -> l.port('lens').centre.z >= 500 mm and l.port('lens').centre.z <= 1200 mm)"
        // z here is measured in laden vehicle attitude; the query API resolves ride height from Tier 0
        on_fail "Headlamp centre must be between 500 and 1200 mm above ground at laden ride height."
        severity "fail"
    }

    rule "ADR31.brakes.service-performance" {
        title "Service brake stopping performance"
        source "ADR 31/... "
        applies_when "vehicle.category in [MA, MB, MC]"
        evidence "simulation" sim="braking.straight-line" tier=1 {
            initial_speed "100 km/h" surface_mu 0.8 load "laden"
        }
        check "result.stopping_distance <= 70 m and result.mfdd >= 6.43 m/s^2"
        physical_test_required true   // simulation is screening; the ADR requires a track test
        on_fail "..."
    }

    rule "ADR69.frontal.occupant-protection" {
        title "Full frontal impact occupant protection"
        applies_when "vehicle.category == MA"
        evidence "simulation" sim="crash.frontal-full-width" tier=2 { speed "48 km/h" }
        check "result.compartment_intrusion.max <= 150 mm and result.pulse.peak_g <= 60"
        physical_test_required true
        note "Simulation thresholds are internal screening values, not the ADR criteria, which are dummy-based."
    }

    rule "ADR34.child-restraint-anchorages" {
        title "Child restraint anchorage points"
        applies_when "vehicle.category in [MA, MB, MC] and vehicle.seating.rear_positions >= 1"
        evidence "declaration"
        check "count(select(instances, tag='restraint.child-anchor')) >= min(3, vehicle.seating.rear_positions)"
        physical_test_required true   // anchorage strength test
    }
}
```

### 3.1 Evidence classes

| Class | Meaning | Report status when check passes |
|-------|---------|---------------------------------|
| `calculation` | Checked directly from the model | Pass |
| `simulation` | Requires a named simulation at a named tier | Pass (simulated) - plus Needs Physical Test if flagged |
| `physical-test` | Cannot be shown by software | Needs Physical Test (with test spec attached) |
| `declaration` | Designer or supplier declares (e.g. glazing certified by supplier) | Needs Input until a declaration document is attached |

### 3.2 Query API

Rule checks read the model through a query API rather than raw file structure, so that primitives
can be reorganised without breaking rules. The API exposes: instances by tag/category, ports and
their world frames, mass properties, vehicle metadata, Tier 0 analytics, dimensional measures
(ground clearance, overhangs, approach and departure angles, track, wheelbase, overall dimensions),
field-of-view helpers (for mirror and lighting rules), and simulation results by name.

## 4. Engine behaviour

* On load and on every change, applicability is evaluated for every rule in the selected packs.
* Applicable `calculation` rules re-evaluate incrementally: each rule records the query paths it
  read; only rules whose paths changed re-run.
* `simulation` rules are marked Stale when their inputs change. The designer runs simulations
  explicitly (Tier 1 may auto-run if enabled; Tier 2 never auto-runs).
* Failures are attached to the instances the rule queried, so the viewport can highlight them.

## 5. Report

Sections: vehicle identity and configuration hash; rule packs and versions; summary counts; rule
table (ID, title, status, evidence, source clause, verified-by); per-failure detail with the
measured value and the limit; list of physical tests required with the test specification; list of
declarations required; unverified-rule warnings; and the mandatory statement that the report is
design evidence for use by an approved signatory and is not a certification (WMDS-35).

Exports: PDF (human), JSON (machine, for CI and for the signatory's own tooling).

## 6. Internal rule pack examples

The same engine enforces company rules:

* Chassis mass fraction ≤ target (doc 04, section 6).
* Every kit-stage mate uses an allow-listed joining method.
* Every CFRP part in a crash load path has a progressive-damage material card.
* Every metallic insert in CFRP has an isolation spec.
* Every port load exceeding rating at Tier 0 is a failure until a Tier 2 result clears it.
