# Sketching and constraints

A sketch lies on a plane: an origin plane, a construction plane or a planar face. Start one with
**Create Sketch** (`sketch.create`) and click a plane, or pick a face first. **Finish Sketch**
returns to the model.

In scripts, `XY`, `XZ`, `YZ` and their `Top`, `Front`, `Right` aliases name origin planes.
These names are reserved: a construction plane cannot be created with or renamed to one
(case-insensitive). If an older design already has a construction plane with such a name, a
string argument naming it is ambiguous and is rejected; use the other alias (`XY` for `Top`)
for the origin plane, the construction feature's numeric ID to select it, or an explicit
`{origin: [x,y,z], normal: [x,y,z]}` plane. Existing saved references retain their meaning;
opening a design does not reinterpret old sketches.

Curves adapt to the current zoom and display scale. Picking and highlighting use the same
visible boundary, including small circles at Fit and sketches in rotated components.
Profile shading follows those curves while preserving holes. Zooming changes the display
without changing sketch geometry, dimensions, or undo history.
On short windows or enlarged displays, the Sketch Palette's options scroll while its
constraint status and **Finish Sketch** button remain visible.

## Geometry

**Point** is available directly on the default Sketch **CREATE** toolbar. Click it, then
click in the sketch to place a point. Existing toolbar customizations are kept; use
**Reset Toolbar** from the toolbar's context menu to restore the default buttons.

Lines and polylines (L), rectangles (two-point R, three-point, centre), circles (centre C,
two-point, three-point, tangent), arcs (three-point, centre, tangent), polygons, slots, ellipses,
fit-point and control-point splines, conics, points and text. **Construction** (X) turns curves
into reference geometry that doesn't form profiles. Modify tools include trim (T), extend, break,
fillet, chamfer, offset (O), mirror, patterns, move/copy and scale.

Polygon tools show their edge outline while positioning the second point, so the orientation
can be checked before placement. Inscribed and circumscribed polygons retain their support
circle and update the outline when the inline **Sides** value changes.

**Text** is set in the built-in font (Liberation Sans) unless it is given another: `sketch.text` and
`sketch.edit_text` take a `font`, the family or full name of an installed font ("DejaVu Sans") or the path
of a `.ttf` / `.otf` file, and the **Edit Text** dialog has a **Font** field for it. An unknown font is an
error, and the font is kept with the text in the design, so the text stays editable in it. `SOLIDCRAFT_FONT_DIRS`
adds folders to the ones searched for installed fonts.

Right-click a letter's filled interior or outline and choose **Edit Text**. You can also
start **Edit Text** first and click the letter. All outlines belonging to that text are
highlighted while the dialog is open. A profile bounded by multiple text sources or other
geometry cannot identify one text: use **Select Other** to choose its outline instead.
Change the text, font, baseline X/Y, height or angle, then confirm with **OK**; **Cancel**
keeps the original text. Position and height use the document's units.

Drag a text outline to move all its letters together. Let go to finish in one undo step,
or press **Esc** during the drag to restore the original text. The text stays editable.
For an exact displacement, select an outline and choose **Move/Copy** from its context menu.
Enter **Distance X**, **Distance Y** and an optional **Angle** about the text's baseline;
lengths accept document units or explicit units such as `1 in`. Enable **Create Copy** to
leave the original in place and create a separately editable text. **Cancel** leaves the
design unchanged. Projected geometry remains protected from ordinary dragging.


**Project** (P) and **Intersect** bring model edges into the sketch as linked geometry that
follows later changes. Sketching on a face projects its edges automatically.
A projected edge stays the edge that was picked, wherever a later change moves it; when that
edge no longer exists, the projection keeps its last shape and the sketch says it was lost.
For Project, select visible source curves or points from another sketch, model edges,
faces, vertices or bodies, or axes and planes, then press P. The entire supported selection
is projected in one undo step. A rejected projection keeps the selection for correction;
an empty or mixed selection starts the ordinary picking tool. External sketch selections
retain their source sketch, even when the active sketch uses the same point or curve names.

A sketch on a construction plane built from the model (through a sketch line, through three
points, midway between two faces…) projects, intersects, includes and holds text like a sketch
on any other plane, and its links follow when a parameter moves that plane.

Run from a script or the control channel, every command that adds sketch geometry returns the
ids of what it added (`created: {points, curves, constraints, dimensions}`). Ids are stable and
never reused within a sketch, even after a delete, so a script can dimension or constrain the new
geometry right away.

Extend finds intersections with exact ellipses, rational conics and spline spans. It keeps the
original line or circular support and extends only beyond the selected endpoint; conflicting
constraints or an ambiguous overlap leave the sketch unchanged.

Closed convex chains of tangent lines and circular arcs support inward offsets that consume
a rounded corner. When the offset passes the corner radius, the neighbouring straight
supports meet at a sharp corner. The surviving lines and endpoint IDs stay stable; the
retired arcs return under their original IDs if a later edit restores their radius.
Whole-boundary profile selections follow this transition through save and reopen, so an
extruded lip can remain associative. Existing eligible closed analytic offsets upgrade
without changing their saved geometry or IDs.

This transition requires a closed convex boundary with tangent junctions and a certified
nonempty region. An extinguished arc referenced individually by a constraint or another
offset cannot retire. Creating another offset from an already sharp retired boundary also
requires breaking its association first. Open, nonconvex and mixed free-form chains keep
their existing behavior. Break Offset Association freezes the current boundary, including
private arc centres and construction flags, for independent editing.

Offsets of smooth ellipses, conics and splines follow their source after dimension changes,
dragging and reopening a design. Their stored cubic spline has a whole-span normal-distance
error bound of 0.002 mm. Singular tangents, curvature cusps, possible self-intersections and
excessive subdivisions are refused. Offset these curves individually: mixed chain junctions
currently require separate geometry. `sketch.offset.break` releases an offset for independent
editing, and undo restores its association. Older independently sampled offsets remain independent.
Smooth open and newly created smooth closed offsets can be offset again, including bounded
refinement after a source edit. New closed outputs use a periodic cubic fit with continuous
tangent and curvature at the seam. Break Offset Association preserves that interpolation,
and its closing point remains joined when dragged. Deleting an output also removes its
unused private fit points; deleting a source with a retained dependent is refused.

For explicit closed fit splines, `sketch.spline.fit_point` accepts `periodic: true` and at
least three distinct cyclic samples. It closes the loop automatically. Existing calls and
older saved closed splines retain natural interpolation; older associative closed offsets
also retain their saved natural fit. A legacy seam without a unique normal still refuses
another offset. Periodic intent is stored on each curve and survives save, reopen and Break.
While drawing a fit spline, click its starting point after three distinct samples to close
it smoothly. Right-click finishes an ordinary natural fit. Offset accepts both ellipse
handles and dense fitted curves; the second click sets the side and distance from the curve.

While drawing in a planar sketch, **Construction** (X) changes the line type without
ending the tool or losing picked points and inline values. The preview and hover feedback
keep construction dashes and centerline dash-dot patterns visible. With curves selected,
Construction changes those curves as before.

Enable **3D Sketch** in the Sketch Palette to draw lines, fit splines and points off the plane.
While placing these points, you can snap to points on other visible sketches. For a bridge
line, start Line and click an endpoint on each source sketch; each endpoint uses its source
plane, even when that plane differs from the active sketch. These snaps capture positions;
they do not create associative links to the source points.
New 3D geometry follows its sketch's component placement, including the selected occurrence
and pending position changes. Click a drawn fit point to show its move triad; its arrows move
along world X, Y and Z. Scripts also supply world coordinates to `sketch.line3d`,
`sketch.spline3d`, `sketch.point3d` and `sketch.move3d`. Activate the desired occurrence before
editing an explicit sketch belonging to a component with multiple placements.

Older drawn or detached 3D wires keep their saved world positions. Included and surface-linked
wires with an explicit component marker keep that meaning after the link is broken. Old files
whose links became lost after an incorrectly scoped cross-component refresh may require a
fresh link once the source is available; their stored point values are preserved on opening.

## Constraints and dimensions

**Horizontal/Vertical** accepts a click on a line or clicks on two distinct sketch points,
including the origin, endpoints and circle centres. For two points, it chooses horizontal
or vertical alignment from their current positions. Esc cancels a pending first point.

The solver is SolidCraft's own (damped Gauss–Newton with degree-of-freedom analysis). Constraints:
coincident, horizontal/vertical, parallel, perpendicular, tangent, equal, concentric, collinear,
midpoint, symmetry, fix, curvature (G2) and polygon. **Sketch Dimension** (D) adds linear,
aligned, horizontal, vertical, radius, diameter and angle dimensions. Each is a parameter
(`d1`, `d2`, …), so it can hold an expression.

Press D, click what you want to measure and type the value: as soon as the click settles the
dimension (one line, circle or arc, or the second point or line of a distance or angle), its
value box takes the keyboard with the measured value selected, so typing replaces it. Numbers,
units (`1 in`), expressions (`2*width`) and parameter names all work; letters go into the box,
not to tool shortcuts. While you type, the dimension text follows the cursor, and Enter places
it there (or in its default place if you haven't moved the cursor) as one undo step. Esc
cancels the dimension.

You can still place the text first. Before you type, a click on empty sketch space puts the
text there and keeps the value box focused; a click on another point or line makes the
dimension a distance or angle instead; Tab leaves the picks waiting for a click to place the
text.

Between two points, or on a line, that don't line up with an axis, where you place the
dimension picks what it measures: across from them, the distance between them; above or below
them, the horizontal distance; beside them, the vertical distance.

A dimension the sketch can't take, because the geometry is already fixed by other constraints
and dimensions, is added as a driven (reference) dimension instead: it is shown in
parentheses, measures the sketch, and has no value to edit.

For **Coincident**, pick two points, a point and a curve (in either order), or two
distinct straight lines. Two lines join their closest pair of endpoints; the first
picked line stays in place when the existing constraints allow it. Pick endpoints
explicitly when you want to connect a different pair.

Horizontal/Vertical also applies immediately to one selected line or two distinct selected
points. Shift-click to add the second point before choosing the constraint.

For Parallel, Perpendicular, Equal, Tangent, Concentric and Collinear, select exactly two
sketch curves (Shift-click to add the second) and choose the constraint to apply it immediately.
You can also start the constraint first and then pick its curves. An incomplete or mixed
selection leaves the tool ready for ordinary picking.

Coincident accepts an exact selection of two points or a point and a curve. Midpoint
accepts one point and one line, in either selection order. Symmetry accepts two distinct
points and one symmetry line, regardless of where the line occurs in the selection.
Successful constraints clear the selection; rejected constraints preserve it.

Under-constrained geometry is blue and fully constrained geometry turns dark, and the status
shows the remaining degrees of freedom. A constraint or dimension that would over-constrain the
sketch is refused rather than applied. Drag a point to move it; the solver keeps every
constraint while you drag.

## Profiles

Closed loops become profiles automatically, including regions split by crossing lines and
islands inside outlines. Extrude, revolve, sweep and loft pick profiles.

A feature remembers a picked profile by the curves around it, not by where it was clicked:
`"profiles": [{"curves": ["l1", "l2", "a1", "l3"]}]` (or `[{"curves": ["c1"]}]` for a circle)
picks the region whose outer loop is made of exactly those curves, or the pieces of them that
other curves cut off. It is found again on every recompute, so the feature keeps the same region
when a parameter moves the geometry. Islands inside the region are its holes. When no region, or
more than one (both halves of a circle cut by a line), is bounded by exactly those curves, the
feature reports an error rather than taking another region. Profiles picked by index or by point
(`[{"point": [x, y]}]`) are stored the same way when the feature is made.

## Measurement units

Change the current design through **Document Settings > Units** in the browser.
The units preference applies to new designs. Existing sketch annotations, live drawing
lengths and Measure follow the current design units without resizing geometry.
Inches display at least three decimal places and retain a meaningful fourth digit;
a higher display precision can be selected in Preferences. Bare length values typed
in the dimension tool use the current design units. Explicit units such as `25.4 mm`
remain valid in an inch design. Angular dimensions keep their angular units. Deliberately typed four-place inch literals
retain trailing zeros. Component/canvas distance controls and inspection, parts-library
and flat-pattern measurements follow design units too; parameter expressions and metric
standard names retain their explicit authored units.
