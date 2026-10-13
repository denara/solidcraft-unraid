# Validated container updates

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
