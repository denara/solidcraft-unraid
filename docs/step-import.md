# STEP import

SolidCraft reads STEP (ISO 10303-21; AP203, AP214 and AP242 geometry) with its own reader in `crates/kernel/src/step_in` (written from the public standards; no third-party STEP code). A STEP file becomes an **Import** base feature on the timeline: it holds the file's B-rep (the STEP text is kept in the design, so the design reopens without the file) and later features — fillets, holes, cuts sketched on its faces, shells — build on its bodies.

- **Open**: File → Open (`doc.open`) accepts `.step`/`.stp` in any case and starts a new design; `solidcraft part.step`; `solidcraft-cli eval part.step`, `solidcraft-cli run script.json --in part.step`; the MCP `open` tool.
- **Insert** into the current design: File → Insert STEP… or SOLID → INSERT (`file.insert_step {path}`).
- **Drag and drop** a STEP file on the window: inserted when the design has features, opened otherwise.

Read: manifold solid breps (with voids), faceted breps, shell-based surface models; planes, cylinders, cones, spheres, tori, B-spline and rational B-spline surfaces (with knots, Bézier, uniform, quasi-uniform), surfaces of revolution and linear extrusion, trimmed surfaces; lines, circles, ellipses, B-spline curves, polylines, trimmed/surface/seam curves; length and angle units (SI prefixes, conversion-based units such as inches); product names (body names come from the solid's name, else the product's); surface colours; assemblies (next-assembly-usage occurrences placed by item-defined transformations, and mapped items) — the product tree is kept in the feature for the components milestone. Files are untrusted: sizes, entity counts and nesting are capped and every kernel call is guarded. What cannot be read becomes a warning on the feature (a face left out gives an open body); not read yet: offset surfaces, pcurve-only edges.

## Imported appearances

Imported body colours and face colours are retained through later features. Body
colours remain the fallback under explicit design appearances and materials; face
colours follow persistent face names. A removed face has no colour assignment to
display on the resulting body.

## IGES

IGES 5.3 files (`.igs`/`.iges`) open, insert and drop the same way: `crates/kernel/src/iges_in.rs` (written from the published specification) restates the file as STEP and the STEP reader reads it, so the feature holds STEP text. Read: manifold solid B-reps (186) with their shells, faces, loops, edge and vertex lists; trimmed (144) and bounded (143) surfaces, and bare B-spline patches, sewn into a solid by their shared edges (an open body when they do not close); lines, circular arcs, ellipses, composite curves, rational B-spline curves; planes (108, 190), cylinders, cones, spheres, tori, rational B-spline surfaces, surfaces of revolution and tabulated cylinders; transformation matrices; units; colours (314 and the predefined numbers); body names (name properties, else labels). Not read yet: ruled and offset surfaces, curves on surfaces given only in parameter space. Export (`file.export` to `.igs`) writes each body as a manifold solid B-rep on the same surfaces a STEP export uses, with its name and colour.
