# Sheet metal

The SHEET METAL tab builds parts from a sheet of constant thickness under a **sheet metal rule**
(thickness, bend radius, K-factor), set in **Sheet Metal Rules** (`sheet.manage_rules`).

- **Flange** (`sheet.flange`): a base flange from a sketch profile, a contour flange from an open
  sketch, or an edge flange from a part edge.
- **Hem** (`sheet.hem`) and **Fold** (`sheet.fold`) along a sketch line.
- **Unfold / Refold** (`sheet.unfold`, `sheet.refold`) for working on the flat part.
- **Convert** (`sheet.convert`) turns a plate of constant thickness into sheet metal, or, with
  bend edges, a solid or a shelled body.
- **Create Flat Pattern** (`sheet.flat_pattern`) shows the flat part with its bend lines and a
  bend table, and exports it as DXF or a bend-table CSV through the save picker.

Bends follow the rule's K-factor, so flat lengths match what a press brake will make. Tests
check flat lengths against hand-computed bend allowances.

An edge flange takes its setback off its own edge only. Where that edge ends at an inside
corner of the base (the base runs on past it, like the leg of an L), the rest of the base
stays, and the rule's **bend relief** (`relief_width` and `relief_depth`; Relief Width and
Relief Depth in Sheet Metal Rules) is cut into it beside the bend's end: a slot the relief
width wide, from the relief depth before the bend line to the relief width past the flange's
far edge in the flat pattern. The bend and the flange are then free of the base, folded and
flat. A relief that can't be cut (a width under 0.01 mm, one as wide as the base beside the
bend, or a slot that would cut the base apart) is left out, and the flange feature reports a
warning saying where.

Where a cut-out has taken part of an edge away (a corner cut off the plate, a notch in the
middle of an edge), an edge flange picked on that edge runs along the stretch the cut-outs
leave nearest the pick, and takes its setback off that stretch only. The base on the other
side of a notch keeps its edge, and a pick there, in the same feature or a later one, makes
a flange of its own. Where base is still left beside the bend's end at such a cut-out, it
gets the bend relief as above. An edge that cut-outs have taken whole is refused.

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

A flange can also stand on the far edge of another flange: a return, such as a lip turned in
along the top of a tray's walls. It runs along that edge from end to end, so it follows the
closed end of the flange it stands on, slanted or not. Two returns of one feature whose flanges
meet at a corner of the base are closed toward each other as well, with the rule's gap. They
are mitred either side of a plane that runs along the mean of the two panels' normals, bisects
their bend lines and passes where the lines their panels start on cross. Returns that lie in
one plane (level returns on a tray's walls, whatever the walls' angle) then meet on the corner's
bisector, like the sides of a picture frame. Each panel ends on a slanted line in the flat.
Each bend stops square where all of it keeps half the gap from that plane, at the latest at
the end of the flange it stands on, which can leave a short stretch of that flange's edge bare.
Returns turned opposite ways pass each other. A return that meets no other at a corner ends
with the flange it stands on; where the flange beside that one leans in over it, the return is
cut back until it keeps as clear of that flange as the end of the flange it stands on does.
Returns that can't be closed stay open with a warning, as on the base. Returns of separate
features don't close toward each other either: make the returns round a corner in one feature.
A corner relief changes none of this: it leaves the flanges' closed ends as they are, so the
returns on them close over a relieved corner as over a whole one.

```json
{"command":"sheet.flange","params":{"edges":[[60,2,20],[118,45,20],[60,88,20],[2,45,20]],
 "height":12,"angle":"90 deg"}}
```

(The four returns of a 120 x 90 tray in 2 mm sheet with 90° walls 20 high: each point is the
middle of a wall's far edge on its inner face, so the returns turn in over the base.)

**Convert** also takes a solid or a shelled body: pick the face that stays fixed and the sharp
edges that become bends (`bends`; Bend Edges in the dialog). The flat faces reached from the
fixed face across those edges become the sheet. A shelled body keeps its walls and their
thickness; any other solid's faces become the outside of the sheet, with the rule's thickness
taken into the body. Each sharp edge stays where the faces of its bend would meet, so the part
keeps its envelope and the flat pattern is the outside lengths less a bend deduction per bend,
2·(R + T)·tan(θ/2) − θ·(R + K·T). A bend can be picked on either skin of a shell; bends turn
either way, follow one another and take any angle (the walls of a tapered tray at 72°). An
edge between two faces of the sheet that is no bend is cut open, each side half the rule's
**gap** from the plane halfway between the two, like the corners of a tray. The result is an
ordinary sheet body: edge flanges, cuts, Unfold, the flat pattern and its exports work on it,
and it follows the solid when a parameter changes (the edges and the fixed face are found by
their names). Without bend edges Convert expects a plate, as before. Not converted yet: curved
faces (rolled sheet, a filleted edge as a bend), faces with holes or curved edges (cut them
after converting) and bends that close a ring.

```json
{"command":"solid.box","params":{"length":100,"width":60,"height":30,"body_name":"Tray"}}
{"command":"solid.shell","params":{"faces":[[50,30,30]],"thickness":2}}
{"command":"sheet.convert","params":{"body":"Tray","face":[50,30,0],
 "bends":[[50,0,0],[100,30,0],[50,60,0],[0,30,0]]}}
```

For a shop drawing or spreadsheet, `sheet.export_bend_table` returns CSV by default; choose
`format: "json"` for structured data. It includes the sheet rule, thickness, bend angles,
radii, allowances and flat centre-line coordinates. Lengths use millimetres and angles use
degrees. A bend is listed where its centre line has material: one that a cut-out has shortened
gives the length and ends it has left, and one that cut-outs split gives a row for every piece,
all with the bend's number (the flat pattern and the DXF draw the same pieces as bend lines).
Supply `path` to save it, or omit `path` to receive the content through the command
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
arcs remain exact. An ellipse, a spline or text stays the curve it was drawn as in the flat
pattern and the unfolded solid; the folded solid follows it with tangent arcs within 0.001 mm,
and the DXF and the point lists write it as chords within 0.001 mm.
Flat-pattern JSON lists each cut-out twice, in the same order: as points (`cutouts`), and
exactly (`cutout_shapes`): a round hole as `{"circle": {"center", "radius"}}`, any other as its
`line`, `arc` (centre, radius, start and sweep in degrees) and `polyline` segments (one
`polyline` for each ellipse quarter, spline or glyph curve).

Cuts can leave separate material pieces, including the centre of a ring cut. These pieces
remain one logical sheet group through Unfold / Refold and export as separate flat outlines.
Large profiles that exceed the geometry processing budget report an error instead of
exporting a partial outline.

A Box cut, or a Cylinder cut whose axis is the sheet's normal, machines the sheet like the
extrude of its footprint: through the sheet it is a cut-out (a cylinder a round one), and
part way it is a pocket. A primitive cut at any other angle, or across a folded bend, is refused.

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
