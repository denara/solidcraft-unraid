# FreeCAD oracle

Reference values for some of the cases in [`tests/oracle/`](../../tests/oracle/) are computed by
building the case's own `recipe.json` a second time, in a different kernel: FreeCAD
(OpenCASCADE). Those cases have `"source": "freecad <version>"` in their `expected.json`.
The cases with closed-form answers are worked out by hand instead (`"source": "analytic"`,
see [`tools/oracle-analytic/`](../oracle-analytic/)).

## FreeCAD is an external tool, not a dependency

FreeCAD is run as a separate program (`freecadcmd`) that executes `build.py` through FreeCAD's
public Python scripting API (Part, PartDesign, Sketcher, MeshPart). SolidCraft does not use,
link, vendor or ship any code, library or binary from FreeCAD or OpenCASCADE, and nothing in
this directory is part of the build. FreeCAD's LGPL licence therefore does not apply to
SolidCraft's code. The scripts here are our own; they were written against FreeCAD's public
API documentation only, never its sources.

Only values for our own recipes are produced: the numbers in `expected.json` (body count,
volume, area, centre of mass, bounding box) and the triangulated surface in `reference.stl`.

## Files

- `build.py`: runs inside FreeCAD. Reads a case's `recipe.json` and builds the part with
  FreeCAD's modelling operations, following the meaning SolidCraft gives the recipe
  (`apps/solidcraft-cli/src/recipe.rs`): sketches (solved by FreeCAD's sketcher when the recipe
  constrains them, otherwise at their recorded coordinates), extrude (distance, symmetric, two
  sides, through all, taper), revolve, sweep, loft, holes (simple, drilled tip, counterbore,
  countersink), rectangular/circular/path patterns and mirror (each source feature's tool moved
  and its operation applied again), combine, shell, draft, fillet and chamfer (edges picked
  nearest the recipe's point, tangent chains followed), split body and construction planes.
  It then measures the result.
- `emit.py`: prints a result or writes it as `expected.json` / `reference.stl`.
- `run.sh`: the entry point.

## Running it

```sh
tools/freecad-oracle/run.sh tests/oracle/17-loft tests/oracle/42-overlapping-patterns   # print
tools/freecad-oracle/run.sh --write tests/oracle/17-loft                                # write
```

Without `--write` each case prints one line. With `--write` it writes `<case>/expected.json`
(`source`, `method`, `body_count`, `volume_mm3`, `area_mm2`, `center_of_mass`, `bbox` from
`optimalBoundingBox()`) and `<case>/reference.stl` (binary, linear deflection 0.005 to 0.02 mm,
at most about 400 KB; when that is not possible the STL is skipped and `note` says so). It never
overwrites an `expected.json` whose source is `analytic`. Per-case comparison settings
(`compare`, `tolerance`, `note`) live in `CASE_META` in `build.py`.

`freecadcmd` is found through `$FREECADCMD`, then `freecadcmd` on `PATH`, then an AppImage
extracted in this directory (`squashfs-root/`, ignored by git). Locally: install FreeCAD 1.x from your distribution or extract the
AppImage (`./FreeCAD_*.AppImage --appimage-extract`) and point `FREECADCMD` at
`squashfs-root/usr/bin/freecadcmd`. In CI (Debian/Ubuntu): `apt install freecad`, then run the
script; it needs no display.

The values were produced with FreeCAD 1.1.4. A different FreeCAD version may differ in the last
digits; regenerate only when a recipe changes, and check the new values against SolidCraft
(`solidcraft-cli oracle`) before committing them.
