# Live observation of MCP modelling sessions

Status: architecture proposal only. No live CAD viewer is implemented by this
document. The existing workspace streams file and session metadata; browser CAD
remains an independent application with independent documents.

## Existing building blocks

- The HTTP gateway runs a separate headless CLI worker for each MCP session and
  serializes exchanges with that worker: [sessions.rs](../apps/solvecraft-server/src/sessions.rs).
- After tool calls it probes the worker's actual model status. The
  [headless backend](../crates/mcp/src/backend.rs) tracks document and visibility
  changes, including unsaved work. Its observed revision is not a count of commands.
- [Scene construction](../crates/engine/src/view.rs) already includes visible
  bodies, edges and sketches. [Image capture](../crates/engine/src/capture.rs)
  produces PNG bytes without requiring a desktop window. CPU rendering omits
  some analysis colours, highlights and legends; it must identify its renderer.
- [Workspace SSE](../apps/solvecraft-server/src/workspace_live.rs) already supports
  cookie authorization, bounded subscribers, reconnects and a latest-state cache.
- The [browser app](../apps/solidcraft-web/src/main.rs) has its own engine session.
  The existing [native-design handoff](../apps/solidcraft-web/src/workspace_import.rs)
  opens a saved design as an independent browser copy, not as a view of a live worker.

## Recommended first stage: a read-only snapshot observer

Add an internal observer operation to the worker protocol that captures the current
in-memory scene without saving the design, changing its path or dirty state, moving
the agent's camera, or adding undo history. Return bounded PNG bytes and an observed
visual revision to the gateway. Do not implement this by invoking the public
file-writing screenshot tool with a generated project filename.

Schedule capture through the worker's existing serialization after completed model
changes, only while authorized viewers subscribe. Browser requests must not write
directly to worker stdin or occupy an independent competing MCP request. Its current
reader expects the response ID of the one outstanding exchange. Prefer a capture
notification and latest-image cache over letting every viewer trigger a render.

Keep at most one bounded image per observed session, tagged with session generation,
visual revision and render settings. Share that image across viewers. Coalesce rapid
changes, cap resolution and capture frequency, and allow at most one capture in
flight per worker. Slow clients receive the latest available state rather than an
unbounded frame queue. Apply a global memory budget and render concurrency budget;
stop generating images when the last viewer leaves. No generated observer image
belongs in the shared project directory or its file inventory.

Use SSE to announce that a newer image is available and a separate cookie-protected
HTTP endpoint to serve the image. Validate the browser login and permitted origin on
both endpoints. Use private, non-cacheable responses; never put the MCP Bearer token
or a usable MCP session credential in browser storage, event payloads or URLs.
Resolve an opaque observer handle to the current server-side session and recheck
authorization instead of treating a dashboard display ID as an access credential.
The present shared-token installation has no per-user project ACL: adding separate
user identities later would require corresponding observer authorization.

Invalidate cached output when its worker ends or is replaced, including idle expiry,
timeout, failure and container restart. Tell existing viewers that the session ended;
do not silently attach them to another worker or continue serving a stale model as
live. Login expiry and logout must close observation promptly and prevent image
retrieval. Reconnects must obtain current state and must not resurrect revoked
access. Every completed capture must still belong to the same live worker generation
before publication; discard late results from ended sessions.

The current model revision omits some visual changes, such as camera-only changes.
An observer needs a visual revision or fingerprint covering the state it displays,
not merely the saved-file revision or current document revision. Failed batches may
also leave observable state, so capture decisions should inspect authoritative state
after completion rather than relying exclusively on a successful tool result.

## Timing and fidelity limits

This first stage shows real modelling progress after completed operations. During a
long command, show the previous image with a calculation-in-progress indication and
its revision/time. The synchronous worker does not expose intermediate geometry
while that command executes. A batch currently exposes its resulting state rather
than one frame for every contained command. Finer progress would need explicit,
bounded intermediate observation points inside execution.

Headless MCP calls engine commands directly. There is no desktop mouse cursor,
interactive dialog sequence or full application window to stream. A model image
can show bodies and sketches, but does not promise a reproduction of the desktop UI.
An observer must not interrupt, cancel or modify the modelling session. Its own
failure should degrade the preview, not end the worker or lose unsaved work.

## Later stage: independent interactive browser viewing

For local orbit, pan and zoom, send a bounded versioned snapshot of the already
evaluated scene to a read-only Rust/Wasm viewer. The current
[Scene](../crates/render/src/scene.rs) is an internal Rust structure, not an existing
network format. A transport must define mesh and line limits, numeric validation,
revision/generation identity, appearance and visibility, and atomic scene replacement.
Each viewer keeps an independent camera; navigation never changes the agent's model
or camera. Reuse the existing navigation and rendering primitives rather than adding
a second camera-control implementation.

An alternative is replicating an in-memory native document into a dedicated browser
copy. That would require a separate observer mode that prevents commands, saving and
autosave from turning observation into editing. It would also re-evaluate geometry
in the browser and need explicit treatment of linked files and unsaved session state
that is not stored in a design document. Repeatedly opening ordinary browser copies
is therefore not a substitute for live synchronization. A scene snapshot avoids
duplicating kernel evaluation and is the preferred interactive-viewing approach.

Replaying the agent's command history is not recommended: undo/redo, failed or partial
batches, loaded files and process replacement make it harder to guarantee that a
viewer represents the authoritative current model.

## Validation required before implementation is shipped

Use a small public fixture to verify unsaved sketch/body progress, unchanged agent
camera and undo state, long-command and batch status, and an ended session. Exercise
cookie/Bearer separation, logout/expiry, cross-origin rejection, late capture results,
multiple viewers, bounded memory/CPU, reconnects and backpressure. Prove that observing
creates no shared files, changes no real design, and does not starve MCP operations.
