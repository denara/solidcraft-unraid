# Import and export

| Format | Read | Write |
|---|---|---|
| `.solidcraft` (versioned container or JSON design) | yes | yes |
| `.json` (self-contained plain JSON design) | yes | yes |
| STEP (AP203, AP214, AP242) | solids, assemblies, units, names, colours | AP242 with names, colours and the component tree |
| IGES 5.3 | manifold solids, or trimmed surfaces sewn into solids | manifold solids with names and colours |
| 3MF, STL, OBJ | mesh bodies | yes |
| DXF, SVG | into a sketch | sketches and flat patterns as DXF |
| PNG, JPEG | canvases and decals | |

Opening a STEP or IGES file creates a design with the file's bodies as a base feature, and
inserting one adds them to the current design. Later features build on them. Mesh bodies render,
measure, move and export, but solid features need B-rep bodies.

For closed mechanical meshes with a constant cross-section, use **Mesh → Modify →
Convert Mesh**. Select the mesh body and set the fitting tolerance. The conversion
rebuilds planar faces and circular through-walls, replacing the mesh with a solid
that accepts solid modeling operations. The imported triangles remain embedded
upstream: changing their scale, undoing the conversion or reopening the design
rebuilds the result through the timeline. Double-click the conversion to edit its
tolerance or source body.

The tolerance bounds circular reconstruction against the original chords. Coarse
polygons remain polygons when rounding would exceed it. Open/nonmanifold meshes,
tapers, stepped parts and organic scans are refused by this first reconstruction
method; it does not silently substitute a faceted solid. Conversion currently
accepts at most 50,000 triangles and a bounded number of boundary segments.

Saves are atomic: a crash at any moment leaves either the old file or the new one, and the
replaced version is kept as `<file>.bak`. Unsaved changes are autosaved to a recovery folder,
and the next launch offers to recover them. Older design files are upgraded on open, and files
from a newer version are refused with a message.

**Save** writes a container by default; files named `.json` keep JSON on subsequent saves.
**Save As** offers a container or plain JSON on
desktop; **Save As JSON** is also available in the File menu on desktop and web. Choose
the `.json` extension for plain JSON. **Export Design as JSON** writes an independent,
self-contained copy without changing the current file. Scripts can explicitly pass
`"format": "json"` to `file.save` or `file.save_as` even with a `.solidcraft` name.
Opening detects the format from the contents, so existing JSON designs keep working.
Containers favor fast reading and saving by default. Scripts can request
`"compression": "compact"` when file size matters more than CPU time.

Containers deduplicate identical STEP, mesh and image payloads. Mesh coordinates retain
their original double precision; this is not a conversion to STL. Geometry still rebuilds
from the timeline when opening. When a saved display preview is available, desktop Open
shows it while checking geometry; editing starts after the complete rebuild. Cancelling
keeps the current design. Previews cannot be edited or measured. Scripts can opt in to a disposable geometry
cache with `cache: true` on warm saves; saving never rebuilds cold geometry just to
create one. Ordinary saves and browser downloads default to `cache: false`. An additional display-mesh cache remains opt-in
with `preview: true`. Unsupported or mismatched caches are ignored. Browser opening
still rebuilds synchronously.
Warm saves with explicit `cache: true` can also include a small PNG thumbnail. It travels with the file, so
Home can show the recent design without reconstructing it when no local picture
is available. Missing or incompatible pictures use the usual thumbnail generation.
The default `cache: false` omits the portable thumbnail too.
The repository's `docs/design-container.md` describes limits and encoding details.

Current designs use format 17, including face anchors on a piece of a face by its own name, sheet metal rims and bend ends, bend edges for Convert to Sheet Metal, the names of projected edges, physical materials defined in the design, the font of sketch text, sheet metal corner relief shapes, sketch face anchors on cylinder caps, sketch axis
links, persistent
names from exact face history, exact planar shell-face anchors, persistent analytic offset boundary roles and
association profile references, durable sketch face anchors, periodic fit splines and versioned
revolve conventions. It also retains recorded body transfers, explicit 3D curve coordinate
spaces and certified face bindings, alongside semantic configuration styles, associative
free-form offsets and cylindrical emboss selections. Older ownership maps, geometry and
expressions retain their saved meaning. Legacy 3D curves with retained associations receive
their producer's coordinate-space marker without changing any saved points; detached curves
retain world coordinates. A lost cross-component association refreshed by an older build may
need an explicit relink or refresh when its source returns. Loading never guesses a numerical
correction. Association records, coordinate arrays and placements are bounded and checked.

## Saving in the web app

In the web app, **Save** and **Save As** keep designs in this browser's private storage;
they do not open a dialog for choosing a folder on your computer. Clearing the site's data
can remove those designs. Download a copy to keep it outside the browser or move it to
another device.

1. Open the File menu and choose **Open** to show **Files in this browser**.
2. Choose **Download open design** for the current design, including its unsaved changes,
   or **Download** beside a stored file for its last saved contents.
3. Find the `.solidcraft` file in your browser's downloads. Whether a folder chooser appears
   depends on your browser's download settings.

Use **Open from computer…** in the same panel to reopen a downloaded design. The web app
works on a browser-stored copy; later saves do not update the original downloaded file.
Use **Rename** beside a stored file to change its name. Exports such as STEP and STL are
also downloaded.
