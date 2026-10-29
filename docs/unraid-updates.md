# Validated container updates

## 2026-10-29: fixed upstream 0.26.0 snapshot and native format 43

Selected `c7a1cee357359145fb6ad6a8a0a5b2764e34c42e` once, 290 commits after
validated `ae07e36a`. The sole `codex/unraid-mcp-web` branch started clean and
synchronized at `1bdb06d4`; fetched fork main is already an ancestor. For upstream
CAD features/changes see [the original project](https://github.com/tangency-labs/solidcraft).
This is unofficial integration, not an application release or an Unraid installation.

All published integration behavior remains: independent browser CAD and HTTP MCP,
quiet/headless workers, immutable project binding, strong file revisions, shared
file/transfer gates, cookie-only remembered browser login, live Workspace inventory,
PNG thumbnails and independent browser copies, read-only observation, confirmed
deletion, bounded streaming ZIP64, manual projects, multi-file uploads with batch
choices and the shared-volume storage footer. Existing layouts, original assets,
private settings and saved designs are preserved. Canonical Unraid XML/icon bytes
are unchanged, so no new catalog commit is expected.

The merge preserves VFS/link confinement and isolated observer caches. The new
appearance clipboard is copied only by ordinary scratch sessions, never by an
observer. New point-cloud source reads use the shared HTTP/MCP gate. PLY/XYZ/PTS
opening is a new unsaved design, protecting its original source from implicit
native saving; scan-only designs now count as nonempty session drafts. The portable
CPU/headless renderer lacks a point-cloud pass: observation refuses a misleading
empty/partial image before snapshot work; hiding the reference permits normal CAD
observation. Import/edit/save work continues. The browser uses its separate WebGL
point pass. Browser upload types and resource limits are deliberately unchanged;
native `.sc` containers with embedded scans pass the existing bounded data policy.

Save-tool batch normalization now retains the explicit `overwrite` argument, matching
the direct tool and existing documented contract. It still requires each current
strong file revision and never overrides stale revisions or active file/transfer
locks. A real SDK test changes the file externally, rejects a stale revision even
with overwrite, then accepts the current revision and preserves the intended model.
No new unconfined output command was enabled: reference import only reads, drawing
container helpers remain library APIs, render copy is retained-image metadata in
headless mode, and 3MF component parts stay inside the single archive output. Desktop/
cloud slicer and unsupported multi-file output restrictions remain in place.

The toolchain stays Rust 1.95/Trunk 0.21.14. Upstream's Apache-2.0 winit patch supplies
Wayland gestures and the existing permissive font parser enables variable fonts;
updated dependencies/notices pass the shipped-license policy. Three equivalent
Clippy simplifications cover the HLR face predicate, sketch-extension match guard
and tree-icon condition; browser reveal uses the new upstream row identity. Initial
lint diagnostics remain private; no behavior was altered to silence a test.

Scoped checks passed: 143 gateway cases, 58 affected library selections (2,181 test
executions), affected CLI integrations and 30 native web/helper cases. Full native
all-target Clippy, 13-crate Wasm compatibility, browser/client lint, formatting,
licenses, 867 attributed assets, 350 original icon masters, layers, lists, shared
containers and skills pass. The unfiltered repository test suite was not rerun.
Actual software-GL capture/depth and stand-up UI cases, browser API/linked-source/
DXF/queue checks and all 20 offline/update cases pass.

A new labelled candidate, fresh disposable volume, loopback port and test token
passed HTTP MCP modelling at 24000 mm3, PNG/STEP, session separation, auth/origin/
path denial, logging/privacy, restart persistence, project binding, Workspace file
revisions, live observation/SSE, confirmed deletion/streaming ZIP, remembered login/
revocation and upload tests. Real browser proofs and inspected screenshots cover
multi-file drop, immutable targets, overwrite/skip/cancel/apply-all reset, stale
revision denial, live folder choices, compact actions/combined overview, footer and
independent native-design handoff. No existing runtime was changed during these tests.

The official MCP SDK reopens original public native formats 13 and 41 without changing
source bytes. Native format-43 saves preserve parameters, 24000 mm3, split-face names/
centroids, STEP, embedded HDR CPU rendering, source/output confinement and revisions.
A three-point public XYZ scan opens as an unsaved draft, survives native roundtrip,
refuses escape paths and implicit source saving, uploads as native `.sc`, and opens
independently in the real WebGL browser. Local color/point-size edits leave the server
bytes unchanged. All native ZIP CRCs pass; the cone keeps 314.1019886 mm3 within the
upstream 0.2% numerical tolerance. Evidence remains under ignored
`target/upstream-c7a1cee3-20261029/`.

Publication verified: source [`4c7db958`](https://github.com/denara/solidcraft-unraid/commit/4c7db9580faf5ee16e20ff09efe6978e8e5db151) passed [the exact container workflow](https://github.com/denara/solidcraft-unraid/actions/runs/40846614284).
That clean CI image passed modelling/PNG/STEP, session separation, authentication,
origin/path checks, restart persistence, logging/privacy, projects, Workspace,
observer/live events, deletion/streaming ZIP, remembered login and uploads before
registry push. Both `latest` and immutable `sha-4c7db9580faf5ee16e20ff09efe6978e8e5db151` pulled anonymously;
source, Linux amd64 and direct anonymous registry manifest/digest checks passed.
Public digest: `sha256:fbc8e1d00e1680a53f9bf81362342401408b1daeb0791a2735090a065c716a86`.

Export: `target/solidcraft-unraid-upstream-c7a1cee3-4c7db958.tar` (183,386,624 bytes), own tag `solvecraft-unraid:upstream-c7a1cee3-4c7db958`.
SHA256: `329db2b310017842591a01dff1477b4ff58832ee55c5512e48561ec78db37d90`. Tar structure and the full checksum were verified again
before this record.

The sole remaining local development runtime `solvecraft-solvecraft-1` is healthy at
0.26.0 on that immutable public image. A private pre-swap data/config backup passed
gzip/tar integrity and the actual prior image is retained for rollback. The same data
volume, token, full environment, user, ports/origins and resource/security limits were
verified unchanged. Post-swap authenticated official-SDK MCP, unchanged public
24000 mm3 source reopened read-only, ephemeral modelling/PNG, browser cookie login/
logout, protected saved properties/SSE and both independent web asset sets passed.
No real saved design was overwritten. Then 9 owned unused build/check/candidate
containers were removed after label/image/mount/activity checks, with diagnoses
preserved. Volumes, build caches, toolchain, private backup and rollback resources
remain. No global prune or unrelated resource deletion was used.

After publication the canonical XML and original icon still match public catalog
`denara/unraid-templates` main `ffa25fd39fbca2c4d5da2c868d5d0eed7b371925`. Profile and license
were untouched; no catalog commit or CA submission is needed. The final fetch found
`7341b8f6be35a69c42136e58d71fe88655340d67` (12 later upstream commits), reserved for
the next fixed snapshot; the tested baseline stays `c7a1cee3`. Fork main is an ancestor.

On Unraid, save active work, retain pre-update server and independent browser backups,
update the existing container while preserving data/token/settings, then reload the
browser. Older builds cannot read new native format-43 saves. This task did not
install on Unraid, alter nginx/DNS, create application releases/Git tags or migrate
existing files.

## 2026-10-28: fixed upstream 0.25.1 snapshot, native .sc compatibility

Selected `ae07e36a7d09c6095f4de1bc7ab51067e2418093` once, 131 commits after
validated `514b0c84`. The sole `codex/unraid-mcp-web` branch started clean and
synchronized at `113c4057`; fetched fork main remains an ancestor. Upstream CAD
features/changes belong to [the original project](https://github.com/tangency-labs/solidcraft).
This remains unofficial integration, without an application release or Unraid install.

The update preserves every published HTTP MCP, independent browser CAD and Workspace
extension: mandatory immutable real projects, strong revisions/shared gates, remembered
cookie-only login, server-push inventory, PNG thumbnails, independent browser copies,
read-only observation, confirmed deletion, bounded streaming ZIP64, manual projects,
multi-file uploads, batch conflict choices and the shared-volume capacity footer.
The permanent Unraid Overview is unchanged. The upstream app-ID icon rename preserves
its original bytes; catalog synchronization is only needed if actual metadata differs.

Upstream now uses `.sc` for new native designs. The integration recognizes `.sc`,
`.solidcraft` and `.solvecraft` equally for inventory, grouping, details, bounded data-only
uploads, strong revisions and cookie-bound one-use browser handoff. New browser copies
use `.sc`; existing files retain their original names, without a bulk migration. The
persistent HTTP upload-to-MCP continuation regression uses `.sc` while preserving a
legacy-extension source. Renamed public upstream fixtures are referenced at their new
paths. New saves use format 41, including pending cylinder-reference edges; older
builds cannot read them, so retain pre-update server and independent browser backups.

Merge resolutions preserve quiet/headless MCP, the save-overwrite revision contract,
unsafe native-bridge omissions and isolated observer caches. The new placement index
cache starts independently in scratch snapshots. Upstream Update Links now memoizes
reads; every actual read still passes the network link root/device/scope/symlink guard
before cache lookup or file access, including links embedded in uploaded designs.
Review found no new file-output command requiring an added gateway exception. Async
UI-save adoption does not change the CLI's synchronous, gated writes. One upstream
boolean was simplified equivalently for the pinned Clippy version; no modelling
behavior was changed. Shipped notices were regenerated with both fork packages.

Local verification passed 143 gateway tests, 48 relevant library selections (2,154 test
executions), affected CLI cases and 29 native browser/helper cases. Native all-target
Clippy, 13-crate Wasm compatibility and browser/client lint, fmt, license, 858 attributed
assets, original icons, layers, sorted catalogs, containers and skills checks passed.
`cargo xtask ci` completed its static/native gates; its unfiltered whole-workspace test
build was deliberately stopped before execution to honor the requested scoped testing.
The separate affected suites and native/Wasm gates passed; no failing behavior test was
waived. Software-GL capture and all 11 depth cases, 12 stand-up UI cases, actual browser
API/linked-source/DXF/queue checks and all 20 offline/update cases passed.

A fresh labelled candidate with a disposable volume, loopback port and test credential
passed modelling at 24000 mm3, PNG/STEP, session separation, path/origin/authentication,
logging/privacy, restart persistence, projects, Workspace revisions, live observation,
SSE, deletion/streaming ZIP, remembered login/revocation and browser uploads. Actual
browser tests and inspected screenshots confirm multiple drops, fixed targets,
overwrite/skip/cancel and apply-all reset, stale-revision denial, live folder state,
combined overview, footer and opening `.sc` as an independent browser copy.

The official MCP SDK reopened public native formats 13 and 40 without changing source
bytes. Format-41 `.sc` saves preserve parameters, 24000 mm3, split-face names/centroids
and STEP export; native ZIP CRCs pass. Existing aliases, extract/output confinement,
strong revisions, embedded HDR CPU rendering and session isolation also pass. The
closed cone retains 314.1019886 mm3 on roundtrip, within upstream's 0.2% analytic tolerance.
Evidence and screenshots remain privately under `target/upstream-ae07e36a-20261028/`.

Publication verified: source [`f7a02d45`](https://github.com/denara/solidcraft-unraid/commit/f7a02d45d67a96e81ccee0fea437afbe590fc49f) passed [the exact container workflow](https://github.com/denara/solidcraft-unraid/actions/runs/40662555859).
That clean CI build passed all container MCP, modelling, authentication, path/origin,
persistence, Workspace, observer, streaming ZIP, login and upload checks before push.
Both `latest` and immutable `sha-f7a02d45d67a96e81ccee0fea437afbe590fc49f` pulled anonymously, with source,
Linux amd64 and direct anonymous registry-manifest verification. Public digest:
`sha256:9d50dcf0c03c6b257b5aaa9d13dce25b5a2c08297972aa6d70dbaa48f0d4c39b`.

Export: `target/solidcraft-unraid-upstream-ae07e36a-f7a02d45.tar` (178,808,832 bytes), own tag `solvecraft-unraid:upstream-ae07e36a-f7a02d45`.
SHA256: `3c2a9fa855473edfdc8a76c6ac0473f63865dc66c9100a0b5a8ee23968a82188`. The tar structure and full checksum were checked again
before this record.

The sole remaining local development runtime `solvecraft-solvecraft-1` is healthy at
0.25.1 on that immutable public image. Private data/config backups passed gzip/tar
integrity and the actual previous image remains available for rollback. The data
volume, MCP token, user, full environment, ports/origins and security/resource limits
are unchanged. Authenticated official-SDK MCP, an unchanged public 24000 mm3 fixture
opened read-only, ephemeral modelling/PNG, browser cookie login/logout, protected
saved properties/SSE and both web asset sets passed after replacement. No real saved
file was overwritten. Only then were 9 owned unused build/check/candidate containers
removed after label/image/mount/activity checks. Diagnostics, proof/data volumes,
private backups, toolchain/cache volumes and the rollback image remain; no global
prune or unrelated resource removal was used.

After image publication, the canonical XML and original icon still match public
`denara/unraid-templates` main `ffa25fd39fbca2c4d5da2c868d5d0eed7b371925`. Profile and license
were untouched; no new catalog commit or CA submission was needed. The final fetch
still found upstream at the selected `ae07e36a`; fork main is an ancestor, with no
additional source changes to integrate.

To install on Unraid, save active work and keep pre-update server/browser backups,
update the existing container while preserving its data/token/settings, then reload
the browser. New native format-41 saves cannot be opened by older builds. This task
did not install anything on Unraid, change nginx/DNS, create an application release
or Git tag, or move existing designs.

## 2026-10-27: fixed upstream 0.24.0 snapshot, format 40 and preserved integration

Selected `514b0c84f9eb7655e3cdc986f0d44d4cd3416f4c` once, 185 commits after
validated `f006eefd`. The sole branch `codex/unraid-mcp-web` started clean and
synchronized at `dbe75ddb`; fetched fork main remains an ancestor. For SolidCraft
features/changes refer to [the original project](https://github.com/tangency-labs/solidcraft).
This is unofficial container integration, not an application release or Unraid install.

All published integration features remain: independent browser CAD and HTTP MCP,
headless/quiet workers, immutable real-project binding, revisions and file gates,
cookie-only Workspace/remembered logins, live inventory and read-only model observation,
thumbnails/browser copies, deletion, bounded ZIP64 streaming, manual projects,
multi-file drag/drop uploads with batch conflict choices and the storage footer.
The canonical Unraid Overview and original icon are unchanged; no catalog commit is needed.

Merge adjustments preserve the fork's unsafe native-bridge omissions (including the
new upstream macOS signal handler), VFS/link confinement and isolated observer caches.
New lazy measurement/surface caches and the EasyPrint host are not shared with an
observer; ordinary scratch computations retain their upstream hosts/renames. Desktop
or cloud slicer delivery is refused by gateway preflight before derived writes or
program launch; project mesh export still works. Link paths pass the upstream
remote/device/scope policy before root canonicalisation, then the existing no-symlink
confinement. Smoke tests require either explicit confinement or scope-denial diagnostics
and no credential disclosure. Upstream's new short AGENTS router is retained, with
all prior mandatory fork rules preserved in `docs/unraid-agent-rules.md`. Skill/front
matter and source-inspection checks handle a Windows CRLF checkout. The generated
shipped licenses were regenerated, retaining gateway coverage.

Local proofs use a new labelled candidate, fresh disposable volume, loopback port,
disposable credential and original/public analytic examples. The complete container
smoke/restart, project, Workspace, logging/privacy, observer, live, deletion/streaming
ZIP, remembered-login and upload tests pass. Real browser tests cover multi-file drop,
fixed targets, overwrite/skip/cancel and apply-all reset, stale revisions, live folder
state, joined layout, footer and independent native-design handoff. Actual screenshots
were inspected. Software-GL capture/stand-up integration and browser API/offline/update
checks pass, including the serialized asynchronous DXF download.

Native formats 13 and 38 open without changing source bytes; format-40 saves preserve
parameters, 24000 mm3 volume, named split faces and STEP export. A real MCP SDK client
also checks aliases, source/extract/output confinement, strong revisions, embedded HDR
native roundtrip, session separation and CPU rendering. A new closed cone roundtrip
measures 314.1019886 mm3 against analytic 100*pi (relative error 0.0182%, within the
upstream 0.2% numerical tolerance); saved/reopened measurement is unchanged. Native
container CRCs pass. Snapshot format 40 includes upstream pending cone and feature-id
references beyond the folded 0.24.0 release format; older builds cannot read new saves.

Relevant doc/IO/engine/MCP/UI/render/GPU/sketch and CLI regressions, 142 gateway tests,
native all-target Clippy, 13-crate Wasm compatibility and browser/client lint checks,
fmt, licenses/assets/icons/layers/lists/containers/skills gates are used. There is no
blanket rerun of the whole repository suite. The unchanged whole-registry hostile scan
was stopped as broader than needed; command-specific bounds and gateway protections
remain covered. Early source-inspection false positives caused by CRLF and the changed
permission diagnostic were corrected; no failing behavior test was waived.

Publication verified: [89497cd6](https://github.com/denara/solidcraft-unraid/commit/89497cd6ce04a17e295043102a956eeee695e68c)
passed [the exact container workflow](https://github.com/denara/solidcraft-unraid/actions/runs/40480408495). The immutable tag
`ghcr.io/denara/solidcraft-unraid:sha-89497cd6ce04a17e295043102a956eeee695e68c` and `latest` were pulled anonymously, with source/architecture
and direct anonymous registry-manifest verification. Public digest:
`sha256:99886289475d22cfe27235a07a466294155e1202b8aaebeb31dee602e449401d`.

Export: `target/solidcraft-unraid-upstream-514b0c84-89497cd6.tar` (176830464 bytes), own image tag `solvecraft-unraid:upstream-514b0c84-89497cd6`;
SHA256 `90ea90d21d66ec5bd5b7c8262e64e08b49bb9dd9c95b266c88a3a2ba54909f07`. The archive was checked again before this record.

The idle local development runtime `solvecraft-solvecraft-1` now uses the verified public
image and is healthy at the existing loopback endpoint. Its data volume, MCP token,
user, ports, origins, full environment and security/resource limits are unchanged.
A private pre-swap backup passed gzip/tar integrity and the actual previous image is
retained for rollback. Post-swap checks passed authenticated SDK MCP, an unchanged
public 24000 mm3 fixture opened read-only, ephemeral modelling/PNG, browser cookie
login, protected metadata/SSE and both browser asset sets. No saved design was overwritten.
Only then were 10 owned unused build/check/candidate containers removed,
after label/image/mount/activity checks; diagnostics, both proof volumes, real data,
private backup, toolchain/cache volumes and rollback image remain. One local runtime remains.

The final fetch found `9cba66c14d38d17504d0c3efb9e04194435743b7` (5 later commits),
reserved for the next fixed snapshot. Fork main still adds no new changes. No catalog
metadata was changed. Unraid installation remains the owner's manual container-update step.


## 2026-10-26: upstream 0.23.0, manual projects and bounded browser uploads

Selected one fixed upstream snapshot, `f006eefd6e2987ec14c8ab596ad39fe11e53be41`,
127 commits after validated `ac53fc1e`. The sole integration branch
`codex/unraid-mcp-web` started synchronized and clean at `13a16c09`; fetched fork
main remains an ancestor. For SolidCraft features and upstream changes refer to
[the original project](https://github.com/tangency-labs/solidcraft). This is an
unofficial container integration update, not an upstream release or Unraid install.

The merge preserves the HTTP gateway, independent browser CAD, quiet headless
workers, project binding, cookie/Bearer separation, remembered logins, file revisions,
server-push inventory, PNG thumbnails, independent browser copies, read-only live
observation, deletion, bounded ZIP64 streaming and all published layouts. Keep the
fork's native unsafe-bridge omissions. Linked-file metadata uses browser VFS while
retaining network confinement. Observer snapshots keep isolated caches and never inherit
render-job hosts; the new component-color choice is copied without sharing mutable state.
New DXF/SVG opening is treated as an unsaved drawing, never an implicit native save
over the source. The upstream disk-change override is carried through direct and
batched `save` aliases while shared strong revision preconditions remain authoritative.
Two equivalent upstream UI expressions were adjusted for the pinned Clippy version.

Own additional functionality:

- Explicit manual creation of real projects, including visible empty folders discoverable
  through MCP, using the existing project naming and identity rules.
- Multi-file picker and drag/drop into an explicitly selected real project/subfolder;
  fixed batch targets, progress/results and cancellation. Virtual groups are not targets.
- Overwrite, skip and cancel decisions for existing names, including identical bytes.
  **Für alle Dateien übernehmen** is off by default and applies only to remaining
  conflicts in that batch/target. Every file retains its own captured strong revision;
  changed or active/leased files are never covered by blanket consent.
- Validated native/design JSON, STEP/IGES, STL/3MF, self-contained OBJ/glTF/GLB,
  textual DXF/passive SVG and PNG/JPEG templates. External glTF/OBJ companions,
  programs/scripts/HTML and generic archives are rejected. Upload does not execute CAD.
- Bounded private streaming/staging, atomic publication, owner-bound expiring tickets,
  shared HTTP/MCP/ZIP/delete gates and cleanup on abort or failure.
- Small authenticated footer capacity indicator for the actual shared-data filesystem,
  safe Rust statistics, MiB/GiB/TiB, 15-second cache and explicit unavailable state.

Limits and format decisions are documented in `docs/unraid.md`. Actual enabled
importers were exercised with original analytic/public-safe data: fourteen native,
exchange, drawing and image examples uploaded and then opened/imported through a
real MCP SDK client. PNG and JPEG became reference canvases. Embedded glTF/GLB
worked without companion fetches. The real browser created a project, dropped multiple
files, handled a single prompt for multiple overwrites, skip-all, fresh-batch reset,
explicit cancel, a concurrent revision change, invalid content and a forced selection
change. Live folder state and the joined search/status surface survived. Its uploaded
24000 mm3 design was discovered and analysed by MCP, continued under a new filename
(24024 mm3), and the source bytes remained unchanged. Actual screenshots were viewed;
this caught and corrected the idle cancel button's CSS visibility.

Validation includes targeted upstream kernel/sketch/doc/IO/engine/UI/render/MCP tests,
CLI setup/quiet/headless tests, gateway/upload/auth/path/lease/abort regressions,
Clippy for all native workspace targets and the Wasm client, 13-crate Wasm compatibility,
licenses/assets/icons/layers/lists/UI containers, software-GL capture and stand-up
integration tests, actual browser API and offline/update behavior. The browser API
harness now finishes its asynchronous UI DXF download before replacing that document
through unrelated API commands; both export and linked-design checks still run.
No blanket rerun of the whole repository test suite was performed.

The isolated labelled candidate used a fresh disposable volume. Existing smoke,
project, Workspace, live, observer, log/privacy, deletion/ZIP and remembered-login/
restart scripts passed. `workspace-upload-test.ps1` is included before public registry
publication. The final CSS-only candidate also passed persisted reopen, upload/MCP
continuation and real browser regression checks. A separate hard-limited 32 MiB tmpfs
proved the footer reads the data filesystem, updates after upload/deletion and reports
ENOSPC without altering existing bytes or exposing a partial upload; only its private
ownership marker remained after cleanup. Unavailable capacity is covered separately.
No host filesystem was filled and no real user file was deleted or overwritten.

Native format36 and legacy format13 sources reopened without changing their bytes;
measurements (24000 mm3), parameters, split-face names/centroids and STEP output were
preserved. New native and embedded-HDR round trips use format38 with verified ZIP CRCs.
Keep pre-update server and separate browser-design backups for rollback.

Local proof scripts, images and detailed evidence remain in ignored
`target/upstream-f006eefd-20261026/`. The canonical Unraid Overview lists only our
permanent additions and retains upstream authorship, the original icon and the
unofficial integration notice. Publication digest/archive, local runtime replacement
and catalog synchronization are recorded below.

### Publication and final verification

The exact source merge `92a1ea576e62f6501dcd9c6e012dc51f21423bfd` passed
[SolidCraft Unraid container run 40315015413](https://github.com/denara/solidcraft-unraid/actions/runs/40315015413).
The fresh public image passed modelling, session separation, file/origin/revision
security, logging, persisted reopen, Workspace/live/observer, deletion/ZIP, the new
manual-project/upload/MCP-continuation test and remembered-login restart checks
before registry push. Public Linux amd64 tags are `ghcr.io/denara/solidcraft-unraid:latest`
and `sha-92a1ea576e62f6501dcd9c6e012dc51f21423bfd`. Digest:
`sha256:32cd578658ea3b38dbfa2b3ee2278dd0cde64c8e9653bd3475e38ca7cad301ab`. Both tags pulled anonymously with an empty Docker configuration;
the anonymous registry manifest and source labels matched.

Archive: `target/solidcraft-unraid-upstream-f006eefd-92a1ea57.tar` (175120384 bytes), tagged
`solvecraft-unraid:upstream-f006eefd-92a1ea57`. SHA256:
`c6baa3cd04469ba9ea6d5469e92ef47a78a72b2c8e232c919e298702b2c8eca7`.

The existing local development/test runtime is healthy at 0.23.0 on that immutable
public source image. Fresh private data/configuration backups passed gzip/tar
integrity verification; the actual previous image is retained as
`solvecraft-unraid:rollback-before-upstream-f006eefd-20261026`. Data volume, MCP token,
complete environment, user, port/origin settings and security/resource limits remain
unchanged. Only the ignored image selector was advanced. Authenticated SDK discovery,
modelling/PNG, read-only reopen/measurement of the known public 24000 mm3 source,
unchanged source bytes, browser-cookie login/logout, properties, SSE and web/client
assets passed after replacement. No real design was overwritten. No rollback was needed.

After those checks, 20 owned idle build/check/browser/candidate containers were removed,
with label/image/mount/activity verification and diagnostics retained under the ignored
task directory. The auxiliary capacity filesystem was explicitly verified as its own
32 MiB tmpfs, not a named user-data volume. One healthy local test runtime remains.
No volumes, unrelated resources, reusable toolchain/build images, caches, private
backups or rollback image were removed; no global prune was used.

The canonical template was synchronized after image publication to catalog main
`ffa25fd39fbca2c4d5da2c868d5d0eed7b371925`. Only `templates/solidcraft.xml` changed, listing our permanent
project/upload/capacity additions. XML/privacy checks passed. The original icon
(blob `1a61ebc1aee9454338e8cab216f0af7a9fb269fc`), profile, license and provenance stayed unchanged.
No new portal submission or policy acceptance was performed. Repository metadata
publication is separate from the catalog's own refresh or moderation.

The final upstream fetch observed `8108646e2c0393d5eb27019f455c8231f3498579`, 17 commits after
the selected snapshot. Those changes remain for the next fixed-snapshot update;
today's validated baseline stays `f006eefd`. To install this image on Unraid, save
active work, retain pre-update server and separate browser-design backups, update
the existing container with its data/token/settings preserved, and reload the browser.
Older builds cannot read newly saved format38 designs. No Unraid installation,
nginx/DNS change, application release, release branch or Git release tag was made.
The one-off project/upload/footer request is complete; subsequent updates preserve it.

## 2026-10-25: upstream 0.22.0 with scoped renderer sources and preserved integrations

Selected exactly one upstream snapshot, `ac53fc1e6fab85a50bdc410178cc75133183393f`,
178 commits after validated `807e367b`. The sole branch `codex/unraid-mcp-web`
began clean and synchronized at `7fd57ad0`; fetched fork main is still an ancestor.
For upstream CAD features and changes see the
[original project](https://github.com/tangency-labs/solidcraft). This integration
does not create an application release or install anything on Unraid.

Merge resolutions preserve the gateway, independent browser copies, native CLI
quiet mode and all Workspace, project, login, deletion, ZIP and observer behavior.
The fork's unsafe native-platform bridge omissions remain. Linked-component paths
retain confinement while adopting upstream's new face/joint handling. Observer
snapshots retain cold contact/display caches and never inherit the new render-job
hosts; ordinary scratch sessions keep upstream render-job sharing. Immutable
settled-joint state can be shared without sharing mutable observer work.

Nested HDR/texture file sources in rendering arguments now resolve relative to
the selected project with bounded recursion, traversal/hidden/symlink rejection
and closed built-in resource identifiers. HDR inspection, settings capture,
preflight and job preparation use the shared HTTP/MCP read gate. New render PNG/
EXR destinations retain the existing strong overwrite-revision gate; existing
multi-file output restrictions remain. New and changed command output sets were
reviewed. Four narrow upstream compatibility corrections retain behavior: reuse
the existing finite-radiance predicate, simplify equivalent HDR/arc input checks,
and compile the Unix-only CLI-link helper only on Unix. Existing boundary
regressions pass: 16 deterministic raytrace, three HDR container and five arc
extension cases. No lint suppression or weakened input validation was added.

Relevant selected tests pass across format/resources, IO, kernel profiles and
faces, engine rendering/components/configuration/parameters/motion/sketches,
observer isolation, UI rendering/navigation and sketch solving. This includes
126 gateway, 76 MCP, 499 renderer and 136 GPU unit cases; existing ignored tests
remain ignored. Targeted gateway path/read-gate groups pass after their final
edits. CLI discovery/quiet mode, command help, headless GPU refusal and all 24
browser-app cases pass. Native workspace Clippy with all targets, 13-crate Wasm
compatibility, scoped Wasm Clippy, formatting, licenses, assets (789 files), icon,
layer, catalog-order and UI-container gates pass. Both browser clients and native
release binaries build. This is selected validation rather than a complete
geometry-suite rerun.

All ten required actual software-OpenGL captures and 12 stand-up-offer UI cases
pass. Real browser API modelling/undo/redo, DXF, bounded queues/errors and tab
handling pass; every real-site offline/update scenario passes. Actual Chromium
checks the live joined Workspace, initially collapsed groups, compact 28-pixel
folder actions, PNG thumbnails and an independent native copy in browser CAD
0.22.0. The source design remains byte-identical after the handoff.

A fresh-volume candidate passes MCP modelling/PNG/STEP, independent sessions,
authentication/origin/path/revision checks, logging, restart/reopen, Workspace
push/metadata/thumbnails/handoffs, remembered login/revocation, read-only
observation and deletion/ZIP checks. The 256-MiB ZIP starts in 8 ms with measured
RSS growth of 128 KiB; integrity, Unicode/empty/nested entries, backup exclusion,
abort release and concurrent write leases pass. SDK 1.32.1 opens legacy format 13
and previous native format 35, preserving parameters, 24000 mm3 volume, original
bytes and named split-face centroids. New format-36 saves pass ZIP CRC checks,
including a captured HDR resource that reopens and renders through its saved
setup. Real CPU HDR/PNG rendering, nested-source confinement, render-job session
isolation, output revisions, coalesced parameter batches, alias/extraction guards
and BOM/DXF outputs pass. CPU EXR refusal leaves no output file. Private evidence
and screenshots stay under ignored `target/upstream-ac53fc1e-20261025/`.

New saves require a newer reader: back up server files and separate browser
designs before updating, and keep the previous image/pre-update files for
rollback. The canonical Unraid XML and original app icon match the catalog;
its permanent product Overview is unchanged. Publication, archive and local
runtime results follow below. No Unraid installation or nginx/DNS change is
performed.

A pre-push fetch observed 20 later upstream commits through `bbc3aa43`; they are
deferred to the next fixed-snapshot update. The validated target remains `ac53fc1e`.

Publication passed [container workflow 40181250565](https://github.com/denara/solidcraft-unraid/actions/runs/40181250565)
from validated merge/source `c15fa4a460ff1830f7ae5bb3364e59c2ced939bf`. Its fresh image passed MCP modelling,
PNG/STEP export, authentication/origin/path checks, independent sessions, restart
persistence, Workspace revisions/push/handoffs, project binding, read-only
observation, deletion/ZIP streaming, remembered-login restarts and logging/privacy
before publication. Public Linux amd64 tags are
`ghcr.io/denara/solidcraft-unraid:latest` and immutable
`sha-c15fa4a460ff1830f7ae5bb3364e59c2ced939bf`. Anonymous pulls with an empty client configuration and an
anonymous registry manifest confirm source labels, architecture, equal image
identities and digest `sha256:faa676bb8a46453b9f323a82e70e0dbeb0047358e041a949142db0d58d94be56`.

Archive: `target/solidcraft-unraid-upstream-ac53fc1e-c15fa4a4.tar`, 172590592 bytes,
tagged `solvecraft-unraid:upstream-ac53fc1e-c15fa4a4`.
SHA256: `ee9270131a5abe44d9be6f999c7101b3ed634c1931126d27eb3789ea276bba19`.

A private data/configuration backup passed archive integrity checks before the
local runtime was refreshed to the immutable public image. Health, authenticated
SDK modelling/PNG, read-only reopening of the public 24000 mm3 fixture with
unchanged bytes, browser-cookie login/logout, metadata, push and web assets pass.
The data volume, token, origins, port, identity and resource/security settings
remain unchanged. The sole local test container `solvecraft-solvecraft-1` is
healthy at 0.22.0, `http://localhost:8781/`. The previous 0.21.0 image is retained
as `solvecraft-unraid:rollback-before-upstream-ac53fc1e-20261025`.
8 inactive build/check containers and the unused candidate were removed after
label/image/workspace-mount/activity checks and diagnostic preservation. All
volumes, reusable build images, caches, private backups and the rollback image
remain; no global prune was used.

Post-publication comparison confirms catalog main `98f194ce87ad4dcab8892a6a51d8bd5b40744050`
already has byte-identical canonical XML and the original app icon. No catalog
commit or new CA submission/agreement is needed. The final upstream fetch still
points to `bbc3aa43`, 20 commits beyond this fixed snapshot; these remain
for the next validated update.

To install on Unraid, save active work, back up server files and independent
browser designs, update the existing container while retaining its data/token/
settings and reload the browser. Keep pre-update files for rollback: the previous
build cannot open newly saved format-36 designs. This task did not install on
Unraid or change nginx/DNS.

## 2026-10-24: upstream 0.21.0 with preserved workspace and scoped MCP aliases

Selected exactly one upstream snapshot, `807e367bdb03210dba188118fe000acd0a4b3725`,
150 commits after validated `edf57cec`. The sole branch `codex/unraid-mcp-web`
began clean and synchronized at `44378d02`; fetched fork main remains an ancestor.
Upstream CAD features and changes belong to the
[original project](https://github.com/tangency-labs/solidcraft). This container
integration creates no application release, release tag or Unraid installation.

Merge resolutions retain private-file ignores, the gateway, independent browser
handoffs, observer isolation and upstream native-menu/browser-navigation behavior.
Existing omissions of unsafe native platform bridges remain. Observer snapshots
keep cold display/contact caches; the new immutable motion-trace cache is shared
without sharing mutable editor locks. The gateway explicitly starts `--headless`
workers, preserving independent network sessions despite upstream desktop discovery.
New batch tool aliases are normalized before path/project/revision preflight.
`component.extract` destinations and `component.replace_with_link` sources now
participate in the same confinement and shared file gate; extraction cannot bypass
strong overwrite revisions. Regressions cover nested aliases, recovery denial,
traversal, project binding and unchanged conflicting file bytes.

The new square-theme search test intentionally discards an egui frame without
applying texture uploads. Explicitly clearing those deltas fixes its debug-only
harness panic while retaining every geometry assertion. The failing case and all
remaining selected UI groups pass; production drawing behavior is unchanged.
The SDK proof accepts the new skill-reference tool's valid plain Markdown reply
alongside JSON tool replies. No application JavaScript or runtime dependency was
added by this integration. Upstream cadmpeg 0.6.3 and new dependency metadata were
reviewed: new packages are permissively licensed; only fcstd/rhino/step codecs are
enabled. Shipped notices were regenerated with gateway coverage.

Relevant checks pass: 125 gateway, 75 MCP, four CLI discovery/quiet-mode, 39 format,
138 IO and 24 browser-app cases, plus selected kernel intersection/face-source,
hollow/revolve, engine frame/component/extraction/motion/joint/sketch/hole,
observer-cache and sketch-offset regressions. Changed UI groups cover preferences,
themes, parameters, documents, selection, timeline, navigation, frame/hole dialogs,
dimensions, viewport overlays and bounded device previews. Existing ignored cases
remain ignored. Native workspace Clippy, changed-test Clippy, scoped Wasm Clippy,
13-crate Wasm compatibility, formatting, licenses, assets (462 files), icons
(290 masters), layers, catalogs and UI-container gates pass. Both browser clients
and release CLI/gateway binaries build. This is targeted validation, not a rerun
of the complete geometry suite.

All 28 GPU unit cases and nine required actual software-OpenGL captures pass.
The real browser API proves modelling/undo/redo, bounded queues/errors and browser
Ctrl+Tab handling; all real-site offline/update scenarios pass. Actual Chromium
verifies the joined live Workspace, initially collapsed groups, compact folder
actions and PNG thumbnails, and opens the migrated native model in the independent
0.21.0 browser app. Server bytes remain unchanged after the browser handoff.

A fresh-volume candidate passes real MCP modelling/PNG/STEP, independent sessions,
authentication, project/path/origin/revision checks, logging, restart/reopen,
Workspace push/metadata/thumbnails/native handoff, remembered login/revocation,
read-only observation and deletion/ZIP checks. The 256-MiB ZIP starts in 8 ms with
measured RSS growth of 128 KiB; integrity, Unicode/empty/nested entries, backup
exclusion, abort cleanup and write leases pass. SDK 1.32.1 opens legacy format 13
and previous native format 31, preserving parameters, 24000 mm3 volume, original
bytes and named split-face centroids. New native format-35 saves pass ZIP CRC
checks. Actual SDK calls prove batch aliases and component extraction enforce
path/revision protection, and verify the new pending and bundled-reference tools.
BOM/DXF output guards remain covered. Private evidence stays under ignored
`target/upstream-807e367b-20261024/`.

New saves require a newer reader: back up server files and separate browser designs
before updating, and keep the previous image/pre-update files for rollback. The
original upstream app icon is mirrored exactly into the canonical Unraid asset;
the product Overview remains unchanged. Source/image publication, archive checksum,
local runtime refresh and catalog icon synchronization are recorded after their
final checks. No Unraid installation or nginx/DNS change is performed.

A pre-push fetch observed four later upstream commits through `8b65d41b`; they are
deferred to the next fixed-snapshot update. The validated target remains `807e367b`.

Publication passed [container workflow 40077973605](https://github.com/denara/solidcraft-unraid/actions/runs/40077973605)
from validated merge/source `e17621c17700073031573087203449b3a0de3134`. The fresh CI image passed real
MCP modelling/PNG/STEP, authentication/origin/path checks, independent sessions,
restart persistence, Workspace revisions/push/native handoffs, project binding,
read-only observation, deletion/ZIP streaming, remembered-login restarts and
operational logging/privacy before registry publication. Public Linux amd64 tags:
`ghcr.io/denara/solidcraft-unraid:latest` and immutable
`sha-e17621c17700073031573087203449b3a0de3134`. Anonymous pulls using an empty client configuration and
an anonymous registry manifest verify source labels, architecture and equal image
identities. Registry digest:
`sha256:8fefc9275b5d666360c7c1fd3407e65f9472ffd319bc6c8d7f5dd7ddd7896aee`.

Archive: `target/solidcraft-unraid-upstream-807e367b-e17621c1.tar`, 164232192 bytes,
tagged `solvecraft-unraid:upstream-807e367b-e17621c1`.
SHA256: `47ce6ad46858719e0807b8a17394f3ae895e368425f29d7c06fc03224b073032`.

A private data/configuration backup passed archive integrity checks before the
local development runtime was refreshed to the immutable public image. Health,
authenticated SDK modelling/PNG, read-only reopening of the known public 24000 mm3
fixture with unchanged bytes, browser-cookie login/logout, metadata, push and web
assets pass. Data volume, MCP token, origins, ports, identity and resource/security
limits remain unchanged. The sole remaining local test container
`solvecraft-solvecraft-1` is healthy at 0.21.0, `http://localhost:8781/`.
The previous 0.20.0 image is retained as
`solvecraft-unraid:rollback-before-upstream-807e367b-20261024`.
Four inactive build/check containers and the unused candidate were removed after
label/image/workspace-mount/activity checks and diagnostic preservation. All
volumes, reusable build/toolchain images, caches, private backups and the rollback
image remain. No global prune was used.

After image publication, catalog main `98f194ce87ad4dcab8892a6a51d8bd5b40744050` synchronized the
original canonical app icon and updated its provenance. The canonical XML is
unchanged; profile, license and other metadata are preserved. No new CA submission
or agreement was needed. The final upstream fetch still points to `8b65d41b`, four
commits after this fixed snapshot; these remain for the next validated update.

To install on Unraid, save active work, back up server files and independent
browser designs, update the existing container while retaining its data/token/
settings and reload the browser. Keep pre-update files for rollback: the previous
build cannot open newly saved format-35 designs. This task did not install on
Unraid or change nginx/DNS.

## 2026-10-23: upstream 0.20.0 with preserved observer and browser isolation

Selected exactly one upstream snapshot, `edf57cec293dcea301201f1d5173b33e16351466`,
24 commits after validated `3d3dd274`. Sole branch `codex/unraid-mcp-web` began
clean and synchronized at `8b0dcfe9`; fetched fork main is an ancestor. Upstream
features and changes remain documented by the
[original project](https://github.com/tangency-labs/solidcraft). This container
update creates no application release, release tag or Unraid installation.

Conflicts in agent instructions, native app imports and session scratch state
retain the integration rules and upstream behavior. The new unsafe Windows
HID-discovery bridge is omitted, preserving existing safe enumeration and Linux
HID behavior. New contact-mesh caches remain shared by ordinary scratch previews,
but observer snapshots get a separate cold cache. A regression holds the editing
cache lock and proves observation does not wait on it. Existing isolated display
caches and the bounded STEP placement copier are preserved.

Two new native-only deferred/apply enum variants and match arms now have platform
guards matching their constructors. Strict Wasm checks pass without suppressing
warnings or changing synchronous browser behavior. Two new headless UI tests
explicitly clear discarded texture uploads, retaining all text/click/layout
assertions. These harness corrections do not change shipped application behavior.
No new runtime dependency or toolchain change; the existing proc-macro2 addition
is test-only. Shipped licenses were regenerated and checked.

Targeted checks pass: 123 gateway, 60 MCP, 37 format, three simplification,
26 native-design IO, 70 STEP and 17 IGES cases, plus relevant immutable-ID,
construction-axis, motion/contact, offset-plane, sketch-edit and placed-mirror
regressions. Mesh-size, ahead-evaluation and cold observer-cache cases pass.
Changed UI groups include 92 UI-kit, 16 colour-vision, 12 viewport-overlay,
four frame-rate and nine import-orientation cases; all 23 browser-app tests pass.
Existing ignored cases remain ignored. The native desktop entry point checks
with all targets. Native/scoped Wasm Clippy, 13-crate Wasm compatibility, formatting,
licenses, assets, icons, layers, sorted catalogs and UI-container gates pass.
Release gateway/CLI binaries and both browser clients build.

All 21 GPU unit cases and eight actual required software-OpenGL captures pass,
including order-independent transparency and rejected-pipeline error handling.
The real browser API proves modelling, undo/redo, bounded queues and errors;
all real-site offline/update scenarios pass. Actual Chromium verifies the live
joined Workspace, collapsed initial groups, compact actions and an independent
migrated design in the 0.20.0 browser app. Server bytes remain unchanged afterwards.

A fresh-volume candidate passes MCP modelling/PNG/STEP, authentication,
project/path/origin/revision separation, logging, restart/reopen, Workspace push,
metadata, thumbnails, native handoff, remembered login, observation and file actions.
A 256-MiB ZIP starts in 9 ms with measured RSS growth of 256 KiB; backup exclusion,
integrity, abort cleanup and write leases pass. SDK 1.32.1 opens legacy format 13
and previous native format 30, preserving source bytes, parameters, 24000 mm3
volume and named split-face centroids. New format-31 saves pass archive CRC checks.
Construction-axis migration and BOM/DXF output confinement/revisions remain covered.
No new file-writing command/output set was introduced. Import-header tests use
analytic SolidCraft exports and synthetic metadata, never commercial CAD data.
Screenshots, public fixtures and diagnostics stay in ignored
`target/upstream-edf57cec-20261023/`. Canonical XML/icon blobs are unchanged.

Validated merge `8d9ebd25ca2bbd5d3578b5a3898d4dfc51b89cfe` remains in published
history. Final tested source `82f82a23ed59b91890cee8ca29fbf79896b0a3dc` includes the
follow-up restoring the original UTF-8 text in one test fixture; the targeted
existing case and formatting pass. The exact
[container workflow 39923092497](https://github.com/denara/solidcraft-unraid/actions/runs/39923092497)
passed all image checks before publishing `ghcr.io/denara/solidcraft-unraid:latest`
and immutable `sha-82f82a23ed59b91890cee8ca29fbf79896b0a3dc`. Anonymous pulls with an
empty client configuration verify Linux amd64, matching source labels and equal
image identities. Public registry digest:
`sha256:49c5b3fbeb4548a757616768339c6472f837ba0cec418b8fc234a0ab0758db50`.

The verified image is exported as
`target/solidcraft-unraid-upstream-edf57cec-82f82a23.tar`, tagged
`solvecraft-unraid:upstream-edf57cec-82f82a23`, 160353792 bytes.
Archive SHA256: `11d4c324b0ee3472f2e6a1ca3d15baa5122fcd3d3dc73aba04fc142025c67032`.
A private data/configuration backup passed archive integrity checks before the
local test runtime was refreshed. Health, authenticated SDK modelling/PNG,
read-only public-model reopen with unchanged original bytes, browser cookie login,
push, saved-model details and independent web assets pass. Data volume, MCP token,
origins, ports, identity and resource/security settings remain unchanged. Only the
healthy local development runtime remains at `http://localhost:8781/`. The owned
inactive candidate was removed after saving diagnostics, without deleting any
volume, build cache or backup; the previous image is retained for rollback.
Canonical XML/icon blobs match fetched catalog main
`72852d1109f091ff4e8979649ae62c277264d962`, so no catalog commit is needed.
Final upstream fetch still points to selected `edf57cec`. Unraid installation
remains the user's separate step; nginx and DNS were not changed.

## 2026-10-22: upstream 0.19.0 with all published integration features

Selected exactly one upstream snapshot, `3d3dd27439995f37eaed1feaa1c53a0e42d2fe39`,
45 commits after validated `e9490093`. The sole integration branch
`codex/unraid-mcp-web` began clean and synchronized at `2be5b67b`; fetched fork
main is an ancestor. Docker Desktop was unavailable at the scheduled start; the
same fixed-snapshot update resumed once the development runtime was available.
SolidCraft features and changes remain documented by the
[original project](https://github.com/tangency-labs/solidcraft). This integration
does not publish an application release or install anything on Unraid.

The merge retains the fork's omission of native Windows unsafe bridges, including
the new work-area caller, and preserves upstream's completion-cache cleanup.
An existing STEP regression exposed a generic placement copier rejecting a valid
periodic cylinder seam. Private body placement now uses the fork's existing
bounded topology-preserving copier, as explicit transforms already did; the
existing parallel preparation test reproduces the failure and passes after this
fix. A conditional context binding avoids an unused-variable warning on Wasm.
Only the two fork package versions were adjusted in the lockfile; dependencies
were not independently upgraded. License regeneration retains gateway coverage.

The real-site offline test driver previously waited for more held downloads than
Chromium's six-connection limit allowed. It now waits for one held request to
establish the failed-download overlap. Its copied real builds also receive one
unique fixture asset, so overlapping-version downloads genuinely overlap instead
of completing immediately because all copied bundle filenames are identical.
These changes affect temporary test fixtures only; shipped app and service-worker
behavior are unchanged. All real-site and stand-in offline scenarios pass.

Relevant checks: 123 gateway, 60 MCP, 36 document-format, 26 native-design IO,
ten file-command, 11 placed-mirror, 14 view-DXF, 70 STEP, 16 IGES, 56 tessellation,
eight completion, five window-fit, two CLI-help and 23 browser-app tests pass
(existing ignored cases remain ignored). GLSL and eight actual software-OpenGL
captures, document/kernel observer-cache regressions, native/scoped Wasm Clippy,
13-crate Wasm compatibility, formatting, licenses, assets, icons, layers, sorted
catalogs and UI-container checks pass. Both release browser clients and native
gateway/CLI binaries build.

A fresh-volume candidate passes modelling/PNG/STEP export, authentication,
revision/path/origin separation, immutable projects, restart persistence, logging,
Workspace push/metadata/thumbnails/native handoff, remembered login, read-only
observer and deletion/ZIP checks. The 256-MiB ZIP starts in 9 ms with measured
RSS growth of zero; integrity, Unicode/empty/nested entries, backup exclusion,
concurrent write leases and aborted-download cleanup pass. Real SDK 1.32.1 opens
legacy format 13 and previous native format 29, preserving source bytes, parameters,
24000 mm3 volume and all named split-face centroids on migration to format 30.
Current-view DXF and BOM outputs retain complete path/revision confinement.
Actual Chromium verifies the joined live overview, initially collapsed groups,
compact folder actions and an independent browser copy of the migrated design.
Screenshots, public fixtures and diagnostics stay in ignored
`target/upstream-3d3dd274-20261022/`.

Validated merge source `474107be42d20c7f68fcc368514ee4cf83233cb6` is published on
`codex/unraid-mcp-web`, preserving both parent histories. The exact
[container workflow 39749624219](https://github.com/denara/solidcraft-unraid/actions/runs/39749624219)
passed its image checks before publishing `ghcr.io/denara/solidcraft-unraid:latest`
and immutable `sha-474107be42d20c7f68fcc368514ee4cf83233cb6`. Anonymous pulls using
an empty client configuration, Linux amd64, matching source labels and equal image
identities were verified. Public registry digest:
`sha256:9f7f6a5894b7a5e499234046e0a46751bc663b9d73d5aa69fcc487bd1deed94e`.

The verified image is exported as
`target/solidcraft-unraid-upstream-3d3dd274-474107be.tar`, tagged
`solvecraft-unraid:upstream-3d3dd274-474107be`, 159698432 bytes.
Archive SHA256: `1dede9592eb5e2a910d2fee006ebf5357ad705e03d3b363a6d16ce82e6f0dac7`.
The private data/configuration backup passed archive integrity checks before the
local runtime was refreshed. Health, authenticated SDK modelling/PNG, read-only
public-design reopen with unchanged original bytes, browser cookie login, push,
saved-model details and independent web assets pass. Data volume, MCP token,
origins, ports, identity and resource/security settings remain unchanged. Only
the healthy local development runtime remains at `http://localhost:8781/`.
The owned inactive candidate was removed after saving diagnostics; no volumes,
build caches or backups were deleted, and the prior image remains tagged for
rollback. Canonical XML/icon blobs still match fetched catalog main
`72852d1109f091ff4e8979649ae62c277264d962`; no catalog commit is needed. Final
upstream fetch still points to the selected `3d3dd274`. The daily 09:00 task stays
active. Unraid installation remains the user's separate step; nginx and DNS were
not modified.

## 2026-10-21: joined Workspace overview and usable ZIPs after native saves

Preserves validated upstream `e94900933dafd63e2b32594afbdc879ecd64d2f7`
and all published 0.18.2 integration features; no additional upstream merge or
design-format change. Search and the four live counters now form one continuous
panel with subtle dividers, four/two/one columns on desktop/tablet/mobile. The
filter form remains outside the replaced live fragment, preserving focused,
unsubmitted search text and the user's open folders during server-push updates.

Native saves create a `.solidcraft.bak` sibling. Previously this normal backup
blocked the entire folder ZIP, despite being absent from the Workspace listing.
Archives now use that listing's auxiliary-name exclusions, independent of search
filters: backup, temporary and hidden entries are omitted without reading their
contents. The interface explains the policy, and the response reports the number
of omitted entries. Discovered exclusions still consume bounded manifest budgets.
Invalid visible paths, excessive nesting, links and special files remain rejected;
streaming, leases, cookie/Origin checks and final revision validation are retained.
Deletion deliberately uses the complete strict manifest, never this filtered set.

Relevant validation: 123 gateway tests, formatting and Clippy pass. A fresh-volume
candidate passes actual MCP modelling/export, Workspace revision/authentication,
live updates and the expanded action proof. Two actual native saves generate the
automatic backup; a 256-MiB streamed ZIP contains only the allowed files, Unicode
and empty folders. First bytes arrive in 25 ms and measured RSS growth is 640 KiB;
abort, conflicting writes, protected deletion and live inventory checks pass.
Actual HTTPS Chromium verifies four/two/one-column layouts, preserved focused
search and open folders across a real SSE file update, and a native ZIP click from
a filtered view: all allowed project members pass CRC, with no backup included.
Screenshots and diagnostics remain private in ignored
`target/workspace-overview-backups-20261021/`.

Validated source `4d2884b3c6c181e30ffce950ec268c44551cabc3` is published on
`codex/unraid-mcp-web`. The exact [container workflow 39635014607](https://github.com/denara/solidcraft-unraid/actions/runs/39635014607)
passed all image checks before publishing `ghcr.io/denara/solidcraft-unraid:latest`
and immutable `sha-4d2884b3c6c181e30ffce950ec268c44551cabc3`. Anonymous pulls with
an empty client configuration, matching Linux amd64 source labels and registry
digest were verified: `sha256:5a504c00244d6cecd786a00b738ce0852393202634d5ac0d13c6770eab768035`.

The verified image is exported as
`target/solidcraft-unraid-overview-backups-4d2884b3.tar`, tagged
`solvecraft-unraid:overview-backups-4d2884b3`, 159208448 bytes.
Archive SHA256: `cd80ac3a72f3c78960546f1af59b910e50209eb7936138098971583dc478377b`.
A private data/configuration backup and previous-image rollback were prepared
before refreshing the local test runtime. Health, authenticated SDK modelling/PNG,
read-only public-design reopen with unchanged original bytes, browser cookie login,
push and independent web assets pass. Volume, token, origins, identity and resource/
security settings are unchanged. The single local runtime remains available at
`http://localhost:8781/`; the owned, inactive candidate was removed after saving
diagnostics, without deleting any volume, cache or backup. Canonical XML/icon blobs
match fetched catalog main `72852d1109f091ff4e8979649ae62c277264d962`; no catalog
commit is needed. Fetched upstream main `728f753b` remains a separate future update.
Unraid data/settings were not modified; installation remains the user's step.

## 2026-10-21: Workspace ZIP downloads show errors instead of saving HTML

Preserves validated upstream `e94900933dafd63e2b32594afbdc879ecd64d2f7` and the
published 0.18.2 integration. No new upstream snapshot or CAD/file-format changes.
The folder link previously forced `download` even for HTML login, origin or manifest
errors, allowing the browser to offer a misleading `.htm` file. Successful archives
already supplied a ZIP content type and UTF-8 attachment filename. The link now lets
the server select attachments and opens failures visibly in a separate tab, retaining
the live Workspace and folder choices. Origin denials additionally log a fixed reason
code at `workspace_zip`; their checks and cookie-only authentication are unchanged.
No proxy, DNS or Unraid runtime settings were modified.

Relevant validation: 121 server regressions, formatting and gateway Clippy pass.
Tests assert attachment MIME/filename, native navigation without Origin and continued
rejection of cross-site, mismatched-host and invalid-Origin requests with safe audit
codes. A fresh-volume candidate passes MCP modelling/export, restart/reopen, Workspace
revisions, push, thumbnail/handoff, observer, remembered-login and ZIP/deletion checks.
The action script now verifies ZIP link markup, visible denied responses and real
attachment headers. An actual HTTPS Chromium click downloads and independently checks
`Demo-ZIP.zip`; a rejected disposable folder opens its error visibly, creates no HTML
download and retains the original Workspace/folder state. Public-safe screenshots and
all diagnostics remain in ignored `target/workspace-zip-download-20261021/`.

Validated source `80c56ca891abe989904a77acdf841e59022b2092` is published on `codex/unraid-mcp-web`.
The exact [container workflow 39626087876](https://github.com/denara/solidcraft-unraid/actions/runs/39626087876) passed before
publishing `ghcr.io/denara/solidcraft-unraid:latest` and immutable
`sha-80c56ca891abe989904a77acdf841e59022b2092`. Both pull anonymously with an empty credential
configuration; source labels, Linux amd64 and equal image identities were verified.
Published digest: `sha256:e4db6e70c549f3372b2a38ddddeded3ad9f9886f00ac855bdcfd6fe89957640a`.

The verified registry image is exported as `target/solidcraft-unraid-zip-browser-fix-80c56ca8.tar`, tagged
`solvecraft-unraid:zip-browser-fix-80c56ca8`, 159200768 bytes.
Archive SHA256: `c93effb56968aa04635b88adeab98476ad19bf980e1626d5a00b69d15e95a091`.
A private data/configuration backup and rollback image were retained before refreshing
the local test runtime. Health, authenticated SDK modelling/PNG, read-only public-design
reopen, unchanged original file bytes, browser cookie login, push and independent web
assets pass. Data volume, token, origins, identity and resource/security settings remain
unchanged. The single local runtime remains at `http://localhost:8781/`; the owned,
inactive ZIP candidate was removed after saving diagnostics. No volumes or caches were
deleted. The unchanged canonical catalog template matches fetched catalog main
`72852d1109f091ff4e8979649ae62c277264d962`, so no catalog commit is needed.
Unraid installation remains the user's separate step. Later upstream main `728f753b`
was observed and deferred to the next upstream update, preserving this fixed baseline.



## 2026-10-21: upstream 0.18.2 and compact Workspace folder actions

Selected exactly one upstream snapshot, `e94900933dafd63e2b32594afbdc879ecd64d2f7`,
157 commits after the validated `e9b1a077` baseline. The single integration branch
`codex/unraid-mcp-web` began clean and synchronized at `02b826ae`; fetched fork
main is an ancestor. All published merge history and integration features remain.
The original project now redirects to
[tangency-labs/solidcraft](https://github.com/tangency-labs/solidcraft); its CAD
features and changes remain documented there. This is container packaging, not
an application release or an Unraid installation.

Real-folder ZIP/delete actions now sit in a compact bar immediately below the
project summary and above the table headings. The redundant folder explanation
and repeated project name are removed. Actual subfolders retain explicit relative
names and accessible targets; virtual groups have no folder actions. The existing
view regression covers this order, targets and collapsed initial state. Confirmation,
revisions, active-project protection, streaming and live updates are preserved.

Merge conflicts retained fork publication/privacy rules, quiet CLI startup plus
new parameter checks, browser disk/font changes and native-copy handoff. Upstream's
R32Uint ambient occlusion replaces the older RGBA8 workaround; the retained GLSL
regression and eight actual required software-OpenGL captures pass. A new lazy
body-outline cache is isolated in observer snapshots; its cold-cache regression
passes. Equivalent Option/match corrections in the rewritten upstream dialogs
satisfy Rust1.95 Clippy without changing behavior. Only local fork package versions
were adjusted in the merged lockfile; unrelated dependencies were not upgraded.
Shipped licenses were regenerated with the gateway, Workspace and IBM Plex notices.
Unmodified upstream OFL text is preserved; the fork diff passes whitespace checks.

No new file-writing command was introduced in this range. New cache/compressed-source
saves retain their confined output. BOM, linked-source reads, immutable project binding,
revision checks and folder leases remain protected. The real SDK verifies the new
command-schema and running-work queries. Existing batch/observer behavior remains covered.

Relevant checks pass: 120 gateway, 60 MCP, two document and two kernel observer-cache,
34 engine schema, 23 browser-app and one CLI parameter test; 26 native-container IO,
35 document-format, ten file-command and 18 changed 3MF parser tests; GLSL translation
and eight actual GPU captures. Native/scoped Wasm Clippy, 13-crate Wasm compatibility,
formatting, 436 attributed assets, icons, layers, sorted catalogs and licenses pass.
Both Trunk release builds, the real browser API/queue proof and all 20 offline/update
scenarios pass. No blanket CAD suite was repeated.

A fresh-volume loopback candidate passes actual 24000 mm3 modelling, PNG/STEP, separate
sessions, authentication/origin/path/project guards, restart persistence, logging,
Workspace revisions, remembered login/logout across restarts, SSE, metadata/thumbnails,
one-use handoff and read-only observer isolation/revocation. Confirmed deletion and
ZIP streaming pass: first bytes in 16 ms for 256 MiB, with 128 KiB measured server
RSS growth. All six ZIP entries pass independent CRC checks. Conflicting writes are
rejected, unrelated MCP work continues and aborted transfers release their leases.
Only disposable candidate data was deleted.

The actual Workspace bar measures 28.39 px and contains only its two actions. Two
browser tabs receive a new public file by push and retain their open folder. The
delete popup still names its exact path and content count; it was cancelled. Embedded
browser capture timed out, so the original repository Chrome test harness captured
the actual candidate in Docker, including its independent 0.18.2 CAD copy. No HTML/CSS
mock was substituted. The one-use fragment was consumed and the server file remained
byte-for-byte unchanged. Screenshots and evidence stay in ignored
`target/upstream-e9490093-layout-20261021/`.

Public legacy format13 and previous-container format28 designs reopen with unchanged
parameters and 24000 mm3 volume, round-trip and export STEP. New saves use `solidcraft/29`;
previous builds cannot read these files. Save active work and back up server files and
independent browser designs before updating Unraid; retain a prior image and old files
for rollback. No automatic file migration or nginx/DNS change is performed.

Validated source `529177c0ed3e29e1fcd364e88c680f97608f5c19` is published on `codex/unraid-mcp-web`.
The exact [container workflow 39568562688](https://github.com/denara/solidcraft-unraid/actions/runs/39568562688) passed its fresh-image
modelling, PNG/STEP, authentication, isolation, project/revision, Workspace live/observer,
ZIP/deletion and login/restart gates before pushing the image. Public Linux amd64 tags:
`ghcr.io/denara/solidcraft-unraid:latest` and immutable
`sha-529177c0ed3e29e1fcd364e88c680f97608f5c19`. Both pulled anonymously using an empty Docker credential configuration;
source labels, architecture and equal image identities were checked. Independent anonymous
registry access confirms digest `sha256:9bf822f55002286295e74715310ce935832e89a38c2dd8b3abff2d2d9c4aac49`.

Archive: `target/solidcraft-unraid-workspace-layout-529177c0.tar` (159205888 bytes), with own tag
`solvecraft-unraid:workspace-layout-529177c0`.
SHA256: `db0e3d24b3fa51e8aea11351b5a52856f704a5e5d6d169db7213547b348d7830`.

The sole surviving local test container `solvecraft-solvecraft-1` is healthy on the
immutable public image. Its data/configuration were backed up privately, and the same
data volume, MCP token, complete environment, user, ports/origins and resource/security
limits were verified unchanged. Authenticated SDK calls, RAM-only 4000 mm3 modelling/PNG,
read-only reopening of the known public 24000 mm3 fixture with unchanged source bytes,
Workspace cookie login/logout, metadata/SSE and browser assets pass after replacement.
No real saved file was overwritten. The idle owned candidate was removed after verifying
its labels, image, volume and absence of active CAD/build work, with diagnostics retained.
No volume was deleted. Build containers removed themselves; toolchain images/caches,
private backups and `solvecraft-unraid:rollback-before-layout-20261021` remain.

Canonical XML and original icon remain identical to `denara/unraid-templates` main
`72852d1109f091ff4e8979649ae62c277264d962`; no catalog commit or CA resubmission is needed.
The Overview remains a permanent description of this unofficial integration, with
upstream changes linked and only this fork's additional features listed separately.
Final fetch observed later upstream `55587f4ac46ded0411e7e77a91e27e0e760ab5c7`; it is deferred to the next fixed-snapshot update.
To install, save active work and back up server files and independent browser designs,
update the existing Unraid container while retaining its data/token/settings, then reload
the browser. Keep old design copies for rollback because new format29 files cannot be
read by the previous build. No Unraid deployment, nginx/DNS change, application release,
release branch/tag or automatic file migration was performed.

## 2026-10-20: upstream integration and confirmed Workspace file management

Selected exactly one upstream snapshot, `e9b1a077d7e33420791791c651f2e293962a2da1`,
and merged it into `codex/unraid-mcp-web`, preserving all published integration
history and the HTTP gateway, browser CAD, project binding, remembered logins and
live model observer. `origin/main` was fetched and is already an ancestor. The
starting integration/source branch was `59856f90`; the previous upstream baseline
was `5122b98a`. SolidCraft remains version 0.16.2; this is container integration,
not an upstream application release.
Later upstream movement to `8a562f27` was observed after validation began; it is
reserved for a subsequent update and was not mixed into this fixed snapshot.

Workspace now offers explicit file/real-folder deletion with a five-minute,
cookie-bound confirmation, exact path, file/subfolder counts and total bytes.
CSRF, exact Origin/Host checks, strong content revisions and the shared HTTP/MCP
file gate reject stale or conflicting deletion. Active MCP projects/current files
and ongoing file operations are protected. Virtual root groups have no bulk action.
Real nested and empty folders appear in the bounded, live inventory. The read-only
model observer and independent browser CAD retain their existing behavior.

Real folders download as Rust-generated ZIP64 streams with safe Unicode relative
names, nested/empty directories and empty files. Stored entries avoid compression
work on already compressed CAD/images. A two-chunk bounded channel provides actual
backpressure; no full archive buffer or server-side ZIP spool exists. Path leases
reject matching HTTP/MCP writes while unrelated projects continue working. Limits
are two downloads, 2048 entries, 1 MiB of names and 16 GiB uncompressed data; body
authorization, stall/disconnect handling and final source checks reject incomplete
or changed archives. Reserved/hidden paths, symlinks and special files are rejected.
The explicit deletion exception and new proof command are recorded in AGENTS.md.

Integration review added the new `assembly.bom` CSV/JSON destination to revision
preflight, confined `component.relink` paths and serialized linked-source reads
without granting save revisions. New upstream placed-mesh/plan caches and clearance
caches remain isolated in observer snapshots. The omitted driver bridge accepts its
new callback signature while continuing to return Unsupported without callbacks or
threads. A Rust1.95 Clippy boolean simplification is equivalent for all 16 truth
combinations. Generated shipped licenses cover the merged dependency graph.

Relevant checks passed: 120 gateway tests; two document observer-cache tests; two
kernel observer-cache tests; five worker observer tests; four workspace telemetry
tests; nine BOM tests; the omitted-driver regression; native/Wasm Clippy, 13-crate
Wasm compatibility, formatting, assets, icon standards and 15 icon-tool tests,
layers, sorted catalogs and licenses. The entire CAD test suite was not repeated.
The isolated container passed modelling/PNG/STEP at 24000 mm3, restart persistence,
MCP/session/project/path/origin safety, logging, file revisions, Workspace push,
metadata/thumbnails/native handoff, observer isolation/revocation and remembered
login/logout across two restarts. The local login test now accepts an explicitly
identified candidate task/image while retaining its volume, loopback, isolation,
credential and identity safeguards.

The new isolated file-action proof validates deletion, stale confirmations,
CSRF/Origin protection, active sessions, parallel writes, independent MCP saves,
ZIP integrity, Unicode/empty/nested/large content and abort cleanup. First ZIP bytes
arrived in 7 ms during a 256 MiB transfer; measured server RSS grew by 128 KiB
(from 9715712 to 9846784 bytes). Python's independent ZIP decoder verified CRCs for
all six entries (268435469 uncompressed bytes). The real browser confirmed initial
collapsed folders, the exact folder confirmation/cancel flow, individual file
deletion and automatic removal in another tab while retaining its open folder.
The actual browser CAD app opened the saved public model as an independent copy;
the server design hash remained unchanged. Public-safe screenshots and proofs stay in ignored
`target/upstream-e9b1a077-workspace-20261020/`.

The real MCP SDK reopened public legacy format13 and previous-container format28,
saved/reopened the migrated design, retained parameters and 24000 mm3 volume,
exported STEP and verified BOM path/revision rejection. Original fixture bytes were
unchanged. Only disposable candidate data was deleted; the existing development
runtime was not used for destructive tests.

Source `e75ca79677221d1a99e893f3005d39be4e57fcba` is published on `codex/unraid-mcp-web`.
The exact [SolidCraft Unraid container workflow](https://github.com/denara/solidcraft-unraid/actions/runs/39403273835)
passed its build and all integration gates before publishing both immutable source tag
`sha-e75ca79677221d1a99e893f3005d39be4e57fcba` and `latest` to `ghcr.io/denara/solidcraft-unraid`.
Both tags were pulled successfully with an empty Docker credential configuration;
source labels, Linux/amd64 identity and equal image identities were verified.
Anonymous registry access independently confirmed digest
`sha256:e8eb620754192d001b4fe49638eaaed3e58fdcf13c2074af409222f779fc8af1`.

Public-image archive: `target/solidcraft-unraid-workspace-actions-e75ca796.tar` (155814400 bytes),
containing tag `solvecraft-unraid:workspace-actions-e75ca796`.
SHA256: `80a10ebe449d8497e9ee542b721b3545486d7665847984a7508ddc51a66d5e6e`.
A separate archive of the identical-filesystem locally validated image is retained in
ignored target as an additional development artifact.

The existing local development instance now runs the verified immutable registry
image. Its data and configuration were backed up privately first; the same data
volume, MCP token, identity, port, origins and security/resource limits were preserved.
Docker and HTTP health, authenticated SDK calls, a read-only reopen of the known
public 24000 mm3 fixture with unchanged bytes, RAM-only 4000 mm3 modelling/PNG,
browser login/logout, Workspace metadata/SSE and browser assets passed after replacement.
One completed owned candidate container was removed only after these checks and
ownership/activity verification. No volumes were deleted. Toolchain/build caches,
private backups and the previous image tagged
`solvecraft-unraid:rollback-before-actions-20261020` remain available.

The canonical Unraid Overview describes only the fork's additional features and
retains original authorship and unofficial status. It was synchronized after public
image verification to `denara/unraid-templates` main commit `72852d1109f091ff4e8979649ae62c277264d962`;
profile, license and original icon were preserved. No new CA submission or acceptance
claim was involved. Nothing was installed on Unraid and nginx/DNS were not changed.
Existing Unraid users can now check for and install the container update themselves.


## 2026-10-19: read-only live observation of unsaved MCP models

The first stage from `workspace-live-view-research.md` is implemented on the same
validated upstream snapshot `5122b98a` (0.16.2). Upstream `3b4db52c` was checked and
is reserved for a separate upstream update. Source `ac3689f8b74a4a7b2f3bd7ce378c63d03c655446`
on `codex/unraid-mcp-web` adds **Live-Vorschau ansehen** to active workspace session
cards. Cookie-protected SSE announces actual PNG snapshots of the worker's current
unsaved sketches and solids. Changes appear after completed commands or batches;
the previous picture is marked while a calculation is running. Browser CAD remains
independent, and observation provides no model or camera controls.

Snapshot preparation/rendering runs in a guarded background thread. The observer
preserves the document, camera, undo history, dirty state and saved path; cold
document and kernel instance display caches are isolated from editing. A visual
revision also tracks camera-only changes. Viewers share the latest in-memory image,
bounded to 800 x 500 pixels and 2 MiB, with a global render limit, a minimum two-second
capture interval, a 32 MiB cache budget and four leased image responses. No observation
image or design is written to shared files. Logout/expiry and session termination
revoke image access and end subscriptions. Independent opaque handles do not expose
usable MCP session credentials. Real MCP requests have priority over internal
snapshot exchanges. Oversized or failed previews degrade independently of CAD work.

Relevant validation includes gateway unit tests, five worker observer tests, four
workspace telemetry tests, two kernel instance-cache tests and a document mesh-cache
test; formatting, Clippy with warnings denied, Wasm compatibility, original asset,
dependency-layer and license checks. Additional regressions cover logout-event
delivery under a competing revocation watchdog and bounded backpressure. Isolated
container proofs passed modelling/PNG/STEP (24000 mm3), restart reopening, project
and path boundaries, file revisions, authentication/origins, logging, workspace SSE,
thumbnails/native browser copies and remembered-login persistence. The new observer
proof verifies a real unsaved 30 x 30 x 40 mm sketch/extrusion (36000 mm3), camera-only
updates, shared frames, responsive MCP, preserved files/history and logout/session-end
revocation. Its test harness explicitly clears a deliberate hostile Origin retained
by PowerShell's browser session before subsequent legitimate image requests.

Actual browser evidence shows an unsaved 40 x 30 mm sketch becoming a 20 mm-high body
without reloading, a second body appearing, retained folder choices, successful
close/reopen and an explicit ended state with the image removed. The independent
browser CAD app loads its original sample plate. Public-safe screenshots and proofs
remain under ignored `target/workspace-observer-20261019/`. The design format and
upstream CAD behavior are unchanged; the full CAD test suite was not repeated.

The local development runtime now uses `solvecraft-unraid:observer-local-ac3689f8`,
with the verified source label and exactly the same filesystem layers as the passing
candidate. This local assembly rebuilds the native binaries and workspace Wasm over
the previous passing image, reusing its unchanged CAD-web assets; the publication
workflow builds the complete image from source. Its previous data/configuration was
backed up privately; data volume,
MCP token, origins, identity, ports and security/resource limits were preserved.
Health, authenticated MCP SDK calls, read-only reopening of the known public 24000 mm3
fixture, unchanged fixture bytes, RAM-only modelling/PNG (4000 mm3), browser login,
workspace SSE and browser assets passed after replacement. The completed candidate
container and its temporary image were removed after ownership/activity checks;
volumes, build caches/toolchains, private backups and
`solvecraft-unraid:rollback-before-observer-20261019` remain.

Local-image archive: `target/solidcraft-unraid-observer-local-ac3689f8.tar`
(249916416 bytes), containing `solvecraft-unraid:observer-local-ac3689f8`.
SHA256: `c91d6ffe2505e3e1b6fca200418105d8c7c5767a3758c7a924a924e9a4f86d15`.
This is the locally built and validated image; public registry distribution is
verified separately below.

The exact-source [container workflow](https://github.com/denara/solidcraft-unraid/actions/runs/39286338598)
passed its complete image build and all modelling, persistence, browser-asset,
logging, project/path/revision, workspace, live-observer and remembered-login gates
before publication. GitHub's Actions incident delayed status reporting during the
run; the final successful result and source revision were verified directly.
The immutable tag
`ghcr.io/denara/solidcraft-unraid:sha-ac3689f8b74a4a7b2f3bd7ce378c63d03c655446`
and `latest` were pulled anonymously and verified against the source and Linux/amd64
labels. Public digest:
`sha256:bc5b925e6ec7de2e2ce9f6a8e59fe39709cc41a7dbfcead4aa5ca1150142382c`.

Public-image archive: `target/solidcraft-unraid-observer-ac3689f8.tar` (151697408
bytes), containing `solvecraft-unraid:observer-ac3689f8`.
SHA256: `141ec2caa19a120b9658647438fa3975b70a50f6ae47e06208ac43a130f63ee5`.
The canonical Overview's own-feature bullet was synchronized after verified image
publication to `denara/unraid-templates` commit
`2fad17ba3a09cb946c164a8ffe2692ad7c1b41c2`, preserving profile, license and original icon.
No Unraid installation, nginx/DNS change or application release was made. Install
through Unraid's usual container image-update action, retaining data and credentials,
then reload `/workspace`; save active unsaved drafts before restarting the container.

## 2026-10-19: explicit MCP project folders and workspace organization

This integration update keeps the validated upstream snapshot `5122b98a` (0.16.2)
and changes the HTTP gateway/dashboard only. The separate latest upstream head is
checked but is not merged into this feature change.

New HTTP sessions must call `project_select(name)` before CAD tools. The project
directory is real and immutable for that session; all CAD output paths stay inside
it. Shared HTTP downloads, file listings and revision keys retain their workspace
prefix. Existing files stay in place; explicitly opened workspace sources cannot be
implicitly overwritten outside the selected project. The desktop stdio interface
is unchanged. Clients should reconnect/discover the new gateway tools after updating.

Workspace groups start collapsed, preserve user choices during server-push updates,
and group nested files under their real top-level project folder. Legacy root image
variants follow the longest unambiguous native-model stem. Details distinguish our
derived group from the design's optional stored project property. Session cards show
the selected project. Live viewing of unsaved CAD sessions is researched in
`workspace-live-view-research.md`; no viewer is included.

Local validation: 94 gateway tests; formatting, Clippy with warnings denied, original
asset checks, dependency layers and shipped-license checks. Isolated fresh-volume
container proofs cover sketch/extrusion (24000 mm3), PNG/STEP, project selection,
immutable bindings, paths and nested writes, legacy source preservation, shared-file
revisions, authentication/origins, logging, restart persistence, SSE, remembered login,
protected thumbnails and one-use browser copies. The official MCP client SDK models
36000 mm3 in separate projects. Actual browser checks confirm collapsed initial and
new groups, retained open/closed choices after server pushes, correctly grouped root
PNG variants and a rendered independent CAD copy opened from a real project folder.
Proofs/screenshots remain under ignored `target/workspace-projects-20261019/`.
The original CAD engine/browser sources and native design format are unchanged;
their full test suite was not repeated.

Published source: `f629a6b913396141368638b67fe330c0b660dcff` on
`codex/unraid-mcp-web`. [Container workflow](https://github.com/denara/solidcraft-unraid/actions/runs/39269471101)
passed its fresh image build and modelling, file/project, authentication, logging,
workspace, restart, browser-asset and remembered-login checks before publication.
Both the immutable tag `ghcr.io/denara/solidcraft-unraid:sha-f629a6b913396141368638b67fe330c0b660dcff`
and `latest` were pulled anonymously and have the verified source/amd64 labels.
Digest: `sha256:6f8e3f0c41d47d52ff7aff940143e5537c5edcfd664f9700c2705cbdc3e7e0fb`.

Transfer archive: `target/solidcraft-unraid-projects-f629a6b9.tar` (151432704 bytes),
containing tag `solvecraft-unraid:projects-f629a6b9`.
SHA256: `d43bf7bac6a748fa67adf935cdf1e74c3a9d65025b89ad8c77a273a702bf17f4`.

The local test runtime was backed up privately and refreshed to the immutable public
image, preserving its data volume, token, origins and resource/security settings.
Health, authenticated SDK calls, the known public model's 24000 mm3 measurement,
unchanged saved fixture bytes, RAM-only modelling/PNG, browser login, SSE and web
assets passed after replacement. The completed candidate container and its temporary
image were removed after ownership/activity checks; volumes, build caches/toolchains,
private backups and a previous-image rollback tag remain. The canonical Overview was
synchronized to `denara/unraid-templates` commit `e51d81f`, retaining profile, license
and icon. No Unraid installation, DNS/nginx change or application release was made.
Install through Unraid's normal image-update action, retaining the existing data and
authentication settings, then reconnect MCP clients to discover project selection.

## 2026-10-11: upstream 4af2d06

Advanced the deployment fork from `af536a7907dd4961bb8e01a3208863c013392c58` to the
fixed upstream snapshot `4af2d060073b17d3a6dcea2018ac9490740f8f4e` on
`codex/unraid-mcp-web`, preserving the tested update merge `c8d1f63`. Upstream main still
pointed to this snapshot at the final pre-publication check. The inherited application version remains `0.2.1`;
this development update is not a new application release.

All subsequent manual and scheduled updates use the same `codex/unraid-mcp-web`
working branch. The temporary branch used to prepare this first update was consolidated
into that branch and removed after publication; the tested code and image are unchanged.

Changes include horizontal/vertical constraints on preselected geometry, direct
centerline-to-construction conversion, clearer constraint glyph placement, active
toolbar tool highlighting, a new original logo, camera roll and closed-pipe CLI
handling. Windows SpaceMouse support is inherited; Windows hardware was not tested
for this Linux/Unraid update. The container already used the newly required Rust 1.95.

Merge conflicts were limited to Cargo.lock and generated license notices. Both the
HTTP gateway dependencies and the new MIT-licensed hidapi dependency were preserved;
license notices were regenerated. No additional package upgrades or gateway changes
were necessary. Web and MCP remain independent.

Validation used change-scoped checks; the full test suite was not rerun:

- Formatting, Clippy with warnings denied, assets, licenses and dependency layers passed.
- All ten library/web targets passed the WebAssembly compatibility check.
- 67 selected tests passed: camera (8), sketch UX (27), point alignment (6), constraint
  glyph scenarios (2), toolbar/header (3), preferences (6), CLI output (2), HTTP gateway
  (4) and stdio MCP (9).
- The Linux amd64 release image built successfully, including the independent browser app.
- Isolated HTTP MCP smoke checks passed: 14 tools, sketch/extrusion, 24000 mm3 volume,
  PNG preview, STEP download, independent sessions, authentication, origin and path rejection.
- Restarting the candidate with its test volume preserved the saved design.
- A copied public-safe design from the previous prototype loaded with 24000 mm3 volume
  and exported valid STEP. The official MCP client SDK 1.32.1 also built a 4000 mm3
  design, received a PNG preview and deleted its session successfully.
- The browser sample and active-tool highlighting were inspected visually. The CLI's
  generated 256 px icon was also inspected. Evidence and logs remain under ignored
  `target/unraid-update-4af2d06/`.

Prepared image: `solvecraft-unraid:upstream-4af2d06` (Linux amd64).
Docker image ID: `sha256:838ec8ca1705172f498404fe56086025ff3db7f225b69ccc52b6b9debee67758`.
Local archive: `target/solvecraft-unraid-upstream-4af2d06.tar` (53,562,880 bytes).
Archive SHA256: `88848d815616b621e1cecbc66a9a34c608455757d85a4ddd9f4b07bc11c540e6`.
The exported manifest was checked for the expected image tag.

The local development container was subsequently switched to this validated image,
retaining its persistent data volume and MCP token. Obsolete project build/check/candidate
containers were removed; the previous `solvecraft-unraid:prototype` image remains available
for rollback, alongside reusable build images and caches. No Unraid installation,
credential rotation, registry publication or release tag was performed. Private deployment
configuration and the local pre-replacement data backup remain ignored.

To install the prepared update, transfer and load the archive on Unraid, then change
the container's image field to `solvecraft-unraid:upstream-4af2d06` while retaining
its existing persistent data mapping, MCP token, network settings and resource limits.
Back up designs and retain the previous image before replacing the container; see
the [update workflow](../.agents/skills/update-solvecraft-unraid/references/upstream-update.md).
Nginx Proxy Manager requires no settings changes for this update.

## 2026-10-12: public registry and unofficial Community Applications packaging

The first registry build is based on source commit
`450eff4ef2fdaa47da6de8b6e9a7549d125a3989`, with the same validated upstream CAD baseline
`4af2d06`. [Container workflow run](https://github.com/denara/solidcraft-unraid/actions/runs/38180172891)
passed build, modelling/PNG/STEP, access and path/origin checks, independent sessions,
restart persistence and web assets before publication.

Image: `ghcr.io/denara/solidcraft-unraid:latest`, Linux amd64. Source-specific tag:
`ghcr.io/denara/solidcraft-unraid:sha-450eff4ef2fdaa47da6de8b6e9a7549d125a3989`.
Published digest: `sha256:279dc00e65e0ca18c27de9dce34fc53e84164443c8aced25f66c31939b381483`.
An anonymous Docker pull and anonymous registry manifest request both succeeded;
the downloaded image reports the expected source revision and architecture.

The separate public [CA repository](https://github.com/denara/unraid-templates) contains
the MIT-licensed template/metadata, a repository profile and the original attributed icon.
The CA portal validated it as public, active and MIT-licensed, then found one valid Docker
app with no hard errors or warnings and confirmed that the image is pullable.
On 2026-10-12, the owner authorized submission and acceptance of the portal terms.
The repository was submitted as **denara's Repository** and the portal confirmed
**received and auto-approved**. It states that the Docker app should appear after the
next Community Applications build publishes; feed visibility is separate from approval.

The catalog app is **SolidCraft**, explicitly an unofficial Unraid integration.
The CAD application and browser app are credited to bherbruck and the upstream SolidCraft
contributors; denara maintains only the container packaging, HTTP MCP adapter and Unraid
template. No upstream application release, version bump or release tag was created.

## 2026-10-12: upstream 07e263d (SolidCraft 0.3.2)

Advanced the deployment fork from `4af2d060073b17d3a6dcea2018ac9490740f8f4e`
to the fixed upstream snapshot `07e263d7e95cac9a6851e48bffcb79f302aabfb8`, merged
directly into the sole working branch `codex/unraid-mcp-web`. Origin's integration
branch was synchronized before integration; origin/main was already an ancestor.
The inherited upstream application version is 0.3.2; this packaging update does
not create a new application release, release branch or release tag.

Changes include removal of artificial Shell wall splits on stepped bodies, faster
cold geometry rebuilds and retained assembly geometry across selection changes,
direct Parameters access, expression completion and Enter handling, free
Shift-middle orbit, optional immediate wheel zoom, and desktop renderer/icon fixes.
The new vendored egui-wgpu is MIT OR Apache-2.0 with original license texts and
attribution. Desktop Windows hardware and OpenGL launches were not tested for
this Linux/Unraid update. No design serialization schema change was introduced.

Merge conflicts were limited to Cargo.lock and generated license notices. Both
the HTTP gateway's dependencies and upstream additions were retained; notices
were regenerated. A headless-only upstream completion-test harness initially
failed on unapplied texture deltas; it now explicitly clears them, matching the
existing UI test harness. All six completion regressions then passed with debug
checks active. No production CAD or gateway behavior was modified for that fix.

Validation was scoped to these changes, without rerunning the full test suite:

- Formatting, workspace Clippy with warnings denied, assets, license notices and
  dependency layers passed; all ten library/web targets passed WebAssembly checks.
- 118 selected tests passed: profile geometry, mesh refinement, wall regularization,
  stepped Shell/file-reference compatibility, document/engine placement caching,
  completion, viewport/navigation, preferences, theme/context menus, parameter Enter,
  CLI behavior, HTTP gateway and stdio MCP. Initial launch/filter diagnostics and
  the corrected test runs remain under ignored `target/unraid-update-07e263d/`.
- Linux amd64 release image and independent browser app built successfully.
- Isolated HTTP MCP checks passed: 14 tools, sketch/extrusion, 24000 mm3 volume,
  PNG/STEP output, independent sessions, authentication and origin/path confinement.
  Restarting the fresh candidate test volume preserved its saved public design.
- A copied 0.2.1 public fixture reopened with 24000 mm3 and exported STEP. Official
  MCP SDK 1.32.1 also built 4000 mm3, returned PNG and deleted its session.
- Browser sample and direct Parameters dialog were inspected visually at version
  0.3.2. Proof images and logs remain ignored. The generated original catalog icon
  is byte-identical to the existing icon; no template/catalog metadata changes apply.

Prepared image: `solvecraft-unraid:upstream-07e263d` (Linux amd64).
Image ID: `sha256:7f0f03fc2d399852f30e859c9ee89510119258206d901338c1ee865955f1a040`.
Archive: `target/solvecraft-unraid-upstream-07e263d.tar` (53,579,776 bytes).
Archive SHA256: `e89e1954d8d5eaf52ad91d80effee3a4c590c19d1cee703d3c90883af5356e5d`.
The exported manifest names the expected image tag.

The local development runtime `solvecraft-solvecraft-1` first ran this local image; after registry verification it was moved to the immutable public source tag below.
A private pre-update backup was preserved, along with the same persistent volume,
MCP token, loopback port/origins and security/resource limits. Health, authenticated
MCP, PNG, browser assets and read-only reopen of the public saved fixture passed.
The previous registry image is pinned as `solvecraft-unraid:rollback-450eff4`.
Obsolete owned update containers are removed only after successful checks; data
volumes, build caches/toolchain images and rollback images are retained.

Public publication is handled by the tested container workflow after the validated
source push. Registry success, exact source revision/digest and anonymous access
must be verified separately before announcing availability. Unraid and nginx/DNS
are unchanged; install a verified registry update through Unraid's normal container
update action while keeping existing settings, data and token.

Publication completed successfully in [container workflow 38217614599](https://github.com/denara/solidcraft-unraid/actions/runs/38217614599)
from tested integration commit `0dcc905c2ece6ae9eff3120be2f16f98d2aaa793`.
The workflow passed its fresh build, modelling/authentication/origin/file checks,
independent MCP sessions, PNG/STEP, restart persistence and browser assets before
publishing `latest` and `sha-0dcc905c2ece6ae9eff3120be2f16f98d2aaa793`.
Published image: `ghcr.io/denara/solidcraft-unraid:latest` (Linux amd64).
Digest: `sha256:7376ae32e4c49696c32129292ac5a8a81451085f235f93a0b8e31e2369a79344`.
Anonymous pulls of both tags succeeded with an empty Docker client configuration;
an anonymous registry manifest returned HTTP 200 and the same digest. OCI source
labels match the tested integration commit. Upstream main still matched the selected
`07e263d` snapshot at the final check.

The sole local runtime now uses the immutable public source tag, with the same
volume/token and security/resource settings; authenticated SDK modelling/PNG and
read-only public-design reopen passed again. Eight obsolete owned check/candidate
containers were removed without deleting volumes. The old 0.2.1 registry image,
local 0.3.2 archive image, build caches/toolchain images and private backups remain
available. Public template/icon blob hashes exactly match denara/unraid-templates;
no catalog commit or further CA submission was needed. To install on Unraid, use
its normal Docker container update action; this run did not install it there.

## 2026-10-12: gateway authentication and session logs

Added operational stderr events to the HTTP gateway on the validated `07e263d`
CAD baseline. This is a packaging/gateway addition, not a further upstream CAD
update or application release; application version remains 0.3.2.

Events include startup/graceful stop, accepted/rejected Bearer authentication,
origin rejection, CAD session allocation, real process-slot release, pending close,
idle expiry, full capacity, busy sessions, timeouts, subprocess/protocol failures
and rejected CAD/file requests. UTC timestamps, icons and plain `system`, `login`,
`warn`, `error` categories allow viewers to use their own palette without ANSI
escapes. The separately generated log ID is not the HTTP session credential.
Routine successful tool/browser/health requests stay quiet; operational fields
exclude raw tokens/session headers, client metadata, addresses, file paths and
model/request contents. Authentication acceptance and slot allocation are separate.

Validation: 11 gateway tests, warning-free gateway Clippy, formatting, assets,
licenses, dependency layers and ten-target WebAssembly compatibility passed.
Real isolated container checks passed for modelling/PNG/STEP, auth/origin/path
confinement, separate sessions, restart persistence and browser sample rendering.
The new `log-test.ps1` passed its four-slot allocation/release, severity/timestamp,
startup/stop, secret/header exclusion and normal-request silence checks; the
container publication workflow now runs this proof before publishing images.
Tests and evidence remain under ignored `target/unraid-logging/`.

Local image: `solvecraft-unraid:logging`, Linux amd64.
Image ID: `sha256:5db74c8c496c587e56f67113ee1a8c12244409ecef1467d3adb6c62eebde9ab2`.
Archive: `target/solidcraft-unraid-logging.tar` (53,609,472 bytes).
SHA256: `aaf2792d13e96d55c7a962aafa456d8fa36b904b8fdb93ca306c5e54587d888c`.
Registry publication and the final retained local runtime are verified separately
below. Unraid is not updated by this development task; installation uses its
normal Docker container update action, retaining existing settings/data/token.
The canonical public template and original icon have not changed.

The final logging addition also reports `token.location` on every successful startup,
not only when a token is generated. File mode names the trusted container path
(normally `/data/mcp-token`); environment mode names `SOLVECRAFT_MCP_TOKEN`, never
its value. This intentionally disclosed credential-file location is distinct from
omitted design/export paths and host volume mappings. Twelve gateway tests pass,
including repeated startup records for both modes. Fresh-container file-mode tests
confirmed two location records across restart, unchanged token and no token leakage.
The final real log proof and persisted-design reopen also passed. The superseded
initial image workflow was canceled before publication to include this addition.

Final local image: `solvecraft-unraid:logging-token-location` (Linux amd64).
Image ID: `sha256:932090de0fb756106ba2a7e89dcd766e21dfc2a3467ca092f76ab3afcd288cff`.
Archive: `target/solidcraft-unraid-logging-token-location.tar` (53,611,008 bytes).
SHA256: `d37759e8d0086137ff818c709b49b7ba45a6caf25b5041c89ed1098ba45ef4f3`.

Final publication passed [container workflow 38251162829](https://github.com/denara/solidcraft-unraid/actions/runs/38251162829)
from source commit `a3efe6fabedd3d4d6ffe032a4ac94eb43815e299`, including the new
real logging/privacy proof before publication. Public image:
`ghcr.io/denara/solidcraft-unraid:latest`, with immutable rollback/source tag
`sha-a3efe6fabedd3d4d6ffe032a4ac94eb43815e299` (Linux amd64).
Digest: `sha256:893c1260d3feb0f9ab487feb4280ee2e8c790535fc2e949651aec815a5f76723`.
Both tags pulled anonymously with an empty Docker profile; the anonymous registry
manifest returned HTTP 200 with the same digest and OCI source labels matched.

The sole local runtime `solvecraft-solvecraft-1` uses that immutable public tag.
A private pre-update backup and `solvecraft-unraid:rollback-before-logging` were
preserved. Volume, token, loopback/origin and security/resource settings are unchanged.
Health, authenticated SDK modelling/PNG, read-only public-design reopen, web assets
and actual runtime token-location/open/release logs passed without token leakage.
Seven owned obsolete check/proof/candidate containers were removed in total,
including the superseded candidate; no volumes were deleted. Build/toolchain images,
caches, prior images, archives and private backups remain. The public CA template/icon
match the canonical blobs and required no metadata push or new submission.

Later upstream movement was observed at `db0a2a0`; it was deliberately not mixed
into this gateway-only addition. The validated CAD baseline remains `07e263d`.
Unraid/nginx/DNS were not modified. Install the logging addition through Unraid's
normal container update action with the existing settings, data and token.

## 2026-10-13: upstream 913583b (SolidCraft 0.7.0)

Advanced the validated CAD snapshot from `07e263d7e95cac9a6851e48bffcb79f302aabfb8`
to the single selected upstream commit `913583bfeebb67eea406b40c9bc282bbf27726c7`
(PR #463, confirmed merged). The 191 upstream commits include native/container storage,
format 14-16 migrations and exact topology naming, new sheet/surface/text features,
geometry fixes, richer feature replies, UI/navigation and the completed SolidCraft rename.
Upstream application version 0.7.0 is inherited; this is not a new fork application release.

Integrated directly into `codex/unraid-mcp-web`, synchronized with its origin counterpart;
origin/main was already an ancestor. Conflicts in agent instructions, attribution, Cargo.lock,
shipped notices and the license roots retained both the upstream work and deployment rules.
Gateway dependency versions were retained, with no unrelated dependency upgrade. License
notices were regenerated with the HTTP server included. Original openly licensed assets
and analytic/external-tool reference fixtures retain attribution; no prohibited CAD source
was read, linked or used in the build.

Docker now builds the upstream `solidcraft-cli`/`solidcraft-web` packages and ships the
legacy `solvecraft-cli` alias. The unchanged gateway, environment variables, mounts, token,
limits and HTTP endpoint remain compatible. Font file parameters in sketch.text are confined
to the shared files directory, without environment/home expansion. Shared desktop crash
reports are blocked through both direct tools and execute/batch; their tool is omitted from
the network catalog. Requests without arguments pass through the same safety checks.
Two upstream Clippy findings were corrected without changing behavior: a solver boolean
and an HID descriptor match guard. Their selected regressions passed.

Validation used only relevant tests, not the entire suite:

- Formatting, warning-free workspace Clippy/all targets, 42 attributed assets, shipped
  licenses, dependency layers and ten-target WebAssembly compatibility passed.
- 125 selected tests passed: gateway (13), stdio MCP (13), design storage (19), document
  migrations (24), solver (10), native geometry envelope (5), XZ frames (2), naming history
  (8), preferences/legacy keys (15), CLI/catalog/legacy alias/MCP connection (11), HID parser (5).
  Five existing upstream tests in the selected MCP/storage modules remain explicitly ignored.
- Linux amd64 native CLI/server and independent WASM browser release builds passed.
- A fresh isolated four-slot candidate passed sketch/extrusion (24000 mm3), PNG/STEP,
  authentication, origin/path rejection, independent sessions and restart persistence.
  Extended smoke tests rejected shared crash/recovery access, omitted arguments and escaping
  font requests. Actual log checks passed category/timestamp/icons, capacity/release,
  secret/session-header exclusion and silence during ordinary requests.
- Official MCP SDK 1.32.1 opened copied public 0.2.1 and 0.3.2 designs (24000 mm3), exported
  STEP, saved/reopened the new container format, built/rendered 4000 mm3 and deleted its
  session. Uploaded confined Liberation Sans text extruded successfully (27.1114 mm3).
- The actual browser sample visibly rendered at 0.7.0, independently of MCP models. The
  compatibility CLI alias reported 0.7.0. Evidence stays in ignored target/unraid-update-913583b.

Image: `solvecraft-unraid:upstream-913583b` (Linux amd64).
Image ID: `sha256:660d743da7fe150aded0b80301395c5d8008741ab64e3fc46532ca786a209305`.
Archive: `target/solidcraft-unraid-upstream-913583b.tar` (85,583,872 bytes).
SHA256: `b0ebf645b25a55a3c56dada2c717919608d493e2873f0c897caa596abbe67b11`.
The export manifest names the expected image tag.

Publication and local runtime replacement are verified separately below. The canonical
Unraid template/icon have not changed. Existing files remain readable, but designs saved
with the new SolidCraft format/container are not readable by old application builds.
Retain a data backup for rollback; browser storage is separate from the server volume.
No Unraid/nginx/DNS installation or application release/version bump/tag is performed.

Publication completed in [container workflow 38363021056](https://github.com/denara/solidcraft-unraid/actions/runs/38363021056)
from validated merge commit `54213307c09db2352865118952a2cdad79be9a08`. Its fresh image
passed modelling/PNG/STEP, authentication/origin/file and extended font/diagnostic confinement,
independent sessions, restart persistence, browser assets and actual logging/privacy before
publication. Public image: `ghcr.io/denara/solidcraft-unraid:latest`; immutable source tag:
`sha-54213307c09db2352865118952a2cdad79be9a08` (Linux amd64).
Digest: `sha256:b8df1550b94b547373bd6b983a7f23816da506057abd0260340a88c3a6fa8732`.
Both tags pulled anonymously with a fresh empty Docker configuration. An anonymous manifest
returned HTTP 200 and the same digest; OCI source/architecture matched the validated commit.

The sole remaining local runtime `solvecraft-solvecraft-1` now uses that immutable public
source tag and is healthy at 0.7.0. Private data/configuration backup and the previous image
`solvecraft-unraid:rollback-before-913583b` were retained. Persistent volume, MCP token,
ports/origins and security/resource limits are unchanged. Authenticated SDK modelling/PNG,
read-only public saved-design reopen, actual browser rendering and file-token-location plus
session open/release logs passed after replacement. Seven obsolete owned build/check/candidate
containers were removed after inspecting labels, workspace mounts, images and inactivity;
no volumes were deleted. Reusable toolchain/build caches, prior images, archives and private
backups remain. CA template and original icon blobs match their public repository exactly;
no catalog commit or new submission was needed. Install through Unraid's normal SolidCraft
container update action, preserving settings/data/token and a pre-update design backup.
This run did not modify Unraid or nginx/DNS.

Upstream main still matched the selected 913583b snapshot at the final verification.

## 2026-10-13: upstream be55a49 (SolidCraft 0.8.0, format 17)

Advanced the validated snapshot from `913583bfeebb67eea406b40c9bc282bbf27726c7`
to the fixed upstream commit `be55a49b047eb516c88a1f8add5723818cd2b57d` (PR #728,
confirmed merged). The 94 new commits include linked design components, compact MCP
execute/batch replies, named-view/camera screenshots, portable thumbnails, materials and
visibility, exact geometry and fillet improvements, new sketch windows, and format 17.
The inherited application version is 0.8.0; the selected main snapshot includes later
changes beyond the initial 0.8.0 release, which only knows format 16. No fork application
release, version bump or tag is created.

Merged directly into `codex/unraid-mcp-web` after synchronization; origin/main was an
ancestor. Only the generated third-party notices conflicted and were regenerated with
the gateway license roots retained. The upstream quick-xml 0.41.0 security update and
original/attributed assets were retained. Gateway dependency versions remain unchanged.
Toolchain, CLI compatibility alias, web/HTTP endpoints and container settings are retained.

New linked components require confinement on both insert and refresh: the HTTP adapter
resolves component.insert_design file parameters, and each CAD subprocess receives a link
root. Engine reads revalidate resolved stored links against that root, rejecting parent
escape and symlinks even for uploaded documents and later component.update_links calls.
Desktop link behavior is unchanged when no network root is supplied. Regression tests
cover allowed normalized paths and outside/symlink rejection. The public workflow's smoke
proof now inserts/refreshes an allowed link and rejects a tampered stored link before it
can read outside the files directory. Existing font/recovery/diagnostic protections remain.

Validation was scoped to the changes; the entire suite was not rerun:

- Formatting, warning-free workspace Clippy/all targets, 43 attributed assets, license
  notices, dependency layers and ten-target WebAssembly compatibility passed.
- 128 selected tests passed: gateway 13, MCP 23, design storage/thumbnail 20, format
  migrations 24, linked components/confinement 4, rendering backend/request safety 9,
  named views 2, Sketch Scale UI 6, CLI/alias/catalog/connectivity 11, variable/taper/spline
  fillets 6, Sketch Offset UI 2, SVG/3MF parser checks 7, and one Boolean determinism test (six rebuilds). Two
  existing storage benchmarks remain ignored. Initial lock-version and filter diagnostics
  remain under ignored target/unraid-update-be55a49 alongside final passing results.
- Linux amd64 native/server and independent WASM browser release builds passed.
- A fresh isolated candidate passed modelling (24000 mm3), PNG/STEP, auth/origin/file
  confinement, independent sessions, persisted-design reopen after restart and real log
  capacity/release/privacy/noise checks. Tampered stored links were rejected, while allowed
  links inserted and refreshed with unchanged volume. File/font/diagnostic probes passed.
- Official MCP SDK 1.32.1 reopened copied public 0.2.1, 0.3.2 and 0.7.0/format-16 fixtures,
  measured 24000 mm3, exported STEP and saved/reopened the new format. It built/rendered
  4000 mm3, used an uploaded confined font, verified compact execute output and a named
  camera PNG, then refreshed a saved linked part from 6000 to 9000 mm3 and deleted its
  session. The actual independent browser sample visibly rendered at 0.8.0.

Image: `solvecraft-unraid:upstream-be55a49` (Linux amd64).
Image ID: `sha256:ab32131fda537570cf78e8b9f0b05812eaf537063b894a88c86669af7cc61d74`.
Archive: `target/solidcraft-unraid-upstream-be55a49.tar` (87,734,272 bytes).
SHA256: `5509887173d9877a12843001827eb84a83a3276b111618e55f9cc416dae635db`.
The export manifest was checked for the expected tag. Existing saved designs remain
readable; newly saved format-17 designs require this snapshot or a newer one, so retain
pre-update server and separate browser-design backups for rollback.

Publication and final local runtime/cleanup are verified separately below. The canonical
CA template and original icon are unchanged. Unraid/nginx/DNS are not modified. The active
daily automation remains scheduled for 09:00 Europe/Berlin. Later upstream movement at
`a8b3b21fd0ec09f5c5353df16cfbd441158720d6` (left-side feature history) was observed before
publication; it is intentionally left for the next fixed-snapshot update.

Publication passed [container workflow 38437831894](https://github.com/denara/solidcraft-unraid/actions/runs/38437831894)
from validated merge commit `054e751c6a09d9ba571bddcbe0dabbe930069e6e`. The fresh build
passed modelling/PNG/STEP, authentication/origin/file/font/diagnostic and stored-link
confinement, independent sessions, restart persistence, browser assets and operational
logging/privacy checks before publishing the immutable source tag and latest.
Public image: `ghcr.io/denara/solidcraft-unraid:latest` (Linux amd64), immutable tag
`sha-054e751c6a09d9ba571bddcbe0dabbe930069e6e`.
Digest: `sha256:8ae80566da970fcaeed941e0a24ff72c6f721ce7d5c5f14f72fda7f6ee0a87cd`.
Both tags pulled anonymously with a fresh empty Docker configuration; the anonymous
manifest returned HTTP 200 with the same digest and matching OCI source/architecture.

The sole local runtime `solvecraft-solvecraft-1` now runs that public immutable image,
healthy at 0.8.0. A private pre-update data/config backup and rollback image
`solvecraft-unraid:rollback-before-be55a49` (previous public 0.7.0) were preserved.
Volume, token, ports/origins and security/resource limits are unchanged. Authenticated
SDK modelling/PNG and read-only known public saved-design reopen passed after replacement;
the actual browser canvas rendered at 0.8.0 and startup/token-location/session logs passed.
Six owned obsolete check/supplement/candidate containers were removed only after verifying
labels, images, workspace mounts and inactivity. No volumes were deleted. Toolchain images,
Cargo/build caches, archives, earlier images and private backups remain. Public template
and icon blob hashes match exactly, so no CA metadata commit or new submission was needed.

On Unraid, back up server designs and separately export browser designs before updating
SolidCraft through its normal Docker update action; retain existing settings/data/token.
Reload the browser afterward. Format 17 requires this main snapshot or a newer one, including
for rollback between builds that display the same 0.8.0 version. Unraid/nginx/DNS remain
unchanged by this development task; the daily 09:00 Europe/Berlin automation remains active.

The final upstream check observed 16766bdbc79a4b9ec46bb8b9879587f38ce2994d; later movement is deferred to the next fixed-snapshot update.

## 2026-10-14: upstream 8ea1fd64 (SolidCraft 0.10.0, format 20)

Advanced from `be55a49b047eb516c88a1f8add5723818cd2b57d` to the fixed
`8ea1fd643fb70cb17f8e335c57de23f03a3415a4` snapshot (PR #1043, confirmed merged),
merging directly into `codex/unraid-mcp-web`. The 159 non-merge upstream commits include
thin extrusions, surface modelling, material cards, GPU/display improvements, replay-script
export, more localization and the independent browser's offline/update support.
The inherited version is 0.10.0; no application release, version bump or release tag is created.
Origin/main was an ancestor and the deployment branch matched its remote before integration.

Resolved conflicts in Cargo.lock, attribution and generated license notices, preserving
HTTP dependencies and all original/openly licensed assets. Regenerated notices retain the
server license roots. The HTTP worker explicitly allows its files root under upstream's new
native MCP path scope. A new --quiet option suppresses informational scope paths while retaining
protocol output and confinement; a CLI regression test proves both. Existing token, font,
stored-link, diagnostics, origin, session and logging protections remain active.

The native container integration retains forbid(unsafe_code) and omits the new macOS-only
unsafe Finder event bridge; normal command-line file opening and Linux behavior are retained.
Upstream's JavaScript service worker is browser infrastructure, independent of HTTP MCP.
Three small Clippy corrections preserve the affected finite geometry and corner predicates;
the sheet correction is test-only. A test-only enum parser normalizes Windows CRLF before
checking golden feature/reference fixtures; its two failures were not CAD behavior failures.

Validation is scoped, without rerunning the complete test suite:

- Formatting, workspace/all-target Clippy with warnings denied, attribution/assets, licenses,
  layers and all eleven WebAssembly targets passed. The upstream wasm-only preview enum
  still emits one harmless dead-code warning; native Clippy is clean.
- 176 selected tests passed: gateway 13, stdio MCP 35, VFS confinement 8, design archives 20,
  format migrations 27, renderer backend 9, GPU module 5, offline generator 6, CLI 10,
  mesh-grid 4, linked designs 4, sheet regression 1, thin-extrude UI 3, reference walks 4,
  sketch corners 13, material cards 8 and thin-extrude engine 6. GPU comparisons permit
  adapter-unavailable skips; no physical GPU certification is claimed. Three existing
  benchmarks remain ignored. Initial diagnostic logs and corrected test filters remain local.
- Fresh isolated candidate passed 24000 mm3 modelling, PNG/STEP, authentication, origin,
  path/font/diagnostic/stored-link confinement, independent sessions, restart persistence,
  capacity/release/noise/privacy logs and the compatibility CLI alias.
- Official SDK 1.32.1 reopened copied public 0.2.1, 0.3.2, 0.7.0/format16 and 0.8.0/format17
  files at 24000 mm3, exported STEP, and saved/reopened format20. It rendered 4000 mm3,
  used a confined uploaded font, checked compact output and named camera rendering, refreshed
  a linked part from 6000 to 9000 mm3 and built/rendered a 2400 mm3 thin wall.
- The actual browser independently rendered Sample Plate at 0.10.0. Root-absolute asset URLs
  prevented the copied container site from starting in the upstream subpath test harness;
  packaging now uses relative URLs, as upstream's default does. Final native CLI/server
  SHA256 hashes are unchanged by this web fix; saved-design reopen passed on the final image.
- Chromium with software graphics tested the final built site through all 19 offline/update
  scenarios (21 launches): offline/slow start, subpath isolation, broken/overlapping downloads,
  hash consistency, tab retirement, removed deployments and development worker retirement.
  Offline caches contain only generated public web assets, never MCP or saved server designs.
  The publishing workflow now verifies sw.js/build.json and tracks build-generator changes.

Image: `solvecraft-unraid:upstream-8ea1fd64` (Linux amd64).
Image ID: `sha256:461654a1a71877ce1e742f60f2a889df8d2203b66433e3cf3af420c30d065ede`.
Archive: `target/solidcraft-unraid-upstream-8ea1fd64.tar` (93,068,800 bytes).
SHA256: `33aed40bd5dfd5002432e419bec6af1472bd8cade91febb6aa188fad277ddefc`.
Export manifest and expected tag were checked. Evidence remains under ignored
`target/unraid-update-8ea1fd64/`, including the native thin-wall PNG.

Public image publication and local runtime replacement/cleanup are verified separately below.
The canonical CA template and original icon hashes match the public repository exactly, so
no catalog commit or submission is needed. No Unraid/nginx/DNS changes are made. Before an
Unraid update, back up server designs and separately export browser designs; format20 saves
need this snapshot or newer for reading, including rollback. The daily 09:00 automation remains.
Later upstream main moved to `54a8c807bf205a12b1c15d896e8325c2adfd2ae6`; its libraries and sheet
perforations are deferred to the next fixed-snapshot run.

Publication passed [container workflow 38601814806](https://github.com/denara/solidcraft-unraid/actions/runs/38601814806)
from validated merge `a28c4d5f70593b034e5956e042157260fa85e236`. The public build repeated
modelling/PNG/STEP, auth/origin/file/font/diagnostic/stored-link confinement, independent
sessions, restart persistence, browser/offline assets and logging/privacy proofs before push.
Public image: `ghcr.io/denara/solidcraft-unraid:latest` (Linux amd64), immutable tag
`sha-a28c4d5f70593b034e5956e042157260fa85e236`.
Digest: `sha256:7b8a1790e3b45388d95ce67485404c5c96a671c01c45c9a3c57ab0dbb121f6e2`.
Both tags pulled anonymously with an empty Docker configuration; the anonymous manifest
returned HTTP 200 with matching digest, OCI source and architecture.

The sole local runtime `solvecraft-solvecraft-1` now runs that immutable public image,
healthy at 0.10.0. A private data/config backup and rollback image
`solvecraft-unraid:rollback-before-8ea1fd64` (previous public 0.8.0) remain available.
Volume, MCP token, full environment, user, ports/origins and resource/security limits are
preserved. Authenticated SDK modelling/PNG and read-only known public saved-design reopen
passed; the actual browser visibly rendered Sample Plate at 0.10.0 after replacement.
Thirteen owned obsolete build/check/offline/fixture/candidate containers were removed after
label, image, mount and activity checks. Diagnostics remain local; no volumes were deleted.
Reusable toolchain/browser images, build caches, archives and private backups remain intact.
No CA metadata commit was required and no Unraid installation or nginx/DNS change was made.

Before updating through Unraid's normal Docker action, back up server and separate browser
designs, retain the existing token/settings/data mapping and reload the browser afterward.
New format20 designs require this snapshot or newer. The final upstream check observed
`805ea45be6db658061cb44d13fcc4d532bb04f5e`; later changes are left to the next fixed-snapshot run.

## 2026-10-15: upstream b3df2c38 (SolidCraft 0.11.0, format 21)

Advanced the validated and publicly distributed baseline from
`8ea1fd643fb70cb17f8e335c57de23f03a3415a4` to the single fixed snapshot
`b3df2c382b5cc600e42273ed6c21466bab181a71` (PR #1313, confirmed merged).
The 100 non-merge upstream commits add headless CPU render jobs, screenshot cameras and
optional grid/axes, local material/appearance libraries, model metadata, improved joints,
sheet conversion/perforations, sketch patterns, asynchronous UI evaluation and original
rendering work. The inherited main-branch application version is 0.11.0 and design format 21;
this integration does not create an application release or release tag.

The branch was clean and synchronized with origin/codex/unraid-mcp-web; origin/main was
an ancestor. Merged directly into the sole deployment branch. Conflicts were resolved in
AGENTS.md, CLAUDE.md, ATTRIBUTION.md, native main and kernel coplanar handling. Kept strict
fork privacy/unsafe rules, all original asset attribution and the upstream GrownBoss union
branch. Removed a duplicated historical icon row from the vendored-source table while
retaining its canonical original-asset attribution. Gateway dependencies and compatibility
names are preserved; the workspace lock advances the server's inherited version only.
Generated license notices remain current and include the HTTP adapter.

The Linux container fork also omits the new Windows-only unsafe native exception bridge,
as it already omits the macOS Finder bridge; Rust crash/recovery and safe startup handling
remain present. No unsafe exception is introduced. Three behavior-preserving lint adjustments
make new pattern validation, seam predicates and the generic Option/Vec/array walks compile
under warning-denied checks. Golden expression/reference/geometry and targeted regression
checks cover the walks and the resolved geometric behavior.

Validation was change-scoped; the complete test suite was not rerun:

- Formatting and warning-denied workspace/all-target Clippy passed, alongside all 87
  attributed assets, dependency layering (16 crates), license notices and eleven-target
  WebAssembly compatibility. The wasm build reports an unused scope field in upstream's
  off-thread UI request; native Clippy is clean. The new proc-macro crate also builds/tests;
  it declares no standalone unit tests and is exercised by document golden fixtures.
- 247 selected tests passed: 191 in the main MCP/VFS/archive/format/catalog/reference-walk,
  render admission/job/texture, library/link and CLI checks; 40 joint, periodic seam and
  asynchronous UI regressions; 16 direct path/font, sketch-pattern, watertight primitive and
  coplanar/Boss-union tests. Two existing archive benchmarks remain ignored. Initial lint
  diagnostics and corrected zero-match filters are preserved privately under target.
- The release Linux amd64 container and independent WASM app built. A fresh test volume,
  loopback port and disposable credential passed 24000 mm3 sketch/extrusion, PNG/STEP,
  auth/origin/file/font/diagnostic/stored-link confinement, independent sessions, restart
  reopen, compatibility alias and logging capacity/release/noise/privacy proofs.
- Official MCP SDK 1.32.1 reopened copied public 0.2.1, 0.3.2, 0.7.0/format16,
  0.8.0/format17 and 0.10.0/format20 files at 24000 mm3, exported STEP and roundtripped
  the current native design. It also rendered 4000 mm3, used a confined uploaded font,
  checked compact replies and a named camera, refreshed a link from 6000 to 9000 mm3,
  and rendered a 2400 mm3 thin wall.
- New behavior was exercised on that actual image: a guides-free native screenshot,
  library export/inspect with forbidden outside paths, CPU render.start/status/export to
  a valid PNG, and rejection of another session's render-job identity. No real saved file
  or token was used for these probes. Native proof PNGs were inspected.
- The actual browser independently rendered Sample Plate at 0.11.0. Chromium with software
  graphics ran the built site through all 19 offline/update scenarios (21 launches),
  including failed/overlapping downloads, tab retirement, subpath isolation and offline
  startup. No physical GPU, Windows hardware or native macOS behavior is certified here.

Image: `solvecraft-unraid:upstream-b3df2c38` (Linux amd64).
Image ID: `sha256:a3cd06061800ee47caae8b6f9f2b5d49cfa9182a076a111de09234617a362723`.
Archive: `target/solidcraft-unraid-upstream-b3df2c38.tar` (97,716,224 bytes).
SHA256: `239eaf0630ee9e359bb0e5e32023d524772441e5b9e6afdce36f940a0c1e8518`.
The export manifest has the expected tag. Logs and public-safe visual evidence stay in
ignored target/unraid-update-b3df2c38, outside Git and the container build context.

Registry publication and the authorized local replacement/cleanup are verified separately
below. The canonical CA template and original icon hashes still match the public catalog
repository; no metadata commit or new submission is needed. No Unraid/nginx/DNS changes
are made. Keep server and separate browser-design backups before updating: format21 saves
need this snapshot or a newer build, including during rollback. Upstream main still matched
the selected b3df2c38 snapshot at the pre-publication fetch.

Publication passed [container workflow 38698443874](https://github.com/denara/solidcraft-unraid/actions/runs/38698443874)
from validated integration commit `123d041b368826c7e932ddf82274c2379f0e9f4b`. The fresh
public build repeated modelling/PNG/STEP, independent sessions, authentication/origin and
file/font/diagnostic/stored-link confinement, persisted reopen after restart, browser assets
and operational logging/privacy checks before publishing latest and the immutable source tag.
Public image: `ghcr.io/denara/solidcraft-unraid:latest` (Linux amd64), immutable tag
`sha-123d041b368826c7e932ddf82274c2379f0e9f4b`.
Digest: `sha256:b1797a37108330b0402a15b279b192eabf2bfffeb2fbf26e9fd56520a06c74a2`.
Both tags pulled anonymously using an empty Docker configuration; an anonymous manifest
request returned HTTP 200 with matching digest, source label and architecture.

The sole remaining local runtime `solvecraft-solvecraft-1` now runs that immutable public
image, healthy at 0.11.0. Private pre-replacement data/config backups and the prior public
0.10.0 image `solvecraft-unraid:rollback-before-b3df2c38` remain available. Volume, token,
full environment, user, ports/origins and security/resource settings are unchanged.
Authenticated SDK modelling/PNG and read-only known public saved-design reopen at 24000 mm3
passed after replacement; the actual independent browser rendered Sample Plate at 0.11.0.
Eight owned obsolete check, supplement, fixture, security, offline and candidate containers
were removed only after inspecting labels, images, workspace mounts and activity. No volumes
were deleted. Diagnostic logs, reusable toolchain/browser images, Cargo/build caches,
archives, old compatibility fixtures and private backups remain intact.

No CA metadata commit or new submission was needed. Unraid/nginx/DNS were not changed.
On Unraid, back up server designs and separately export browser designs, update the existing
container through the normal Docker action with the same settings/data/token, then reload
the browser. Newly saved format21 documents require this snapshot or newer for reopening.
The final upstream fetch observed `6f115ad7409af91716fe52b41bea6e6bacffcece`; subsequent
upstream movement is deferred to the next fixed-snapshot run.

## 2026-10-16: upstream 3707a67c (SolidCraft 0.13.1 development snapshot, format 25)

Advanced the validated baseline from `b3df2c382b5cc600e42273ed6c21466bab181a71`
to the single fixed upstream snapshot `3707a67cee8ff9da897d9a9b9b5174739a0d4ee2`.
Its 145 non-merge commits include typed feature-field roles (PR #1273, confirmed merged),
Motion Trace, mesh collision, sketch-line Split Body, revised surface/sheet/tessellation
operations, shared GPU/CPU capture, glTF imports, and open FreeCAD/Rhino file imports.
The package reports 0.13.1; main already carries five pending design-format changes and
writes format 25. This is a tested development snapshot, not an application release.

Started with a clean, synchronized codex/unraid-mcp-web; origin/main was an ancestor.
Merged directly into that sole branch, preserving published history. Resolved conflicts
in instructions, Cargo.lock, generated notices, desktop lint/startup declarations, pattern
validation and corner selection. Retained new direction validation and construction-side
selection. The new Windows unsafe resize bridge is omitted alongside the existing Finder
and Windows native-crash bridges; safe egui resizing and Linux/container behavior remain.
Two equivalent boolean simplifications satisfy warning-denied Clippy without relaxing it.
The fork retains strict shipped-license freshness and the HTTP server license/layer roots.
Cargo.lock adds the adapter graph to the selected upstream lock without unrelated upgrades.

Validation was scoped to changed behavior and deployment boundaries, not the entire suite:

- 312 selected tests passed: 243 HTTP gateway, stdio MCP, scoped VFS, archive, glTF/native
  import, format/golden walk, direct path/font, capture, file/link, sketch pattern/corner
  and CLI tests; 69 general surface-fill, adapter-policy, joints/Motion Trace, coplanar
  and sketch-line Split Body checks. Five existing tests/benchmarks remain ignored.
- Formatting, workspace/all-target Clippy with -D warnings, 90 attributed assets, dependency
  layering (16 crates), regenerated notices and eleven-crate WebAssembly checks passed.
  cadmpeg's Apache-2.0 open-format codecs only (fcstd/rhino/step) are enabled; commercial
  formats and sat are disabled. No prohibited CAD code was read, linked or copied.
- Built the release Linux amd64 container and independent WASM app. A new test volume,
  loopback port and disposable token passed HTTP MCP sketch/extrusion at 24000 mm3,
  PNG/STEP, independent sessions, auth/origin/file/font/diagnostic/stored-link rejection,
  persisted reopen after restart, and operational logging/capacity/release/privacy/noise.
- Official MCP SDK 1.32.1 reopened public 0.2.1/0.3.2 and format16/17/20/21 files at
  24000 mm3, exported STEP and saved/reopened the current format. The actual ZIP contains
  document.json stamped solidcraft/25 and the five pending change names. Verified uploaded
  font geometry, compact replies/named cameras, a saved link changing 6000 to 9000 mm3,
  a 2400 mm3 thin wall, confined library paths, completed PNG render jobs and job isolation.
- New imports were exercised through real HTTP MCP: public FCStd/3dm boxes each measure
  6000 mm3; our analytic tetrahedron loads from glTF with a relative external buffer.
  Parent traversal, encoded traversal, absolute paths and network buffer URIs are rejected.
  Sketch-line Split Body yields 2400/3600 mm3 from a 6000 mm3 box. Screenshot metadata
  explicitly reports the container's CPU fallback and its limitation warning.
- The actual browser independently rendered Sample Plate at 0.13.1; native proof PNGs were
  visually inspected. Chromium software graphics passed every built-site offline/update
  scenario (19 scenarios, 21 launches). No physical GPU, Windows or macOS certification.
  An initial new SDK upload assertion expected 200 instead of the valid 201 response;
  corrected the harness and repeated the affected SDK checks successfully.

Image: `solvecraft-unraid:upstream-3707a67c` (Linux amd64).
Local image ID: `sha256:4c3930dc445b2b7f1004105045de5ff5c395c9b09487f72a8fc5f5a2b21853eb`.
Archive: `target/solidcraft-unraid-upstream-3707a67c.tar` (142,574,080 bytes).
SHA256: `7a7ed1eec37013167bfb4c819db867e9c866e50018e7db41bf66b7b51823b432`.
Exported manifest names the expected image tag. Private diagnostics and public proof models
remain under ignored target/unraid-update-3707a67c, excluded from Git and build context.
The canonical CA template and original icon still match denara/unraid-templates; no catalog
commit or portal action is required. Publication/local replacement evidence follows below. No Unraid installation or nginx/DNS change is performed.

Before updating Unraid, back up server files and separately export browser designs; preserve
the existing token, data mapping and settings. Reload the browser after the container update.
Format25 saves require this snapshot or a compatible newer build; the released 0.13.1 and
older images must not be assumed to reopen them. Keep pre-update copies for rollback.
Publication passed [container workflow 38867713254](https://github.com/denara/solidcraft-unraid/actions/runs/38867713254)
from validated source/merge commit `8e0bc62b0c6ce3284506df38ae0d9ce5297ac1a7` on
codex/unraid-mcp-web. The fresh CI image passed modelling, PNG/STEP, independent sessions,
auth/origin/file/font/diagnostic/stored-link checks, persisted reopen after restart, browser
assets and operational logging/privacy checks before pushing the immutable source tag and latest.
Public image: `ghcr.io/denara/solidcraft-unraid:latest`, immutable tag
`sha-8e0bc62b0c6ce3284506df38ae0d9ce5297ac1a7` (Linux amd64).
Digest: `sha256:d3fcdf5a53afe75995da524ce6eaf494e0619d1e94017fa83bb933624e89aa6e`.
Both tags pulled anonymously through an empty Docker configuration, and an anonymous
manifest request returned HTTP 200 with matching digest, OCI source revision and architecture.

The sole remaining local runtime `solvecraft-solvecraft-1` runs that immutable public image,
healthy at 0.13.1. A private pre-replacement data/config backup and the previous public
0.11.0 image `solvecraft-unraid:rollback-before-3707a67c` remain available. The same data
volume, token, complete environment, user, ports/origins and security/resource limits were
verified after replacement. Authenticated SDK modelling/PNG and read-only reopening of the
known public saved example at 24000 mm3 passed without saving over existing data. The actual
independent browser visibly rendered Sample Plate at 0.13.1 after replacement.
Five owned obsolete check, fixture, offline, supplement and candidate containers were removed
only after label, image, workspace/volume mount and activity inspection. No volumes were deleted.
Diagnostics, reusable toolchain/browser images, Cargo/build caches, private backups and the
previous image remain available. No CA metadata commit, Unraid installation or nginx/DNS
change was needed. On Unraid, back up server designs and separately export browser designs,
update the existing container with unchanged settings/data/token, and reload the browser.
Format25 designs require this snapshot or a compatible newer build for reopening.
The final upstream fetch observed `db1fd39139596ecfcce0c27dfcc9fa33d1c45e5b`; changes after the fixed target
are deferred to the next update run.

## 2026-10-17: upstream c5992386 (SolidCraft 0.14.0 development snapshot, format 26)

Advanced the validated baseline from `3707a67cee8ff9da897d9a9b9b5174739a0d4ee2`
to the single fixed snapshot `c59923865324cb487ff90def4361ad9c7d8a2556`
(PR #1827 confirmed merged). Its 91 non-merge commits add cam couplings, motion plots,
interference analysis and GIF/PNG animation export, Class-A surface work, feature-pattern
alignment, expression-driven hole positions, shared navigation controls, viewport depth fixes,
Linux spacenavd support and a corrected SVG picker filter. Upstream's package version is
0.14.0; main has three pending changes beyond the folded format25 and writes format26.
This is container packaging of a tested main snapshot, not an application release or tag.

Started clean and synchronized with origin/codex/unraid-mcp-web; origin/main was an ancestor.
Merged directly into that sole branch without conflicts, preserving published merge history,
HTTP MCP/browser independence, strict unsafe/privacy rules and the omitted native bridges.
Three equivalent boolean simplifications (curve-blend length validation and two feature
pattern/mirror Option predicates) satisfy warning-denied Clippy; no warning was suppressed.
The first local build was cancelled and rebuilt with those final corrections. Diagnostics
remain private. Cargo.lock changes the gateway's inherited version only beyond upstream's
graph; weezl's MIT/Apache-2.0 LZW code was already an indirect dependency. Regenerated notices
and server license/layer roots remain correct. The canonical CA template/icon hashes match
denara/unraid-templates; no catalog commit or portal submission is needed.

Validation was scoped to the changes and deployment boundaries, not the complete test suite:

- 382 selected tests passed: 329 gateway/stdio MCP, VFS/archive, format/golden walks,
  direct file/font scoping, motion/analysis/animation/cam/joints, file commands, GIF,
  navigation, CLI and curve-blend checks; 53 feature-mirror/pattern, file-picker,
  expression-driven hole and surface command regressions. Five existing tests/benchmarks
  remain ignored. The analytic rounded-corner rolling-ball case passed in the selected
  31-case blend group; it is a longer check, not a timeout or ignored failure.
- Formatting, workspace/all-target Clippy with -D warnings, all 95 attributed assets,
  dependency layering (16 crates), fresh license notices and eleven-crate Wasm checks passed.
  The Linux amd64 release CLI/gateway and independent browser app built successfully.
- A new named test volume, loopback port and disposable token passed real HTTP MCP
  sketch/extrusion at 24000 mm3, PNG/STEP, independent sessions, auth/origin and
  file/font/diagnostic/stored-link confinement, restart reopen, compatibility CLI alias,
  logging severity/icons, authentication/session capacity/release and no secret/noise leakage.
- Official MCP SDK 1.32.1 reopened copied public 0.2.1/0.3.2 and format16/17/20/21 designs,
  plus the previous snapshot's format25 with its five pending names, at 24000 mm3.
  Save/reopen produced an actual ZIP document.json stamped solidcraft/26 with cam-couplings,
  fillet-corner-and-loft-continuity and hole-position-expressions. STEP, uploaded-font
  geometry, compact/named-camera replies, linked-file refresh (6000 to 9000 mm3), thin
  extrusion (2400 mm3), render-job PNG export and cross-session job rejection also passed.
  Existing public FCStd/3dm/glTF imports and their external buffer path guards still passed.
- New behavior passed on that actual image through HTTP MCP: explicit window screenshots
  correctly error in the headless service; model pictures remain available. A slider study
  reports the analytic -5 mm/s velocity and detects intentional overlap. An exported GIF
  has three real 64x40 frames; three PNG frames render different start/end placements.
  Out-of-workspace GIF/folder destinations are refused and joint values stay unchanged.
  Native start/end PNGs were visually inspected, without using or overwriting real designs.
- The actual independent browser rendered Sample Plate at 0.14.0. Chromium software
  graphics passed all 19 built-site offline/update scenarios (21 launches). Physical GPU,
  Windows/macOS platform bridges and a real SpaceMouse are outside this container proof.

Image: `solvecraft-unraid:upstream-c5992386` (Linux amd64).
Local image ID: `sha256:9e98221b1506044025660b77a9fcc92cf03969a61eb13c19a5c1531f91eceaa5`.
Archive: `target/solidcraft-unraid-upstream-c5992386.tar` (144,539,136 bytes).
SHA256: `dd46b82831c059dec4045d2825398b1c6b24c638064d60c8d22b8883ceb0f3eb`.
The export manifest names the tested tag. Proof models, PNG/GIF files, logs and private
configuration stay under ignored target/unraid-update-c5992386, excluded from Git/build context.
Verified registry publication and local replacement/cleanup are recorded separately below.
No Unraid installation, nginx/DNS change or application release is performed.

Before updating Unraid, back up server files and separately export browser designs. Preserve
the existing token, data mapping and settings, then reload the browser. Format26 saves need
this snapshot or a compatible newer build; upstream's released 0.14.0 reads up to format25.
Retain pre-update design copies and the previous image for rollback.

Publication passed [container workflow 39013719874](https://github.com/denara/solidcraft-unraid/actions/runs/39013719874)
from validated integration/source commit `8d476cedf3b90671a4ca347a5f3d168b6d5651ad`
on codex/unraid-mcp-web. The fresh CI image passed modelling/PNG/STEP, independent sessions,
auth/origin/file/font/diagnostic/stored-link confinement, persisted reopen after restart,
browser assets and operational logging/privacy checks before registry publication.
Public image: `ghcr.io/denara/solidcraft-unraid:latest`, immutable tag
`sha-8d476cedf3b90671a4ca347a5f3d168b6d5651ad` (Linux amd64).
Digest: `sha256:05b66b15a73a33be6fcbb970281ca24d98d2f4f937b8ae8976787a320bc71a7b`.
Both tags pulled anonymously using an empty Docker configuration. An anonymous registry
manifest request returned HTTP 200 and matched the digest, OCI source revision and architecture.

The sole remaining local runtime `solvecraft-solvecraft-1` runs that immutable public image,
healthy at 0.14.0. Its private pre-replacement data/config backup and previous public 0.13.1
image `solvecraft-unraid:rollback-before-c5992386` remain available. The data volume, MCP token,
complete environment, user, ports/origins and all resource/security settings were preserved.
Authenticated SDK modelling/PNG and read-only reopen of the known public saved example at
24000 mm3 passed after replacement without saving over existing designs. The actual browser
independently rendered Sample Plate at 0.14.0 after replacement.
Five owned obsolete checks, fixture, offline, supplement and candidate containers were removed
only after inspecting fresh labels, images, mounts and activity. No volumes were deleted.
Diagnostics, reusable toolchain/browser images, Cargo/build caches, private backups, compatibility
fixtures, archives and rollback images remain. No CA metadata commit, portal action, Unraid
installation or nginx/DNS change was required. On Unraid, back up server files and separately
export browser designs, update the existing container with the same settings/data/token and
reload the browser. Newly saved format26 documents need this snapshot or a compatible newer build.

The final upstream fetch observed `c59923865324cb487ff90def4361ad9c7d8a2556`. The selected target was still current.

## 2026-10-17 — Protected file and session workspace

Implemented the approved dashboard on `codex/unraid-mcp-web`, published from source
`51b53cf9c562e14ae3fad5598631be1b5fc09241`. This deployment feature keeps the validated
CAD baseline `c59923865324cb487ff90def4361ad9c7d8a2556`, version 0.14.0 and format26.
It also publishes the preceding Unraid category-color correction `dfbaf6ed`.
The final upstream check observed `6bde126a` (grid-union regression seed, #1813);
those newer CAD changes are separate from this feature and were not integrated here.
Origin's integration branch was synchronized; origin/main remained an ancestor.

`/workspace` now provides the approved dark file/session overview, search, filters and
authenticated downloads. It distinguishes opened files, in-flight requests, confirmed
unsaved changes and new unsaved models. The existing MCP token grants a bounded browser
session through an HttpOnly, SameSite=Strict cookie, with Origin/Host and CSRF checks;
the cookie does not authenticate `/mcp` or the Bearer file API. Browser CAD stays independent.
The gateway adds `workspace_list_files` and `session_status`, strong file revisions,
conditional uploads and stale-save rejection across shared files. No private identifiers,
credentials or filesystem roots are included in the dashboard metadata.

Validation focused on the changed boundaries: 44 gateway tests, 48 MCP tests and three
CLI MCP-connect tests passed (95 total), plus formatting, warning-denied workspace/all-target
Clippy, 95 attributed assets, layering for 16 crates, sorted catalogs, licence inventory
and Wasm checks for eleven crates. The unrelated full CAD/kernel suite was not repeated.
The isolated candidate used a fresh test volume, loopback listener and disposable credential.
Real HTTP MCP modelling, PNG/STEP export, authentication/origin/path confinement, independent
sessions, restart persistence and operational logging/privacy passed. The new proof exercised
two sessions sharing a file, a rejected stale save with the in-memory 36000 mm3 model retained,
an unrelated export that cannot reset a stale baseline, conditional PUT responses 428/412/201,
internal-method rejection and dashboard login/filter/download/logout with credential separation.

Native browser checks in the in-app browser and Chrome/Vivaldi found and verified a fix for
form authentication: `Referrer-Policy: same-origin` preserves the required same-origin POST
Origin while suppressing external referrers. Actual dashboard screenshots use public example
models and real session metadata. The independent CAD browser rendered Sample Plate at 0.14.0.
Proofs and screenshots remain in ignored `target/workspace-proof`, outside the build context.

[Container workflow 39024407871](https://github.com/denara/solidcraft-unraid/actions/runs/39024407871)
rebuilt the full image and passed modelling/export, restart reopen, browser assets, logging/privacy
and workspace/concurrent-revision checks before publishing. Public Linux amd64 image:
`ghcr.io/denara/solidcraft-unraid:latest`, immutable tag
`sha-51b53cf9c562e14ae3fad5598631be1b5fc09241`.
Digest: `sha256:1a3b54ed9c363168e217ff332cdd65b405c0fc43063b8b1277a67d1de9938dbf`.
Both tags pulled anonymously using an empty Docker configuration; an anonymous registry
manifest request returned HTTP 200 with the matching digest, source label and architecture.

Archive: `target/solidcraft-unraid-workspace-51b53cf9.tar` (146,164,224 bytes), tagged
`solvecraft-unraid:workspace-51b53cf9`.
SHA256: `d0e70fe3ad985204878372328c2ed064b93db2381d7df1aff1e5a9bf4cd5a332`.

After a private data/config backup and an idle-process check, the sole local test runtime
`solvecraft-solvecraft-1` was replaced with the immutable public image. Health, authenticated
SDK modelling/PNG, read-only reopen of the known public 24000 mm3 saved example, dashboard
cookie login/API/logout and the actual independent browser CAD passed after replacement.
The data volume, MCP token, complete environment, user, ports/origins and security/resource
settings were preserved. The owned disposable workspace candidate was then stopped and removed
after fresh label/image/mount/port checks; its diagnostics were retained. Build containers
used automatic removal. No volumes were deleted or global prune used. Toolchain images,
build caches, private backups, proof fixtures and the previous image tagged
`solvecraft-unraid:rollback-before-workspace` remain available.

Limits: manual refresh, bounded inventory and a shared-token trust model; no filesystem lock
against external writers. Batch rollback cannot undo filesystem writes. To avoid bypassing
the new revision guards, the network gateway conservatively rejects multi-target URDF,
per-body DXF file output and PNG animation frame directories. Ordinary STEP/3MF/PNG and GIF
exports remain available. This adds no new design format or application version.

The canonical Community Applications template and original icon are unchanged, so no catalog
commit or portal action was required. No Unraid installation or nginx/DNS change was performed.
On Unraid, save active models, preserve/back up existing files and browser designs, then update
the container with its existing settings/data/token. Open `/workspace` and sign in with the
existing MCP token; reconnect MCP clients to discover the two new gateway tools.

## 2026-10-17 — Upstream 72145d4a with preserved Unraid extensions

Selected `72145d4aba05eab66e7a49b383bee9c9f5764da3` once at the start and merged it directly
into `codex/unraid-mcp-web`, advancing the validated upstream baseline from `c5992386`.
Origin's integration branch was synchronized and origin/main was an ancestor. Upstream PR2035
was confirmed merged at the selected commit. Later upstream movement is tracked separately.
The application remains version 0.14.0; this is a container snapshot, not an app release/tag.

This snapshot includes geometry and STEP-import corrections, sweep guide rails, clearance
and silhouette analysis, script/batch timings, a safer ZIP dependency, reader mutation
coverage, the original SVG icon system, translations, browser-local command bindings and
the ground-plane grid preference. All published fork extensions remain: independent web
CAD and MCP models, protected `/workspace`, authoritative session metadata, cookie/Bearer
separation, file confinement/revisions, operational category colors and token-location logs.

Resolved the agent-instruction conflict with all fork safeguards retained, accepted the
replacement of the superseded HID parser by the upstream pure-Rust shared input crate,
and regenerated shipped licences from the merged dependency graph including the gateway.
Two equivalent boolean simplifications satisfy warning-denied Clippy (mixed fractions and
STEP whole-sphere validation). The icon registry test caught a missing icon on the new
programmatic `inspect.silhouette` command; it now uses the established inspection icon.
No artwork or accepted review hashes were changed. Narrow Git attributes retain canonical
LF bytes for SVG masters and their palette, avoiding Windows CRLF hash/revision failures.

Validation was scoped to the changes, not the entire unrelated test suite:

- 402 selected Rust tests passed across gateway/MCP, all file readers, format/feature walks,
  expressions, confined file access, clearance/silhouette, sweep rails, sheet cuts, placed
  mirrors, timing, relevant kernel/STEP cases, icon rendering, navigation, shared HID input
  and CLI connections/results. Existing ignored cases remain ignored. The large-sphere
  distance regression passed in 229 seconds. The missing-icon failure was fixed and the
  affected gates/tests rerun, rather than suppressing or weakening the check.
- All-target workspace Clippy with warnings denied, formatting, 369 attributed assets,
  268 icon masters and their 13 Python tool regressions, sorted catalogs, 17-crate layering,
  regenerated licence inventory and twelve-crate Wasm checks passed. Linux amd64 CLI/gateway
  release and the independent Trunk browser build succeeded.
- The actual candidate used a fresh named volume, free loopback port and disposable token.
  Real HTTP MCP modelling at 24000 mm3, PNG/STEP, independent sessions, authentication/origin/
  path checks, restart reopen, category-color log/privacy checks and all workspace proofs
  passed, including stale-save rejection with the model retained and conditional uploads.
- Official MCP SDK 1.32.1 reopened eight public older designs at 24000 mm3, including the
  previous snapshot's format26 with its three pending changes. New native/3MF ZIP saves
  were verified; new format26 saves list five pending changes, adding `mirror-placements`
  and `sweep-guide-rails`. Batch profiling, a 1 mm3 clearance overlap and a 40x60 silhouette
  passed. A guide-rail frustum saved and reopened at 7329.7796 mm3 (analytic 7330.3829 mm3,
  within the upstream 0.2 percent mesh-volume tolerance) and produced an actual native PNG.
- The real browser rendered Sample Plate with the new icons. Native dashboard login and
  file search passed while MCP models stayed independent. Public-safe screenshots, models,
  logs and private local recovery data stay under ignored `target/unraid-update-72145d4a`,
  excluded from Git and container build contexts.

Before updating Unraid, save active models and back up server files and browser designs
separately. Preserve the existing data mapping, token and settings. Older format26 builds
may refuse files carrying the new pending markers; keep pre-update copies and the prior
image for rollback. No Unraid installation, nginx/DNS change or application release is
performed here. Registry publication, archive checksum, local replacement/cleanup and the
description-only catalog synchronization are recorded below after verification.

The additional browser regressions passed on the actual built site: its local command API
discovers commands, models/undoes/redoes a 6000 mm3 box, rejects invalid inputs and safely
drains 256 queued calls while refusing the 257th. All 19 offline/update scenarios passed
(21 page launches) in isolated Chromium with software graphics. A single upstream Python
trailing space was removed at the final staged-diff check; its 13 tool tests passed again.
No modelling behavior changed after the successful candidate proofs.

Publication passed [container workflow 39042081456](https://github.com/denara/solidcraft-unraid/actions/runs/39042081456)
from validated merge/source `fccb86ec42f0f97b5993b5a329c843ca6fab24b9`. The fresh CI image
passed HTTP MCP modelling/PNG/STEP, independent sessions, authentication/origin/path checks,
restart persistence, browser assets, logging/privacy and workspace/concurrent-revision
proofs before registry push. Public Linux amd64 image: `ghcr.io/denara/solidcraft-unraid:latest`,
immutable tag `sha-fccb86ec42f0f97b5993b5a329c843ca6fab24b9`.
Digest: `sha256:a981b6967f9167495cebd153fff284602a538d95ce806d83a26717eb15135305`.
Both tags pulled anonymously with an empty Docker configuration. An anonymous manifest
request returned HTTP 200 and matched the digest, source revision and architecture.

Archive: `target/solidcraft-unraid-upstream-72145d4a.tar` (148,038,144 bytes), tagged
`solvecraft-unraid:upstream-72145d4a`.
SHA256: `6409dc4f54c9fa322af60218d48580df96e14f6bc4bb05ecc593016a94b93404`.

The sole remaining local test runtime `solvecraft-solvecraft-1` runs the immutable public
image, healthy at 0.14.0. Private initial and fresh pre-replacement data/config backups and
`solvecraft-unraid:rollback-before-72145d4a` retain the prior working image. Idle-process
and unchanged-config checks preceded replacement. The data volume, MCP token, complete
environment, user, ports/origins and resource/security settings were preserved. Authenticated
SDK modelling/PNG, read-only reopen of the known public 24000 mm3 example, dashboard cookie
login/API/logout and the actual browser CAD passed after replacement. No real saved file
was overwritten by the proof. The owned idle candidate container and its now-unused prototype
image were removed only after fresh label/image/mount/port/process checks. Build/browser
check containers removed themselves. No volumes were deleted or global prune used;
toolchain images, build caches, test volumes, public fixtures, archives and private recovery
data remain available.

The canonical overview-only template change was synchronized to `denara/unraid-templates`
main as `69d444ad6b51a3b292e28d03ab5c5223df27a2b3`, after combined-image publication passed.
Anonymous retrieval of its public XML returned HTTP 200 and matched the canonical template.
It lists only the additional Unraid/MCP/workspace/file-protection/logging integration features
as bullets; original SolidCraft features and changes have an upstream link. Original authorship
and unofficial-distribution/trademark notices remain explicit. Runtime fields, the original
icon, catalog profile and licence are unchanged. No new portal submission or terms acceptance
was needed; catalog ingestion/cache refresh is separate from publishing this repository.

The final fetch observed one later upstream commit, `f7343e91` (PR1860, CI work-sharing only).
The selected snapshot remains fixed; that later change is left for the next update check.
No Unraid installation, nginx/DNS modification, application version bump or release tag was made.
To install on Unraid, save active work, preserve backups, update the existing container with
the same data/token/settings and reload the browser. The catalog description is available
through the refreshed Community Applications metadata.

## 2026-10-17: workspace login diagnostics

The validated upstream baseline stays `72145d4aba05eab66e7a49b383bee9c9f5764da3`.
This focused gateway correction preserves the published CAD and independent browser app.
The reported 403 came from opening Unraid's direct HTTP-IP WebUI link instead of the
configured HTTPS proxy address. Native MCP authentication remained functional. Private
deployment values and the persistent token are unchanged and are not published.

Workspace form rejections now report fixed, credential-free audit reasons and show a
German explanation with a reload link, including Origin/Host, cookie, expired/mismatched
CSRF, malformed form, rate and browser-capacity rejection. A still-valid login challenge
is reused across tabs without extending its expiry; successful submissions remain one-use.
Strict Origin/Host checks, HTTPS cookies, browser/MCP authentication separation and file
confinement are preserved. The deployment guide explains the local Unraid WebUI override.

All 47 gateway tests, gateway all-target Clippy with warnings denied, formatting and the
release build passed. An isolated candidate with fresh volume and disposable credential
passed actual MCP modelling/PNG/STEP, authentication, session separation, path checks,
restart persistence, category-color/privacy logs and the workspace/file-revision proofs.
The workspace proof now also covers a second login tab and actionable, private 403 pages.
Actual container logs contained the new rejection reasons without the test credential.
The real browser proved successful login from the first tab after opening a second one,
the explanation for a consumed form, and recovery using its reload link. Public-safe
evidence is under ignored `target/workspace-login-fix`, outside the container build context.
No Unraid installation or proxy change was performed. Publication and local refresh are
recorded after the exact-source container workflow passes.

Published source `25f3e2286b1bdeb4d44a5dce52f8466eabf85bd7` passed
[container workflow 39052067052](https://github.com/denara/solidcraft-unraid/actions/runs/39052067052),
including its actual-image modelling, persistence, authentication, file/origin, browser,
logging and extended workspace checks. Public tags are `ghcr.io/denara/solidcraft-unraid:latest`
and `sha-25f3e2286b1bdeb4d44a5dce52f8466eabf85bd7`; both pulled anonymously and the manifest
matched source, architecture and digest
`sha256:f25e7bbaf0c61bab22fdc093562bde75732ffa3b5eb1f5731ac4563052e49db9`.

Verified archive: `target/solidcraft-unraid-workspace-login-25f3e228.tar` (148,043,264 bytes),
tag `solvecraft-unraid:workspace-login-fix-25f3e228`;
SHA256 `79c9aba0573ce9a587760b9b7b6631d0d8ee009b476c86006152159361608d1b`.

The sole local runtime `solvecraft-solvecraft-1` now uses that immutable public image.
Fresh private backups and `solvecraft-unraid:rollback-before-workspace-login-fix` retain
the prior state. Idle/ownership checks preceded replacement; data volume, token, complete
environment, ports/origins, user and resource/security settings were verified unchanged.
Authenticated SDK modelling/PNG, read-only reopen of the known public 24000 mm3 fixture,
workspace cookie login/API/logout, health and actual browser CAD all passed after replacement.
The owned idle candidate and its unused prototype image were removed after those checks,
with diagnostics retained under ignored target. No volumes, toolchain images, build caches,
rollback image or backups were deleted. Catalog metadata is unchanged, so no catalog commit
was needed. No Unraid deployment, application version bump or release tag was made.
The login-address correction works with the prior image; update the installed container
when the improved rejection logs and explanations are desired.

## 2026-10-18: upstream 8f768b65 with all published integration features

Selected once after fetching: `8f768b65c801b255e48fd5892b3c50c136fa0992`
([merged PR1907](https://github.com/bherbruck/solidcraft/pull/1907)), advancing the
validated upstream baseline from `72145d4aba05eab66e7a49b383bee9c9f5764da3`.
The sole integration/publication branch remains `codex/unraid-mcp-web`, synchronized
with origin. Fork main was checked separately and is already an ancestor. The starting
published integration includes the `25f3e228` workspace-login diagnostics correction.
No additional branch, rewritten history, fork application release or release tag is made.
The upstream author's existing 0.15.0 version is retained.

The selected snapshot includes boundary loops/networks, grouped-profile/face extrusion,
rib direction and boolean/STEP corrections, constrained sketch dragging, dimensions and
loose-end tools, viewport picking/grid and toolbar/timeline changes, translations and the
web service-worker sign-in correction. Upstream's separate web-only Docker packaging is
retained alongside our combined gateway/web packaging; it does not replace the latter.
For product features and changes, the canonical Unraid Overview continues to link upstream
and lists only this fork's additional integration features, with original authorship and
unofficial status explicit. The template and original icon did not change in this update.

Resolved three conflicts: agent instructions preserve the fork's safety/privacy/update
rules, the upstream original `project` icon supersedes our equivalent silhouette assignment,
and the app keeps unsafe native crash/frame/Finder bridges omitted while retaining the
shared Rust HID changes. The new unsafe macOS 3Dconnexion framework bridge is likewise
omitted; its API returns Unsupported without invoking a callback or starting a thread.
A targeted regression and workspace forbid-unsafe lints cover this compatibility stub.
Linux/container CAD and input behavior are preserved. Licences were regenerated from the
merged dependency graph, including the gateway, without adding external dependencies.

All previously published gateway features remain: independent browser CAD and MCP models,
cookie-only workspace login (including actionable 403/audit reasons and cross-tab challenge
reuse), authoritative per-session file/dirty metadata, bounded slots, cookie/Bearer separation,
confined files, stale-save and HTTP revision checks, category colors and token-location logs.
Review of the new/changed commands found no new filesystem writer: sketch loose-end commands
read/mutate sketches; boundary surfaces and grouped-face extrusion mutate the document;
occurrence paths refer to model occurrences. Existing multi-output export guards remain.

Validation was scoped to the changed code:

- 457 selected Rust tests passed across geometry/network/section validation, sketch motion
  and ends, format/display/feature walks, boundary surfaces, STEP/trimmed projections,
  revolve/sweep and multi-profile/face/rib regressions, viewport/device navigation, dimensions,
  icons/localization, shared input, MCP, CLI and all 47 gateway tests. Existing ignored cases
  remain ignored. Unrelated whole-suite tests were not repeated.
- Workspace all-target Clippy with warnings denied, formatting, 377 attributed assets,
  original SVG icon checks, sorted catalogs, 18-crate layers, shipped licences and Wasm gates
  passed. Linux amd64 CLI/gateway release and the independent release browser bundle built.
- A fresh-volume, loopback-only candidate with disposable token passed real HTTP MCP
  modelling at 24000 mm3, PNG/STEP, independent sessions, authentication/origin/path checks,
  restart persistence, operational color/log privacy and the extended workspace test.
  Actual browser login, file filtering and CAD rendering at 0.15.0 passed.
- The official SDK reopened nine public older box designs at 24000 mm3, including the
  previous format26 with five pending markers, and the previous guide-rail model at
  7329.7796 mm3. Native and 3MF ZIP round trips passed. Loose ends/ties and a boundary-loop
  slab saved/reopened at 200 mm3. A new boundary network saved/reopened at 128.1018 mm2
  (within 0.1 percent of independent surface integration) and produced a real native PNG.
  The individual new public analytic oracle case83 also passed through the actual CLI.
- The built browser command API passed its actual model/undo/redo/error/queue proof.
  All 20 offline/update scenarios (22 launches) passed, including the new expired-sign-in
  scenario: it displays the sign-in page instead of serving a retained build past the gate.
  Screenshots, public models and logs stay under ignored `target/unraid-update-8f768b65`.
  Docker cp refuses staging into a read-only rootfs even at this tmpfs path; the public
  oracle JSONs were instead streamed into the writable tmpfs, preserving the security flag.

This snapshot writes `solidcraft/27`, with `boundary-surface-networks` and
`rib-normal-direction` pending. The previous five markers are folded into v26 by upstream.
Older saved files were verified in the new reader; previous builds cannot read new format27
files. Save active work and keep pre-update server/browser-design backups and the prior image
before a production update. No Unraid installation or nginx/DNS change is performed here.
Publication, archive checksum and the preserved local runtime/cleanup are recorded after the
exact-source container workflow passes. No catalog commit is needed for unchanged metadata.

Publication passed [container workflow 39108655415](https://github.com/denara/solidcraft-unraid/actions/runs/39108655415)
from validated merge/source `135d04e65f12702ac82c8d5f8b35b43b4f4d0ed2`.
The fresh CI image passed real HTTP MCP modelling/PNG/STEP, independent sessions,
authentication/origin/path guards, restart persistence, browser assets, operational log
privacy/colors and the extended workspace/revision proofs before registry push.
Public Linux amd64 tags: `ghcr.io/denara/solidcraft-unraid:latest` and immutable
`sha-135d04e65f12702ac82c8d5f8b35b43b4f4d0ed2`.
Digest: `sha256:34a81391af66de083e3369c89f74c168fc80d1df9da1c143050cf1a2cd2641f9`.
Both tags pulled anonymously using an empty Docker configuration; the anonymous manifest
matched digest, source revision and architecture.

Archive: `target/solidcraft-unraid-upstream-8f768b65.tar` (149,224,960 bytes), tagged
`solvecraft-unraid:upstream-8f768b65`.
SHA256: `2f8d9c913c7471a096616a49c1d1df7af059d6f3647dd65f8b227359b91a5d91`.

The sole local test runtime `solvecraft-solvecraft-1` now runs that immutable public image,
healthy at 0.15.0. Ownership/idle checks, fresh private data/config backups and
`solvecraft-unraid:rollback-before-8f768b65` preserve the prior state. The original data
volume, MCP token, complete environment, user, port/origin and resource/security settings
were verified unchanged. Authenticated SDK modelling/PNG, read-only reopen of the public
24000 mm3 fixture, workspace cookie login/API/logout and browser startup passed after
replacement. No real saved file was overwritten. The owned idle candidate and its untagged
unused prototype image were removed after ownership/image/volume/activity checks, with
final diagnostics retained. Build, extra-test and browser containers removed themselves.
No volumes were deleted or global prune used; toolchains, caches, backups and rollback
images remain. The canonical catalog template/icon are unchanged, so no new catalog commit
or submission was needed. No Unraid deployment, nginx/DNS change or fork release was made.

The final fetch observed one later upstream commit, `b685b7301f35948ab75015d48ec8b89cfd8252b7`
(horizontal-pan direction preference, PR2047). The chosen and validated snapshot remains
fixed; the later change is left for the next scheduled check. To install, save active work,
back up server and independent browser designs, update the existing Unraid container while
preserving data/token/settings, and reload the browser. Keep pre-update files for rollback:
new format27 designs are not readable by the previous build.

The actual local browser also rendered Sample Plate at 0.15.0 after replacement; its public-safe screenshot is retained in the ignored proof directory.

## 2026-10-18 — live workspace, project groups and saved properties

This integration update retains the validated upstream snapshot `8f768b65` and all
published HTTP MCP, browser CAD, workspace, file-revision and logging behavior.
The independent read-only dashboard now receives cookie-authenticated server-sent
events, with bounded connections and one shared change monitor. The small browser
client is Rust/Wasm; it preserves search input, folder state and dialogs during pushes.
Graceful shutdown closes streams promptly, and valid remembered logins reconnect
without a page reload, including after consecutive container restarts.

Virtual groups use existing parent directories or exact model/export filename stems;
no saved file or MCP path is moved. File details show bounded, whitelisted properties
from saved JSON/native ZIP designs without loading geometry. Original SolidCraft icon
masters and their palette are reused. Browser logins optionally persist for seven days,
30 days or until revoked, using hashed random identifiers in a private mode-0600 store
outside shared files. The MCP token is never retained in browser storage.

Validation on the development computer: 71 gateway tests, scoped native/Wasm clippy
with denied warnings, formatting, asset/icon attribution, layering, sorted catalogs,
shipped licenses and the 13-crate Wasm check. A fresh-volume isolated container passed
real HTTP MCP modelling/PNG/STEP, independent sessions, authenticated workspace and
shared revisions, live file creation/replacement, grouping, saved properties, logout
revocation, remembered-login restart/revocation and operational log privacy/colors.
The existing public 24000 mm3 fixture reopened after restart. The actual browser showed
new saved files and unsaved session state without reloading, preserved typed search and
collapsed groups across two restarts, and rendered the details dialog and independent
CAD sample. The two final isolated restarts and their login checks completed in 2.63 s.
An earlier log-check invocation lacked its documented restart precondition; after the
candidate restart, the complete log check passed. No unrelated geometry suite was rerun.

Private evidence, screenshots and runtime backups are under the ignored
`target/workspace-live-20261018/` directory. The public container workflow now checks
the Rust client assets, live workspace proof and remembered-login restart proof before
registry publication. The canonical Unraid Overview remains a product description:
upstream CAD features are linked, and only the unofficial integration additions are
listed separately. Publication, archive checksum, catalog synchronization and local
runtime replacement are recorded after their final checks. No Unraid installation,
nginx/DNS change, application release or design migration is performed.

The same update was extended before image publication with real PNG thumbnails,
a subtly separate file-explorer surface and **Bearbeiten** links. The intermediate
source `19337a0b` was preserved; its container workflow was intentionally cancelled
before registry publication so the requested additions could ship together.
PNG inputs are fully decoded under file/dimension/memory limits and re-encoded as
small static thumbnails without ancillary metadata. Cookie-bound one-use design
handoffs expire after 120 seconds, enforce ownership and file revision, and hold a
bounded memory permit until the response is consumed or abandoned. No server file is
changed. The original CAD `doc.open` opens JSON/native ZIP designs as independent
browser files with cryptographically unique names, preserving existing browser work.
Companion linked-design files are not silently fetched and browser edits are not
automatically saved back to the server.

Final relevant checks: 85 gateway tests, six browser-import tests, native clippy for
the changed apps, scoped Wasm clippy, formatting, licensing/assets/icons/layers, both
Trunk release builds, actual browser API/queue behavior and the complete 20-case
offline/update regression. The existing unrelated IO Wasm-only clippy warning is
outside the scoped browser lint; no kernel/geometry code was changed. Two redundant
browser install-prompt error conversions were removed without changing behavior.
The isolated candidate passed the extended real MCP/live/file-revision proof with a
rendered PNG preview, one-use native transfer, replay rejection, unchanged saved bytes
and unchanged MCP model. Remembered login/revocation/restarts and operational logging
passed again. The actual browser rendered the thumbnails and new surface, opened the
box model through the clicked edit link, and opened a public native ZIP surface design;
handoff fragments disappeared from the address bar. Public-safe screenshots and all
checks remain under `target/workspace-live-20261018/`.

Publication passed [container workflow 39131938693](https://github.com/denara/solidcraft-unraid/actions/runs/39131938693)
from validated source `4bb74c6bd236a595e6bf9cc10314363151c69297` on
`codex/unraid-mcp-web`. The fresh CI image passed real MCP modelling/PNG/STEP,
restart persistence, browser assets, logging/privacy, concurrent file revisions,
live workspace/preview/import checks and remembered-login restart/revocation before
publication. Public Linux amd64 tags are `ghcr.io/denara/solidcraft-unraid:latest`
and immutable `sha-4bb74c6bd236a595e6bf9cc10314363151c69297`.
Digest: `sha256:234fdd6cc3f68ce86f67e9d53ca32f0a445fa15ee5b6ca6e4d84cf8e54eb5a91`.
Both tags pulled anonymously with an empty Docker configuration; the anonymous
registry manifest matched the source, architecture and local image digest.

Archive: `target/solidcraft-unraid-workspace-live-20261018.tar` (150,417,408 bytes),
tagged `solvecraft-unraid:workspace-live-20261018`.
SHA256: `de40ea9b4c8a857ed7a39bec41f475c20d1a132c042641fa52275ed0554d3355`.

The sole remaining local test container `solvecraft-solvecraft-1` runs the immutable
public image, healthy at 0.15.0. Fresh private data/config backups and the prior
image `solvecraft-unraid:rollback-before-workspace-live-20261018` are retained.
The data volume, MCP token, complete runtime environment, user, port/origin settings
and resource/security limits were verified unchanged. Authenticated MCP modelling
and PNG rendering, read-only reopening of the public 24000 mm3 fixture, workspace
cookie login/logout, saved properties, the SSE endpoint, Rust browser assets and
independent CAD startup passed after replacement. No real saved file was overwritten.
The owned idle candidate and its three unused prototype images were removed after
label, image, volume and activity checks; diagnostics remain in the ignored proof
directory. No volumes or unrelated Docker resources were removed. Build caches,
toolchain images, backups and rollback images remain; no global prune was used.

The canonical seven-bullet Overview was synchronized to `denara/unraid-templates`
on `main` in commit `4cac094`, preserving the original icon, catalog profile,
license and other template settings. No new CA submission or application release
was created. The final fetch observed upstream/main at
`2b8e85a7c60b3cff0ef694d26a12ccadb304d345`; the validated CAD baseline remains
`8f768b65c801b255e48fd5892b3c50c136fa0992`. Later upstream work is deferred to a
separate validated update. Fork `origin/main` remains an ancestor of this branch.
To install, save active work, update the existing Unraid container while retaining
its data/token/settings and reload the browser. Browser editing opens an independent
copy and does not save changes back to the server. No Unraid deployment, nginx/DNS
change or file migration was performed.

## 2026-10-19 — upstream snapshot 5122b98a, SolidCraft 0.16.2

Merged the fixed snapshot `5122b98af3fc5fe287c2e6d4beee8587a503244b` directly into
`codex/unraid-mcp-web`, advancing the validated CAD baseline from `8f768b65`.
The selected upstream range contains 90 commits; upstream CAD features and changes
are described by the [original project](https://github.com/bherbruck/solidcraft).
The tree began clean and matched the published integration branch. Fork main is
already an ancestor. Published merge history and all HTTP MCP/workspace extensions
are preserved. Nextest scheduling, private/local ignores and fork agent instructions
were retained when resolving conflicts; shipped notices were regenerated with the
HTTP gateway and Rust workspace client included. New image codec dependencies are
permissively licensed; unrelated dependency upgrades were not introduced.

Two integration corrections were required. A boolean simplification leaves the new
agent-chat paste behavior unchanged while satisfying clippy. Upstream's new ambient
occlusion prevented even an unoccluded viewport from compiling on OpenGL/WebGL:
GLSL cannot load a depth texel directly. A failing pure translation regression and
actual GPU captures reproduced this. The prepass now keeps a bounded RGBA8 copy of
24-bit depth for portable color-texture loads, while depth testing retains its
original precision and direction. All seven actual OpenGL capture cases passed
with the correction, and the real browser rendered Sample Plate with contact
shading enabled. Geometry/kernel behavior was not patched.

File-writing review found no new output commands: the added joint-edit/view commands
are memory operations. Matrix placement on STEP insertion retains its existing
confined input path. Save As warm-cache changes write inside the same native file,
without new sibling outputs. Existing multi-output export restrictions, authoritative
session status and shared HTTP/MCP revision checks remain in place. The protected
live dashboard, remembered login, project groups, metadata, PNG previews and expiring
one-use browser-copy handoffs are retained; browser editing remains independent.

Relevant local checks passed: workspace-wide native clippy, formatting, 385 attributed
assets, 268 original icon masters, layers, sorted catalogs, shipped notices and the
13-crate Wasm build. Tests include 85 gateway cases, 49 MCP cases, six browser-import
cases, the GLSL regression, seven GPU capture cases, document-format/joint migration,
14 3MF cases, changed geometry helpers/kernel cases, 25 occurrence/matrix cases,
102 joint cases, 14 browser-visibility cases, warm Save As caches and changed browser
UI/attachment/paste/theme/navigation behavior. Focused test binaries were reused for
extra joint/visibility checks while the browser release built. No unrelated full
geometry suite was run. The actual browser API/queue proof and all 20 offline/update
cases passed. Initial lint/render failures and final passing logs remain under the
ignored `target/unraid-update-5122b98a/` directory.

A fresh-volume candidate on a free loopback port passed actual HTTP MCP sketch/
extrude modelling at 24000 mm3, PNG/STEP, independent sessions, authentication,
origin/path denial, persistence after restart, logging/privacy/colors, workspace
revision conflicts, live file events, saved properties, bounded PNG previews and
cookie/Bearer separation. Remembered login and durable logout revocation passed
across two isolated restarts. The official SDK reopened three public older designs,
including format27 pending boundary surfaces, verified 24000 mm3 and 200 mm3 solids
and a 128.1018 mm2 surface, and round-tripped native/3MF files. A rigid matrix placement
reopened at the independently expected center [9.5,21,30.5] mm. The actual browser
showed live workspace/model properties at format28 and opened the migrated native
file as an independent CAD copy, consuming its one-use fragment.

New native saves use `solidcraft/28` with `joint-expressions`, `joint-origin-edge`
and `joint-origin-frame-x` pending. Previous builds cannot read format28 files.
Before an Unraid update, save active work and keep pre-update backups of both server
files and independent browser designs, along with the previous image. No Unraid
installation, nginx/DNS change, application release or file migration is performed
by this update task. Publication, archive checksum and local replacement are recorded
after the exact-source container workflow and anonymous download checks pass. The
canonical Unraid product Overview is unchanged, so no catalog commit is needed.
Publication passed [container workflow 39218638514](https://github.com/denara/solidcraft-unraid/actions/runs/39218638514)
from validated merge/source `36447e46d0be72c955783400f7758b59d9cb2f67`.
The fresh CI image passed real MCP modelling/PNG/STEP, independent sessions,
authentication/origin/path guards, restart persistence, browser assets, operational
logging/privacy, shared revisions, live workspace/preview/browser-copy checks and
remembered-login restart/revocation before registry push. Public Linux amd64 tags:
`ghcr.io/denara/solidcraft-unraid:latest` and immutable
`sha-36447e46d0be72c955783400f7758b59d9cb2f67`.
Digest: `sha256:083abb7e0aa0dcb1b1a81ccfca7556398316dee9f53b7722832aabad89506787`.
Both tags pulled anonymously using an empty Docker configuration; the anonymous
registry manifest matched source, architecture and digest.

Archive: `target/solidcraft-unraid-upstream-5122b98a.tar` (151,325,184 bytes),
tagged `solvecraft-unraid:upstream-5122b98a`.
SHA256: `060307a605d33d01929146ad1e9700d60d4dc79e0dfd1a22a677bc1e47755c3a`.

The sole remaining local test runtime `solvecraft-solvecraft-1` is healthy at 0.16.2
on the immutable public image. Fresh private data/config backups and
`solvecraft-unraid:rollback-before-5122b98a` retain its former passing image/state.
Its data volume, MCP token, complete environment, user, port/origin settings and
resource/security limits were verified unchanged. Authenticated SDK modelling/PNG,
read-only reopening of the known public 24000 mm3 fixture, workspace browser-cookie
login/logout, saved properties, SSE, Rust client assets and independent browser
startup passed after replacement. No real saved file was overwritten. The owned
idle candidate and its unused prototype image were removed after label, image,
volume and activity checks, with diagnostics retained in the ignored task directory.
Build and browser-test containers removed themselves. No volumes or unrelated
resources were removed; toolchains, build caches, private backups and rollback
images remain. No global prune was used.

After publication, the canonical XML/icon still exactly matched the public catalog
at `denara/unraid-templates` main `4cac0942acd84241bc659c2b8eecac7f85f6eec1`.
The profile and license are unchanged; no new catalog commit or submission was
needed. The final fetch observed one later upstream commit,
`5997d28de73eb7bfc746652dcd4af90f7a5805a7` (STEP instances reuse orientation,
PR2875). It remains for the next fixed-snapshot update. The selected and validated
baseline stays `5122b98af3fc5fe287c2e6d4beee8587a503244b`.
To install on Unraid, save active work, back up server files and independent browser
designs, update the existing container while preserving its data/token/settings,
and reload the browser. Keep pre-update designs for rollback: new format28 files
cannot be read by the previous build. No Unraid deployment or nginx/DNS change was
performed; browser-copy edits still do not write back to server files automatically.
