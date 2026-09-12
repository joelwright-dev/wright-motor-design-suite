# 12 - Progress notes, 13 September 2026

The session that turned the application from a viewer into an editor, and filled in the steering
and braking systems.

## What to run first

Double-click `view.cmd`. That builds the editor and opens it on the reference vehicle. Nothing
else needs setting up.

## The correction

You said the tool was a model loader, not something you could build a vehicle with, and that
authoring vehicles by editing text files made no sense. Both were right.

The architecture called for definition files as the storage format, which is correct: text files
diff, review and version, and a vehicle programme needs that. The error was treating the storage
format as the authoring interface. Files are now an output of the editor. They remain readable
and hand-editable, and nothing requires anyone to type one.

[Document 11](11-editor.md) describes the editor in full. The short version: choose a chassis,
add parts from a searchable catalogue, and join two parts by picking two ports that fit. The
editor only ever offers ports that exist, and once you have picked the first it only offers ones
the mate checker will accept, sorted nearest first. You can pick them in the 3D view or from a
list. Save writes the file.

## The rebuild had to get fast

The first working version took 20.8 seconds to rebuild after every change, which is not an
editor, it is a batch job with a window. Two things fixed it.

Parts are now tessellated once and cached, keyed by everything the part resolved to. Placement is
a rigid transform of the cached mesh, which costs nothing. A test asserts the thing that matters:
changing the battery length rebuilds exactly one part, not forty-nine.

The same cache also removes duplicates within a single build. A vehicle with four identical
corners builds one of each part rather than four, which is why a cold open of the reference
vehicle now reports "35 of 63 parts reused" and takes about 10 seconds instead of 21. An edit
after that is effectively instant.

## What the library gained

**Braking.** Disc, caliper and tandem master cylinder, plus the caliper mounting lugs on the
upright. The disc carries both halves of the wheel interface, a `wheel.mount` on the hub side and
a `hub.face` on the wheel side, because that is physically what a hat disc is: it sits between the
hub and the wheel. One consequence showed up immediately in a test, which is the point of having
the test: the front track grew by 20 mm, because the disc's mounting face pushes each wheel that
far further out. It does that on a real car too.

**Steering.** Rack, tie rod, intermediate shaft, column and steering wheel. The rack bolts to the
chassis grid like everything else, so where the rack goes is a choice rather than a weld.

The steering geometry is where the model earned its keep twice.

First, the tie rods did not close: the checker reported the rack end and the tie rod joint 595 mm
apart. The cause was a wrong local frame, a tie rod drawn running fore and aft instead of across
the car. The rod now has a lateral span and a longitudinal sweep, which is what a real tie rod
has, because the rack never sits directly inboard of the steering arm.

Second, with the column mounted straight onto the pinion, the steering wheel finished directly
above the front wheels. That is not a drawing error, it is what the design said, and it was
wrong. Real cars solve it with a raked intermediate shaft, so the library now has one. The
steering wheel sits at x = 404 mm and the driver's mass is at x = 600 mm, which is a driving
position rather than a bonnet ornament.

**Five new compliance rules**, on dual circuits, a brake at every wheel, a caliper for every
disc, a collapsible steering column, and one tie rod per steered wheel. All five pass. They read
the compliance tags each primitive declares rather than part names, so a bought-in caliper
satisfies them as long as it says what it is.

## Two bugs the work uncovered

**Every geometry feature in the library had lost its name.** When `Expr::TextOr` was added so
that `"amphenol-surlok"` would stop parsing as a subtraction, it also wrapped feature names like
`hat_face`, and the code reading them only understood the two older shapes. Nothing noticed,
because nothing had yet used `subtract a=X b=Y` or `mirror of=X`. The first part that did could
not find the body it was naming. Fixed, with the reason written next to the fix.

**Compliance tags were declared and thrown away.** Every primitive in the library carries tags
like `braking.disc`, and the rules engine was building its facts with an empty tag list, so no
rule could ever ask what a part claims to be. That is why the new rules could be written the way
they are.

## Things added to the expression language

`pi`, `tau` and `e` as constants, because a piston area written with 3.14159 in it is how a
rounding error gets into a brake calculation. Litres and millilitres as units, because brake
reservoirs and fuel tanks are quoted in litres and nobody quotes them in cubic metres.

## Where the reference vehicle stands

| | |
|---|---|
| Modelled mass | 526 kg |
| Declared point masses | 469 kg |
| Wheelbase | 2400 mm |
| Track | 1280 mm |
| Weight distribution | 46 / 54 front to rear |
| Centre of gravity height | 574 mm |
| Static stability factor | 1.11 |

Thirteen instances, sixty-three parts once sub-assemblies are expanded, thirty-five joints.

## What still needs your decision

The four questions from [document 10](10-overnight-progress.md) are all still open. The static
stability factor is the one worth looking at: 1.11 is SUV territory, and the two things driving
it are the battery sitting on top of the rails rather than in the floor, and the provisional
point masses for the body and interior being placed high. Both are decisions, not bugs.

Two new ones:

1. **The caliper clock position is not modelled.** Calipers are drawn at top dead centre. Where a
   caliper actually sits around the disc decides whether it clears the wheel and how it bleeds.
   Worth modelling before any wheel and tyre package is locked.
2. **The column bracket has nothing to bolt to.** The steering column is positioned correctly by
   its shaft, but the bracket that should hold it up is an unconnected port, because there is no
   cabin structure in the model yet. It is the first thing that will need one.

## What the editor still cannot do

Listed in [document 11](11-editor.md) so it stays in one place. The short list: no undo, no
dragging in 3D, no editing an existing joint, no compliance panel in the window, and point masses
are file level only.
