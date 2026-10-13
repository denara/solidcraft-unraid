# Sheet metal

The SHEET METAL tab builds parts from a sheet of constant thickness under a **sheet metal rule**
(thickness, bend radius, K-factor), set in **Sheet Metal Rules** (`sheet.manage_rules`).

- **Flange** (`sheet.flange`): a base flange from a sketch profile, a contour flange from an open
  sketch, or an edge flange from a part edge.
- **Hem** (`sheet.hem`) and **Fold** (`sheet.fold`) along a sketch line.
- **Unfold / Refold** (`sheet.unfold`, `sheet.refold`) for working on the flat part.
- **Convert** (`sheet.convert`) turns a solid of constant thickness into sheet metal.
- **Create Flat Pattern** (`sheet.flat_pattern`) shows the flat part with its bend lines and a
  bend table, and exports it as DXF or a bend-table CSV through the save picker.

Bends follow the rule's K-factor, so flat lengths match what a press brake will make. Tests
check flat lengths against hand-computed bend allowances.

Where two bends of one edge flange feature meet at a corner of the base, the rule's **corner
relief shape** (`corner_relief_shape`; Corner Relief Shape in Sheet Metal Rules) says what is
cut there. `notch`, the default, cuts nothing more: the bends end square at the corner. `round`
and `rectangular` cut a relief the rule's **corner relief** across, centred on the corner where
the two bend lines meet: out of the base a round with half that size as its radius, or a square
with half that size as its side, and both bends stop where it begins, cut straight across. The
flange panels keep their ends. The relief is in the folded part, the flat pattern, the bend
table (the bends are shorter) and the DXF (a round relief is an arc). At a corner that isn't
square the round follows the angle of the base and the square becomes a kite, its two cuts
square to the edges. Changing the shape in a rule changes the parts made with that rule. A
relief that doesn't fit (under 0.01 mm across, leaving nothing of a bend, or reaching as far
from its corner as another edge of the base is) is left out, and the flange feature reports a
warning saying where.

```json
{"command":"sheet.manage_rules","params":{"name":"Sheet 2 mm","thickness":"2 mm","k_factor":0.5,
 "bend_radius":"1 mm","corner_relief_shape":"round","corner_relief":"8 mm","active":true}}
```

Where the edge flanges of one feature meet at a corner of the base, their panels are closed
toward each other at any bend angle: each end is cut square to the sheet, on a line in the flat
chosen so that the edges nearest each other (on the inner faces) run parallel, the rule's
**gap** apart, either side of the plane halfway between the flanges. Flanges at 72° get slanted
ends in the flat; corners that aren't square close the same way, and at 90° it is the corner
the rule always gave. A corner that can't be closed (a gap under 0.01 mm, flanges bending
opposite ways, ends that would overlap in the flat pattern) stays open, and the flange feature
reports a warning saying where. A flange added by a later feature leaves the corner as it was:
the earlier flange keeps its length and the new one ends at its bend.

The flanges of one edge flange feature can each have an angle and a height of their own. An
entry of `edges` is a point `[x, y, z]`, which takes the feature's `height` and `angle`, or an
object `{"point": [x, y, z], "angle": "105 deg", "height": "30 mm"}` whose `angle` and `height`
(either one, or both) replace them for that edge. They are expressions like the feature's own,
so parameters work, and each is a feature input of its own (`Edge angle`, `Edge height`) after
`Height` and `Angle`. The corners between such flanges close like those between equal ones:
the ends run toward each other the rule's gap apart, either side of the plane halfway between
the two flanges, and a taller flange's end keeps that line up to its own tip. This is how to
close a corner between flanges at different angles, because flanges of separate features
don't close toward each other. In the Flange dialog, picking more than one edge shows a row
per edge with an angle and a height box; an empty box takes the value above.

```json
{"command":"sheet.flange","params":{"edges":[[60,0,2],{"point":[120,45,2],"angle":"90 deg"},
 {"point":[60,90,2],"angle":"105 deg","height":"30 mm"},{"point":[0,45,2],"angle":"120 deg","height":"25 mm"}],
 "height":38.6,"angle":"75 deg"}}
```

For a shop drawing or spreadsheet, `sheet.export_bend_table` returns CSV by default; choose
`format: "json"` for structured data. It includes the sheet rule, thickness, bend angles,
radii, allowances and flat centre-line coordinates. Lengths use millimetres and angles use
degrees. Supply `path` to save it, or omit `path` to receive the content through the command
response. `body` defaults to the last sheet body. Exporting does not add a timeline entry or
consume an undo step.

```json
{"command":"sheet.export_bend_table","params":{"format":"csv","path":"bracket-bends.csv"}}
{"command":"sheet.export_dxf","params":{"path":"bracket-flat.dxf"}}
```

DXF cutout geometry preserves lines, circles and circular arcs; other curves are sampled
adaptively. The `OUTLINE`, `BEND_UP` and `BEND_DOWN` layers separate cuts from bend centres.

Base-flange profiles keep their interior holes. Through cuts that cross a sheet edge become
notches in the flat outline; overlapping and repeated cuts remove their combined area once.
The flat preview, DXF and unfolded solid use the same material boundaries. Lines and circular
arcs remain exact; other profile curves use a 0.001 mm chord tolerance.

Cuts can leave separate material pieces, including the centre of a ring cut. These pieces
remain one logical sheet group through Unfold / Refold and export as separate flat outlines.
Large profiles that exceed the geometry processing budget report an error instead of
exporting a partial outline.

Parallel, untapered extrude cuts also preserve shallow pockets through Unfold / Refold.
The pocket floor and side walls remain solid geometry; overlapping cuts from opposite sides
become a through hole wherever they remove the whole thickness. Flat-pattern JSON lists
`pockets` with their side, depth and bottom/top thickness coordinates in millimetres.
DXF pocket boundaries use `POCKET_BOTTOM_Z…_TO_…_MM`, `POCKET_TOP_Z…_TO_…_MM`, or
`POCKET_INTERNAL_Z…_TO_…_MM` layers. These are machining operations, so do not treat every
pocket layer as a through-cut contour when importing the drawing into a CAM tool.

Extrude Intersect on a straight base or an unfolded sheet retains the material inside both
the current tool footprint and its depth interval, including interior holes, detached islands
and earlier pockets. A shallow intersection records the removed layers above and below the
retained material as top and bottom pockets. The sheet rule keeps the original stock gauge;
the remaining solid can be thinner. A completely empty intersection removes the sheet and
its metadata together. DXF records pocket depth in its layer names; importing that DXF as
ordinary sketch geometry does not reconstruct the machining depths automatically. Each
positive retained or removed depth layer must meet the kernel's 0.000001 mm extrusion
resolution; smaller layers report an error rather than being rounded away. To Object can
retain a tool that trims to a straight prism, using its actual trimmed footprint. A curved
tool with varying depth is refused before changing sheet geometry. Pocket profiles can move with planar panels when forming,
but a pocket crossing a bend zone cannot yet be refolded. Machining folded bend material,
tapered sheet cuts and other unsupported 3D sheet Cut/Intersect operations report an error before changing
the design; they cannot silently restore removed material during a later sheet rebuild.

A sketch can be drawn on a flat face of the sheet: the base or a flange, on either side. Pick
the face (or give `sketch.create` the point `plane: {face: [x, y, z]}`) and the sketch attaches
to that panel instead of to a certified source face. It keeps the face's name,
`sheet:<body>:<panel>:<top|bottom>` (the panel is `base` or the flange's feature tag), and finds
the face again whenever the sheet is rebuilt: after a parameter change such as the thickness or
a flange angle, through Unfold / Refold, after the file is reopened and when the body is
renamed. Sketch coordinates are the flat pattern's, so a point drawn on a flange lies where the
flat pattern and its DXF have it, and on the bottom face y runs the other way, as seen from
below. When the face is gone (its flange is suppressed or deleted) the sketch reports a lost
attachment instead of moving to another face; **Redefine Sketch Plane** attaches it to another
face. `fixed: true` still freezes the picked plane in space.
