# Solid features

Features live on the **timeline** and rebuild incrementally from the first one that changed.
Each one can be edited (double-click), suppressed, renamed, reordered (if nothing it depends on
moves after it), rolled back to, and deleted. Deleting asks first when other features depend on
it.

| Panel | Features |
|---|---|
| Create | extrude (one side, two sides, symmetric, taper, start offset, to a distance or through all; new body, join, cut, intersect; thin walls along curves), revolve, sweep, loft, box, cylinder, sphere, torus, coil, pipe, rib, web, emboss, hole (simple, counterbore, countersink, tapped; on faces or at sketch points), thread, rectangular/circular/path patterns, mirror |
| Modify | press pull, fillet, chamfer, shell, draft, scale, combine, split body, offset face, replace face, move/copy, align, remove |
| Construct | offset plane, plane at angle, plane along path |
| Inspect | measure, section analysis, interference, curvature comb and map, zebra, draft, minimum radius, isocurves, centre of mass |

Extrude automatically selects **Join** for additions when the active component already has a
solid, including sideways elbow connections and screw bosses that meet only part of a face.
Extruding into existing material selects **Cut**. In an empty component it selects **New Body**;
choose **New Body** explicitly to keep an addition separate. Changing the default does not alter
operations already saved in a design.

A taper slopes the sides of any profile, sketch text included: lines, arcs and spline or conic
curves move in or out together, and a flat tip shorter than the taper closes to a point. On
spline and conic sides the slope is approximate: the moved outline is a curve of the same kind
within 2 % of the taper's offset (or 10 µm) of the true one, a curve that turns too much for
that being cut into pieces first; where a curve bends tighter than the offset, the offset folds
and is smoothed over. A taper steeper than the profile's details (a stroke, a curve or a gap
narrower than it) is refused with that reason; a smaller taper or distance builds.

**Thin Extrude** (`thin: true` on `solid.extrude`, the Thin Extrude box in its dialog) builds a
wall of constant thickness along sketch curves instead of filling profiles: open chains (a single
wall) and closed loops (a ring), lines, arcs, circles, ellipses and splines. In the dialog, a
picked profile stands for the curves around it (its outline and its holes). The wall lies on the curves' centre, inside,
outside, or on both sides with two thicknesses (`side`: `center`, `inside`, `outside`,
`two_sides` with `thickness2`). A closed loop's inside is the area it encloses; an open chain's
is the side it bends toward (the inside of a U), and a straight line's is the left of its first
curve as drawn. Curves that meet end to end are one chain whose corners are mitred; walls that
cross or meet (a ring and a cross, a honeycomb) become one piece. Every extent, operation and target of an extrude applies, and a taper
for walls of straight lines or of whole circles. A wall thicker than the
radius of a curve it follows on the inside is refused. Spline walls follow the exact offset
within 0.0001 mm.

Editing an extrusion retains its signed distances, From/To Object inputs, taper expressions
and explicit body targets. Revolve edits retain world, sketch and explicit-line axes, both
angle expressions and To Object targets. An unchanged **All profiles** selection continues
to include profiles added later. Cancel restores the complete timeline; a successful edit is
one undo step, including downstream parameter changes. Editing a placed component previews the
edited feature before its downstream history. Cylindrical Emboss profile guides follow the
sketch's chosen occurrence frame.

Newly selected adaptive profiles store complete outer and hole source identities when the
entire boundary can be qualified. Changing an ellipse or periodic offset after extrusion can
then change its spline span count while keeping the same selected material region. A partial,
intersected or ambiguous boundary cannot satisfy that whole-source reference; changing the
hole identities also makes it lost. Older curve-span selections retain their exact stored
multisets. If an older feature has already lost its profile after subdivision changed, edit
the feature and reselect the intended current region to capture the new identity.

New symmetric solid extrusions treat the entered distance as the total thickness and split it evenly
around the start plane: 10 mm spans -5 to +5 mm on an XY sketch. Switching from One Side to
Symmetric keeps the entered total thickness. Through All still reaches targets in both
directions, and Two Sides retains its independent distances. Symmetric extrusions saved before
this change keep their original per-side distances when reopened or edited. Selecting another direction
in the feature editor uses the current total-distance convention.

New symmetric revolves split the entered angle evenly around the sketch profile, regardless
of which way the selected axis points. Two Sides measures the second angle backward and the
first angle forward along the sketch-normal sweep direction. Profiles on opposite sides of
an axis each use their own sweep direction. Designs saved before format 11 keep their original
angular placement when opened, edited or copied; creating another revolve uses the new behavior.
To Object follows that same direction for each selected profile, including curved target faces.
Pick a target away from the axis so its angular position is defined.

For cylindrical **Emboss**, select a supported exterior cylinder face and a tangent sketch
profile. **Deboss** cuts radially into the wall. Orthogonal line profiles, including holes and
concave outlines, wrap with exact radial walls; the profile must stay clear of seams and ends.
Complete cylinders and coaxial sleeves are supported. Capture pending component positions
before applying. The feature stores the selected target and author occurrence paths, so edits
and reopening retain the chosen instances. Other curved surfaces and free-form profiles remain
unsupported. A new Emboss without an explicit face infers a unique eligible supported cylinder;
select the face when the target is ambiguous. Older saved features without a face retain their
planar placement.

A **Hole** with **Single** placement drills where you pick a planar face. A pick on or near a
point of a shown sketch that lies on that face, such as a circle's centre, places the hole
exactly at that point, as **Multiple** placement picks it; **Select Other** (press and hold) also
offers the face where you clicked. The corners of text outlines don't count, nor do projected
points while the sketch hides projected geometry. The hole keeps that place on the face; to have
holes follow later edits of the sketch, place them with **Multiple**.

Fillets and chamfers handle straight edges between planar faces, whole smooth loops, runs of
neighbouring edges with mitred or spherical corners, and closed chains of curved edges (a rolling
ball). See the roadmap's known limitations for the rest.

**Curvature (G2)** in the Fillet dialog's **Radius Type** blends straight edges between two
planar faces with an exact quintic section. The tangent plane and curvature match each side
face: curvature goes to zero at both contact rails, unlike a circular G1 fillet. Each end
must meet one planar face perpendicular to the edge. Curved edges/faces, sloped or multi-face
ends, and intersecting corner blends are not supported; they return an error, not a G1 fallback.
**Equivalent Radius** sets the same contact setback as a circular fillet (for a right angle,
setback equals the entered value); it is not a constant curvature radius. Reduce/Taper to Fit
are unavailable for this type.

Scripts and MCP use the same command, for example on a 20 mm box:
```json
{"command": "solid.fillet", "params": {"edges": [[20, 0, 10]], "radius": 2, "type": "curvature"}}
```
`type: "g2"` is an alias. Existing fillets still default to circular, constant-radius G1 blends.

Feature references to faces and edges are stored by name, so an upstream edit that moves geometry
keeps them on the right face (see [Persistent naming](../dev/naming.md)). When a reference can't
be found, the feature warns instead of silently picking another face.

**Plane Along Path** accepts one sketch curve (including a drawn 3D wire) or model edge.
Pick the path in the viewport and enter **Position (0 to 1)**: 0 starts the path, 1 ends it,
and 0.5 places the plane halfway along its length. The plane is perpendicular to the path's
local segment. Preview does not change the design; OK creates one editable timeline feature.
The plane follows later path edits. Curved paths use their sampled polyline; absolute distance,
reference-object placement and multi-curve chains are not yet available in this dialog.

## Editing a feature from a script

For `solid.pattern.circular`, `axis` accepts `X`, `Y`, `Z`, or `{origin, dir}`.
An explicit origin must contain three finite numeric coordinates in millimetres
(`[x, y, z]` or `{x, y, z}`); omitting it uses the world origin. Expressions and
construction-axis references are not supported. Invalid origins are rejected rather than
silently replaced with zero.

`timeline.inspect {feature}` returns a feature's `definition`: the fields `timeline.edit` changes,
with their current values. `timeline.edit {feature, set}` merges `set` into that definition
(objects are merged field by field, anything else replaces the value), rebuilds the feature and
reports `recomputed` and `restored`. The feature's type cannot change, and sketch geometry is
edited with the sketch commands. Expressions are text (`"30"`, `"30 mm"`, `"width / 2"`,
`"45 deg"`); a bare number is a length in the design's units, or degrees for an angle.

```json
{"command": "timeline.inspect", "params": {"feature": "Slab"}}
→ {"type": "ExtrudeFeature",
   "definition": {"sketch": 1, "profiles": {"type": "all"}, "operation": "new_body",
                  "extent": {"distance": "10", "direction": "positive"}},
   "inputs": [{"label": "Distance", "param": "d1", "expression": "10", "unit": "mm"}], …}
{"command": "timeline.edit", "params": {"feature": "Slab", "set": {"extent": {"distance": "12"}}}}
```

`inputs` lists the feature's named inputs. `parameters.change {name: "d1", expression}` sets one
of those directly. Optional fields that are not set do not appear in the definition; they can
still be added through `set`. The fields of the common features:

| Feature | Fields |
|---|---|
| Extrude | `extent`: {`distance`, `direction`: positive\|negative\|symmetric, `distance2`, `start_offset`, `through_all`, `taper`, `to`, `to_offset`, `from`}; `operation`: new_body\|join\|cut\|intersect; `targets`; `profiles`; `sketch`; `thin`: {`curves`, `thickness`, `side`: center\|inside\|outside\|two_sides, `thickness2`} |
| Revolve | `angle`, `angle2`, `symmetric`, `axis`, `to`, `operation`, `targets`, `profiles`, `sketch` |
| Fillet | `radius`, `edges` (a point on each edge), `body`, `style` |
| Chamfer | `distance`, `distance2` or `angle`, `flip`, `edges`, `body` |
| Box | `length`, `width`, `height`, `corner`, `operation` |
| Cylinder | `radius`, `height`, `base`, `axis`, `operation` |
| Hole | `diameter`, `depth`, `position`, `direction`, `hole` (simple, drilled, counterbore, countersink), `thread`, `to` |
| Shell | `thickness`, `faces`, `direction`, `body`, `tangent_chain` |
| Draft | `angle`, `faces`, `neutral`, `pull`, `body` |
| Offset face | `distance`, `faces`, `body` |
| Pattern | `pattern` (its kind and counts/spacing), `features`, `bodies` |
| Mirror | `plane`, `features`, `bodies`, `combine` |

For any other feature, `timeline.inspect` shows the fields.

Where a command takes a face as a point (`face.delete`, `solid.shell`, `solid.draft`,
`solid.offset_face`, `solid.split_face`, the `faces` of `solid.replace_face`, and the `face` of
`sheet.convert`), a script can give the face's name instead, as `model.faces` lists it:
`{"command": "face.delete", "params": {"faces": ["F2:side#1", "F2:side#2"], "body": "Plate"}}`.
`model.faces` also lists a `point` on each face that picks it (the centroid of a face with a hole
in it is in the hole, that of a half cylinder inside the cylinder) and, for a cylindrical face,
its `cylinder`: `origin` and `axis` of its axis, and `radius`. A name that is more than one face
(the pieces of a split face, or one face of several bodies) is refused; `body` says which body to
look on.

Press **Delete** on anything selected (bodies, faces, sketches, features, planes, components,
canvases). It runs as one undoable step and lists the features that would go with it or fail.

Face Extrude, face Revolve, and Press Pull retain linked generated profiles. When a modified planar face has no construction certificate, these internal profiles use the legacy named-face attachment and projection behavior so upstream edits still update the plane and outline. This compatibility path is private to those modeling tools: newly attached user sketches require a certified face or an explicitly fixed plane. Legacy internal attachments can follow the nearest matching face after topology changes; inspect their result after major upstream edits. Activate the intended occurrence before generating a face profile from a repeated component; capture a separately moved source position before creating a legacy profile across components.

Negative Press Pull across supported aligned prismatic faces retains round through-holes,
including after a joined extrusion and in a rotated component occurrence. The cut partitions
the current material into axial sections, preserving the hole through undo, redo, and reopen.
Walls that cannot be qualified as a parallel extrusion still use the general Boolean path
and may report an unsupported coincident-surface operation atomically.
