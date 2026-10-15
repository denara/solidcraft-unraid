# Vendored third-party crates

Copies of third-party crates that SolidCraft builds with local fixes, wired in with
`[patch.crates-io]` in the workspace `Cargo.toml`. They keep their own licence; changed lines
are marked `SolidCraft:` in the source.

## truck-shapeops 0.4.0 (Apache-2.0, © RICOS Co. Ltd., https://github.com/ricosjp/truck)

Changes:
- `transversal/intersection_curve/mod.rs`: reject empty/single-point polylines and
  collapsed or non-finite segments before endpoint indexing and normalization. A hidden
  constructor re-export lets the kernel workspace tests exercise the actual vendored code.
- `transversal/integrate/mod.rs`: mesh both shells with `robust_triangulation` (faces made by
  earlier booleans have boundaries that lie on their surface only within tolerance).
- `transversal/divide_face/mod.rs`: when projecting a loop onto a face's surface, fall back to
  the nearest parameter when the exact search fails (intersection curves are approximations).
- `transversal/integrate/mod.rs`: the result's shells and faces keep the order of the shell
  they come from (truck's connected components followed hash order of face addresses, so a
  boolean's faces came out in a different order on every run).
- `transversal/loops_store/mod.rs`: an intersection edge added to a face at a vertex that more
  than one wire visits (several curves end on it: the touching point of two equal cylinders) goes
  into the corner that holds its direction, swept counterclockwise about the face's normal from
  the wire's outgoing edge to the way back along its incoming one (`add_edge_by_angle`, told on
  the polygon loops and followed by the geometry loops). Taking the first visit of each end
  joined wires that are different regions of a face and left a wire visiting a vertex twice.
  With no direction to go by, the edge goes where both of its ends are in one wire.
- `transversal/divide_face/mod.rs`: a face piece whose wire is not simple makes the operation
  give up (`Face::try_new`) instead of panicking.
- `lib.rs`: `tr!` reports where an operation gives up when `SHAPEOPS_TRACE` is set; compiler
  warnings no longer fail the build.
- Face and edge history (`transversal/history.rs`, new): `and_with_history` / `or_with_history`
  also return, for each face of the result (in `face_iter` order), the operand face it is a
  region of and whether it is that face unchanged, and for each edge (distinct edges in order
  of first use along the faces) the operand edge it is (a piece of) or the two operand faces
  whose intersection made it. The code that creates faces and edges records them as it goes:
  `divide_face` (face pieces), `loops_store` (intersection edges, split edges, edges rebuilt on
  a moved vertex), across the passes that fold in the operands' extra shells. `and` / `or` are
  wrappers that drop the history; the `*_with_history` forms return the same solid with the
  history as an `Option` (`None` where a face or edge went unrecorded).

## truck-stepio 0.3.0 (Apache-2.0, © RICOS Co. Ltd., https://github.com/ricosjp/truck)

Only the writer (`out`) is used; SolidCraft reads STEP with its own reader.

Changes:
- `out/geometry.rs`: an `INTERSECTION_CURVE`'s second surface was written at the first
  surface's entity index, so every boolean-made edge produced duplicate entity ids.
- `out/geometry.rs`: surfaces of revolution carry the right `same_sense` for inverted and
  mirrored revolutions (ISO 10303-42 parameterises them by angle then profile, so their normal
  is the opposite of truck's; a mirroring transform reverses it again), and the modeling-surface
  wrapper passes the revolution's sense through instead of always `true`.
- `lib.rs`: compiler warnings no longer fail the build. Example and test targets are dropped
  (their sources are not vendored).

## truck-meshalgo 0.4.0 (Apache-2.0, © RICOS Co. Ltd., https://github.com/ricosjp/truck)

Changes:
- `tessellation/mod.rs`, `triangulation.rs`: a separate provisional `preview_face`
  path accepts at most 4,096 already sampled boundary points and a fixed grid of
  at most 16 subdivisions per direction, including periodic seam connections.
  It does not run adaptive subdivision or grid balancing. The kernel caller bounds
  projection complexity and total allocation before using it; ordinary modelling
  tessellation keeps its existing accuracy policy.
- `tessellation/triangulation.rs`: exact, robust and provisional-preview projections prefer an
  equivalent in-domain parameter at collapsed surface edges, and snap to exact bounded
  endpoints only at collapsed edges when that improves the spatial fit within the existing
  search tolerance. Closed spline seams retain their distinct parameter endpoints.
  This prevents huge arbitrary pole parameters from corrupting trimmed meshes (#796),
  while retaining genuine extrapolation for unbounded planes. Work is bounded to four
  endpoint evaluations per projected point; no search or tessellation tolerance is relaxed.
- `tessellation/triangulation.rs`: a boundary loop is projected onto its surface point by point,
  each searched from the one before; the first has no hint. When that walk fails, it is tried
  once more from the first point an unhinted search finds. An unhinted search can miss a point
  that lies on the surface, such as the corner of a fillet strip on a surface extended past it,
  and the face was then left unmeshed, which also failed every boolean with the body (#882). A
  walk that succeeds is kept as it was: where a loop starts decides how it is laid out across a
  periodic surface's seam.
- `tessellation/triangulation.rs`: the inside test of a face's boundary skips loops whose
  parameter box is clear of the point (it tested every segment of every loop, so a face with
  hundreds of holes meshed in quadratic time).
- `tessellation/triangulation.rs`: meshed faces and edges are built unchecked: faces whose loop
  runs along a seam edge both ways and closed edges (a full circle on one vertex) are valid,
  but the debug-build checks rejected them (debug builds could not mesh them).
- `tessellation/triangulation.rs`: parameter searches that land outside a surface's bounded,
  non-periodic domain are rejected (a hinted B-spline search can run off and extrapolate to
  thousands of parameter units, and meshing that grid never finishes); the last-resort nearest
  search is clamped into the domain.
- `tessellation/triangulation.rs`, bounded work (hostile files must not hang): edge polylines
  and surface grids come from our own bounded divisions (at most 20 000 points per edge, a
  grid budget per face shared out of 10 million points per shell, 24 refinement rounds; a
  point that does not evaluate counts as flat instead of recursing). Surface grids refine only
  the direction that bends, and a ruling left undivided gets cells comparable to the other
  direction (long triangles from a trim loop to the grid's far ends cut through cylinders).
- `tessellation/mod.rs`, `tessellation/triangulation.rs`, for display: a `Deflection` is the
  chord tolerance plus an optional bound on the angle one facet spans, and
  `RoundMeshableShape::robust_triangulation_within` divides by it: an edge or a grid direction
  within the chord tolerance is still halved while a piece turns more than the angle, until its
  sag is under a floor (a radius that is small beside the tolerance was one flat facet).
  Bounded work: a face that would be bounded by more than 4096 segments has its curved edges
  divided at twice the angle, and again, down to the chord tolerance alone, and the angle
  stops refining a face's grid at 16 384 points. Each edge is still divided once for both its
  faces. `triangulation` and `robust_triangulation` divide by the chord tolerance alone, as
  before.
- `lib.rs`: compiler warnings no longer fail the build. Example, test and bench targets are
  dropped (their sources are not vendored).

## matext4cgmath 0.1.0 (Apache-2.0, © RICOS Co. Ltd., https://github.com/ricosjp/matext4cgmath)

truck-geometry uses it for a matrix's operator norm (an intersection curve scales its tolerance
by it whenever it is moved).

Changes:
- `solver.rs`: the Newton refinement of cubic and quartic roots stops after 100 steps. Near a
  double root it could cycle forever: moving a body whose edges a boolean made (the retries
  move both bodies and the result back) hung the app.
- `lib.rs`: compiler warnings no longer fail the build. Test targets are dropped (their sources
  are not vendored).

## egui-wgpu 0.36.2 (MIT OR Apache-2.0, Emil Ernerfeldt and contributors)

Original package sources from https://github.com/emilk/egui/tree/0.36.2/crates/egui-wgpu,
with both original licence texts retained. No images or bundled documentation are copied.

SolidCraft changes:
- `src/lib.rs`: use 1x MSAA on the selected OpenGL adapter before creating pipelines,
  exposing the effective sample count so host paint callbacks can match it.
- `src/winit.rs`: use that same effective count for depth and resolve attachments.
- Other backends retain requested MSAA, without another adapter or device probe.
- `src/lib.rs`: clamp MSAA to the largest sample count the device supports for the surface and
  depth formats (`supported_msaa_samples`, `RenderState::msaa_supported`): a saved 8x on a
  device without it failed pipeline validation, a panic at start.
- `src/setup.rs`: the default device requests `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` when
  the adapter has it, so 2x and 8x work where the adapter supports them (WebGPU only
  guarantees 1x and 4x).
- `src/setup.rs`: the default device asks for the adapter's largest 2D texture (at least 8192)
  instead of exactly 8192.
- `src/lib.rs`, `src/winit.rs`: `set_startup_hook` is told before each step of getting the
  device and the window's surface up (instance, surface, adapter request, the chosen adapter,
  device, renderer, surface configure, first present), so the app's crash journal can say which
  step a crash inside the graphics driver happened in.
- `src/lib.rs`, `src/winit.rs`: a window larger than the device's largest texture gets a surface
  clamped to it (`clamp_surface_extent`, aspect ratio kept) and egui is drawn at the matching
  smaller scale, instead of a `Surface::configure` validation panic (#869). The resize path and
  an unsupported surface no longer unwrap.
