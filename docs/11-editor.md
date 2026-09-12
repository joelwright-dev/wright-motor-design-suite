# 11. The editor

This document describes the design surface: the part of WMDS a person actually uses to build a
vehicle. Everything else in the suite exists to serve it.

## The mistake this corrects

The architecture document specifies definition files as the storage format, and that is right.
Text files diff, review, merge and version in a way that a binary CAD document never will, and a
vehicle programme lives or dies on being able to see what changed between two revisions.

It does not follow that a person should author those files by typing them. That was an
unexamined leap from "the model is data" to "the user edits data", and it made the application a
viewer with a file path box. Nobody designs a car by writing KDL.

The rule now: **definition files are an output of the editor, not an input to the user.** They
remain fully hand-editable, and the reference vehicle is still readable as text, but no workflow
requires it.

## What the editor is

A single window. A tool panel on the left, a 3D view filling the rest.

The left panel has three tabs.

**Parts.** Four sections, in the order a vehicle gets built.

1. *Chassis.* Choose the system, the length configuration, the width and the rail section, then
   set each section's length with a slider bounded by what the section kind allows. Changing any
   of these regenerates the mounting grid immediately.
2. *Add a part.* The whole library, searchable, grouped by category, one button per entry.
   Primitives and sub-assemblies both appear; a sub-assembly is labelled as one, because adding
   a front corner brings nine parts with it.
3. *In this vehicle.* Every instance, with the generated chassis listed first. A part that no
   joint reaches is coloured red and says so, because a part sitting at the origin because
   nothing holds it is the single most common mistake and the easiest one to miss.
4. *The selected part.* Its adjustable dimensions as sliders in the declared unit, its variants
   as dropdowns, and Delete, Make root and Join. A dimension the vehicle has not overridden is
   marked `library`; once changed it gains a Reset button that puts the library value back.
   Derived parameters are not shown, because they are computed and setting them would be a lie.

**Joints.** Two ports, chosen in either order, and a button. See below.

**Check.** Mass and centre of gravity, anything that failed to resolve, every part that is not
placed, the warnings from the mate solver, and the full placed-parts table with coordinates and
masses.

## Making a joint

A joint is a mate between two ports. The editor never lets a person name a port that does not
exist, and never offers a second port that cannot legally take the first.

The flow:

1. Pick the first port, from the dropdown or by clicking its marker in the 3D view.
2. The editor asks the mate checker about every free port in the vehicle, keeps the ones that
   pass, and sorts them by distance from the first. On a chassis this matters: the grid offers
   dozens of identical stations, and the one wanted is nearly always the nearest that fits.
3. Pick the second, again from the list or from the view. Only candidates are drawn while a
   joint is being made, so the clutter of a full vehicle disappears at the moment of choosing.
4. Bolt them together. A fastener is filled in from the port's own bolt size, so the joint
   arrives with what the bill of materials and the assembly instructions both need rather than
   as a bare geometric constraint.

The compatibility test is the same `ports_compatible` the resolver uses, not a copy of it. An
M14 stud will not appear in the list for an M12 socket, because the port registry says so, and
the editor and the checker cannot drift apart.

Colours in the view: blue is the first pick, green the second, yellow a candidate, grey a port
already used, dull gold a free port. Escape cancels.

## Why the model rebuilds after every change

Every edit modifies an `AssemblyDef` and then re-resolves it from scratch: generate the chassis,
place everything through the mate graph, build the geometry, roll up the mass. There is one path
from a definition to a placed vehicle and the editor takes it like everything else.

This is slower than patching the scene in place, and it is worth it. What is on screen is what
the file says. A saved file cannot open differently from how it looked when it was saved, and a
bug in placement shows up while designing rather than at export.

The rebuild runs on a worker thread, so the window stays responsive; a build that is superseded
by a newer edit is discarded by version number rather than being waited for.

## Saving

Save writes the definition with `wmds_schema::write_assembly`. Three properties are tested:

- Reading a file, writing it and reading it again gives the same model.
- Writing the same model twice gives byte-identical text, so saving does not churn a diff.
- Expressions keep the text they were written with. `36 kWh` stays `36 kWh` rather than becoming
  `1.296e8 J`, which would be correct and useless.

An unsaved change is marked in the toolbar. Nothing is written until Save is pressed.

## What the editor still does not do

Stated plainly so it is not mistaken for finished.

- **No undo.** The next thing to build.
- **No dragging parts in 3D.** Position comes from joints, which is the right default for a
  bolted modular vehicle, but free placement exists in the schema and has no user interface.
- **No joint editing.** A joint can be made and deleted, not adjusted; offset and clock are file
  level only.
- **No body or interior surfaces.** The library has no panels to add yet.
- **No compliance panel.** The rules engine runs from the command line; its results are not in
  the window.
- **Point masses are file level.** The provisional masses standing in for unmodelled systems
  cannot be edited or moved in the application.

## Related

- [02. Architecture](02-architecture.md) for where the editor sits.
- [03. Primitive library](03-primitive-library.md) for what can be added.
- [04. MCDSv1](04-mcds-v1-spec.md) for the chassis the editor generates.
