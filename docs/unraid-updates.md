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
Catalog finalization and acceptance are separate from those completed checks.

The catalog app is **SolidCraft-Unraid**, explicitly an unofficial Unraid integration.
The CAD application and browser app are credited to bherbruck and the upstream SolidCraft
contributors; denara maintains only the container packaging, HTTP MCP adapter and Unraid
template. No upstream application release, version bump or release tag was created.
