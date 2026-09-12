# 07 - Manufacturing and Assembly Export

## 1. Two audiences

* **Manufacturing** makes parts and factory-stage sub-assemblies. Needs per-part files in the
  format its process consumes, plus a BOM.
* **Assembly** (the kit customer or an assembly partner) puts finished parts together. Needs
  instructions that assume nothing.

Both are generated from the assembly graph. Nothing is authored by hand.

## 2. Manufacturing methods and export recipes

Each primitive declares one or more manufacturing methods with a scale range (doc 03). A method
has an export recipe. Shipped recipes:

| Method | Exports | Notes |
|--------|---------|-------|
| `flat-cut` (laser, waterjet, router) | DXF per part with bend lines and etch marks; nest sheet (optional) | For brackets, plates, floors |
| `sheet-bend` | DXF flat pattern with bend table (angle, radius, K-factor) | |
| `tube-cut-notch-weld` | Tube list (stock, length, end notch profiles as DXF unroll), weld fixture drawing | For arms, subframes |
| `machined` | STEP, plus a drawing with tolerances and datums | |
| `cast` | STEP of casting and of machined part | |
| `extrusion-cut` | Profile reference and cut list | Cross-members, crash boxes |
| `pultrusion-cut` | Profile reference, cut list on grid pitch, insert schedule (station, insert type) | MCDS rails |
| `composite-layup` | Ply book (ply order, material, orientation, flat pattern DXF per ply), mould STEP, cure schedule | Body panels, structural panels |
| `additive` | STL / 3MF with orientation and support hints | Brackets, jigs, interior |
| `purchased` | Supplier part number, quantity, spec sheet reference | Engines, tyres, glass, fasteners |

Recipes are data too: an export recipe is a list of geometry operations (section at plane,
unroll, project, offset) and a template for the accompanying text. New processes are added by
writing a recipe.

## 3. Bill of materials

Hierarchical, following the assembly tree, with factory-stage and kit-stage grouping so that a
kit's contents are a filter on the same BOM.

Columns: level, part ID, description, primitive version, variant, quantity, material, unit mass,
total mass, manufacturing method, make/buy, supplier, supplier part number, unit cost at selected
scale, total cost, stage (factory/kit), kit bag ID.

Fasteners are rolled up from mates and grouped into **kit bags** per assembly step, the way
flatpack furniture packages hardware: bag A contains the fasteners for steps 1 to 4, and the
instructions reference bag letters.

Exports: CSV, JSON, and a formatted PDF.

## 4. Cost roll-up

Each manufacturing method carries a cost model: fixed (tooling) plus per-unit (material, machine
time, labour), as expressions of the primitive's parameters. The roll-up evaluates cost at 1, 10,
100, 1 000 and 10 000 units per year and picks, per part, the cheapest method whose scale range
covers that volume. Output: cost per vehicle vs volume, cost breakdown by system, and a list of
parts whose method changes with volume (the switchover points that matter for planning).

## 5. Assembly instruction generation

### 5.1 Build sequence derivation

Inputs: the mate graph with stages, port frames, and part envelopes.

1. Take the kit-stage mate subgraph (factory-stage sub-assemblies arrive as single parts).
2. Choose the base: the chassis central section (or the section-joint operation for a
   full-length vehicle).
3. Order mates by: structural dependency (a part must be mounted before things mount to it),
   then accessibility (a mate whose fasteners would be occluded by an already-placed part must
   come first; computed from port frame direction and envelope occlusion), then grouping (mates
   of the same sub-system and same fastener bag stay together), then a designer-editable priority.
4. Group ordered mates into steps of 1 to 6 fasteners of the same type, one sub-assembly per step
   where possible.
5. Insert check steps ("confirm the section joint bolts are torqued before proceeding") after
   structurally critical mates, and lifting or support steps where a part exceeds a mass threshold
   (default 20 kg for one person, 40 kg for two).

The sequence is stored with the project and can be edited; edits are preserved across
regenerations as long as the referenced mates still exist.

### 5.2 Per-step content

* Exploded 3D view: parts already assembled shown grey, parts being added shown in colour,
  fasteners drawn with insertion arrows from the port frame axis.
* Fastener callouts: bag letter, item icon, size, quantity, and torque value in Nm with a plain
  language equivalent ("tight, then a quarter turn" is never used; torque wrench use is a stated
  prerequisite for structural steps).
* Tools required for the step.
* Warnings drawn from the mate and primitive metadata: heavy, sharp, orientation critical,
  left/right hand.
* Verification: what the assembler should see when the step is correct.

### 5.3 Document structure

1. Contents of the kit with a checklist and bag inventory.
2. Tools required for the whole build.
3. Safety page.
4. Steps, numbered, one per page or spread.
5. Torque table and fastener glossary.
6. Final checks before first drive, tied to compliance items that require assembler declaration
   (e.g. wheel nut torque, brake bleed).

### 5.4 Outputs

* PDF, with page size and language selectable (English first; strings are externalised).
* Interactive viewer: the assembly rendered in 3D with a step slider, the same content. Delivered
  as a standalone HTML/WebGL bundle with the geometry embedded, so it opens in any browser without
  installing WMDS (this is the one place where a browser build of the viewer is required; see
  WMDS-65).

### 5.5 Usability testing

Generated instructions are tested with people who have never assembled a vehicle. Metrics: steps
completed without asking, errors, time. This is a product requirement (WMDS-52, MCDS-06), and the
generator is not done until a test passes.

## 6. Jigs and fixtures

For factory-stage sub-assemblies (bonded rail end fittings, cross-member bonding, body panel
layup) the software generates fixture geometry from the part geometry: location surfaces offset by
clearance, pin locations at port frames, and a fixture BOM. Jigs are themselves primitives with the
`additive` or `flat-cut` method so they can be made by a small workshop.

For kit-stage section joining, the target is **no jig**: self-locating joint fittings. If that
cannot be achieved, a simple alignment fixture is included in the kit and appears in the
instructions like any other part.

## 7. Traceability

Every export carries the project name, vehicle configuration hash, primitive versions and the WMDS
version. A part file can always be traced back to the exact design state that produced it.
