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
