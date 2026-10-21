# SolidCraft on Unraid: independent web app and MCP CAD service

Unraid® is a registered trademark of Lime Technology, Inc. This application is not affiliated with, endorsed, or sponsored by Lime Technology, Inc.

This fork adds deployment packaging and an HTTP adapter. The modelling engine and the
existing stdio MCP server retain their behaviour. The first target is Linux x86_64 (AMD/Intel),
tested locally using Docker Desktop Linux containers, then deployable on Unraid.
Upstream renamed the displayed product from SolveCraft to SolidCraft. Technical names
use upstream compatibility shims: `solvecraft-cli` remains a CLI alias and existing
`.solvecraft` files still open. The gateway binary, container environment variables,
persistent volume paths and image names retain their existing identifiers. New documents
use upstream's current container/format marker and cannot be opened by older builds. Hostnames and deployment paths below are examples: substitute your own
values locally, outside Git. `.env` and `target/` are ignored by Git and Docker builds.
The fork is hosted at [denara/solidcraft-unraid](https://github.com/denara/solidcraft-unraid).
The public Unraid template names a new container `SolidCraft`; the existing local Compose service,
image tags, command names, data format and environment variables retain compatibility.
The first prototype used upstream commit `af536a7`. The current validated snapshot is
`e9b1a077d7e33420791791c651f2e293962a2da1`, packaged as
`solvecraft-unraid:workspace-actions-e75ca796`. See [update records](unraid-updates.md) for checks,
the transferable image archive and installation instructions for this update.

For future requested updates, use the repository skill
[update-solvecraft-unraid](../.agents/skills/update-solvecraft-unraid/SKILL.md).
Keep this repository as the primary local project folder so new chats discover root
`AGENTS.md` and `.agents/skills/`. Invoke it with `$update-solvecraft-unraid`, or request
an upstream update in natural language. The skill checks changes, preserves the deployment
addition, validates an isolated candidate and prepares the image for installation.
It does not schedule monitoring or automatically replace the running Unraid container.
Manual and scheduled updates both use the single working branch `codex/unraid-mcp-web`.
Upstream snapshots are merged into it; no separate update branches are created.
After successful validation, keep one local test container on the latest validated image
and remove obsolete project build/check containers. Select the local image with
`SOLVECRAFT_IMAGE` in ignored `.env`; preserve the existing data volume and MCP token.
Useful build images and caches, saved designs and a rollback image are retained.
This local cleanup does not update Unraid or remove unrelated Docker resources.

## Registry installation and Community Applications

The public distribution uses `ghcr.io/denara/solidcraft-unraid:latest`. GitHub's container
workflow builds Linux amd64 on pushes affecting container code on `codex/unraid-mcp-web`,
then verifies modelling, PNG/STEP, isolation, access/path/origin checks, restart persistence
and web assets before pushing. `latest` is the newest passing container build, not untested
upstream main or a guarantee that every CAD operation is supported. This is experimental
community packaging, not an official upstream release. The CAD application and browser app
are developed by bherbruck and the upstream SolidCraft contributors; denara maintains this
Unraid integration, HTTP MCP adapter and packaging. `sha-<full-source-commit>` tags
identify earlier builds for controlled rollback.

The package must be set to **Public** in GitHub Packages before anonymous Unraid pulls work;
new GitHub packages default to private. Test an anonymous pull before announcing availability.
No GitHub token should be put into a public Unraid template.

The dedicated public catalog repository is
[denara/unraid-templates](https://github.com/denara/unraid-templates), with root `LICENSE`,
`ca_profile.xml`, `templates/solidcraft.xml` and an original, attributed icon. Catalog inclusion
requires validation, scanning and acceptance through [the CA submission portal](https://ca.unraid.net/submit).
Creating the repository does not itself list the application in Community Applications.
This repository's submission was automatically approved on 2026-10-12 as
**denara's Repository**. The portal says SolidCraft should appear after the next
Community Applications build publishes; approval alone does not establish feed visibility.

Once the image is anonymously accessible, use the supplied template and retain its registry
image reference to install directly, without loading a tar archive. Before public catalog
acceptance, the same XML can be placed under
`/boot/config/plugins/community.applications/private/denara/` for local CA use, or under
`/boot/config/plugins/dockerMan/templates-user/` for Docker's Add Container screen.
CA's private templates are local XML files; modern Unraid does not have the old repository-URL
subscription field. An additional helper is not needed for container image updates.

Unraid's normal update action pulls a newer `latest` image and recreates the container with
its saved settings. Keep the existing data mapping, origin settings and MCP token; back up
designs and retain a prior commit-specific image before updating. This workflow does not
restart an Unraid installation automatically.

| Address | Purpose |
| --- | --- |
| `/` | Original browser CAD app; calculations and designs stay in the browser |
| `/workspace` | Protected file overview, confirmed deletion, streamed folder ZIPs and active sessions |
| `/workspace/api` | The same bounded overview as JSON; requires a dashboard login |
| `/mcp` | Bearer-authenticated Streamable HTTP MCP; a separate CAD process per session |
| `/files/NAME` | Authenticated GET download / PUT upload of persistent server files |
| `/health` | Container health check; no authentication required |

Browser CAD remains independent of active MCP sessions. The workspace can open a saved native design as an independent browser copy; browser edits are not automatically saved back to the server. Browser storage is browser-specific
(OPFS, with local-storage fallback); it is not the Unraid data volume. Download browser
designs to back them up. HTTPS is recommended for browser storage and WebGPU support.
MCP previews work headlessly on the CPU; no GPU passthrough or remote desktop is needed.

## Project folders for MCP clients

Every new HTTP MCP session must choose a project before using CAD tools. Discovery,
`project_list`, `workspace_list_files` and `session_status` remain available first.
Call `project_select` with `{"name":"Demo-Bracket"}` to select or create the real
directory `/data/files/Demo-Bracket`. The exact name is fixed for that session;
selecting another project requires a new session. Names are bounded to 96 UTF-8 bytes
and must be one visible directory component, without separators, reserved names,
surrounding spaces or a trailing dot. Existing folders retain their spelling.

CAD paths are relative to the selected project: `save({"path":"part.solidcraft"})`
writes `Demo-Bracket/part.solidcraft`. Downloads, `workspace_list_files` filters and
`expected_revisions` keys remain relative to the shared workspace and include the
project prefix. For example, use `/files/Demo-Bracket/part.solidcraft` for download
and `{"Demo-Bracket/part.solidcraft":"<revision>"}` for a revision precondition.
This distinction preserves the same file identity for HTTP and all MCP sessions.

Existing files are not moved or rewritten. To use an older root file as a source,
explicitly call `open({"path":"old-part.solidcraft","from_workspace":true})` after
selecting a project. Then save to an explicit project-relative destination. An
implicit save back to a source outside the selected project is rejected, including
inside a batch. The same workspace token still grants shared workspace access:
project folders organize work; they are not separate user permission boundaries.
The original desktop/stdio MCP server retains its existing behaviour.

The dashboard groups files by their first real directory component, including nested
exports. Existing root files use virtual groups based on native model names; exports
with additional hyphen/underscore suffixes use the longest unambiguous matching model
stem. Files without a safe match remain in “Weitere Dateien”. Real-folder ZIP/delete actions sit in a compact bar above the file-table headings; the project name is not repeated. Groups start collapsed
on every page load; live updates preserve the user's current open/closed choices.
The file-detail “Projektgruppe” is derived by this integration. The separate “Projekt”
property comes from the design's optional `metadata.project`; it does not select or
rename a server directory.

## Watching a live MCP model

Choose **Live-Vorschau ansehen** on an active MCP session card in `/workspace`. The protected
dialog shows a real image of that session's current model, including unsaved sketches
and solids. Updates arrive automatically after completed commands. While a long command
or batch is running, the previous picture remains visible with a calculation status;
intermediate geometry inside a command or batch is not streamed.

Pictures are calculated in an isolated background snapshot, without changing the
agent's camera, document, undo history, saved path or dirty state. The viewer is
read-only and offers no camera controls or model commands. The original browser CAD
app and its saved-design copies remain independent.

Observation starts only while an authenticated browser watches a session. Viewers
share the latest bounded image; no preview PNG or design is written to server files.
Rendering is limited globally, rate-limited per session and bounded in memory and
resolution. Closing the dialog stops its subscription. A slow, oversized or failed
preview reports its own unavailable state while MCP modelling remains usable.

The events and images use the existing workspace browser cookie, not the MCP Bearer
token. Logout or login expiry revokes access; ending a CAD session invalidates its
preview and cannot silently attach the viewer to a replacement session. CPU-rendered
pictures identify their renderer and limitations. These are model snapshots rather
than a desktop-window stream. The [viewer architecture](workspace-live-view-research.md)
also describes the separate, future interactive 3D stage.

## Build and run locally

Start Docker Desktop with Linux containers. From this repository:

```sh
docker compose up -d --build
docker compose ps
```

Open `http://localhost:8781/`. The container generates a random MCP token on first start,
stores it in `/data/mcp-token` and keeps it across restarts. Retrieve it privately:

```sh
docker compose exec solvecraft cat /data/mcp-token
```

Configure your MCP client with Streamable HTTP URL `http://localhost:8781/mcp` and
header `Authorization: Bearer YOUR_TOKEN`. Keep credentials out of repository files.
Clients support protocol revisions 2025-06-18 and 2025-03-26. The server responds with
JSON, not SSE; GET `/mcp` returns 405 as permitted by Streamable HTTP. HTTP sessions
use `Mcp-Session-Id`; the client must retain that header between calls and may DELETE
the session when finished. Clients speaking only stdio can still use the original CLI.

For a local Codex/Work client, add the MCP server through Settings → MCP Servers using
Streamable HTTP and Bearer authentication (availability depends on the client surface).
Alternatively, a Codex configuration can use:

```toml
[mcp_servers.solvecraft]
url = "http://localhost:8781/mcp"
bearer_token_env_var = "SOLVECRAFT_CLIENT_TOKEN"
tool_timeout_sec = 330
```

Set `SOLVECRAFT_CLIENT_TOKEN` in the client's environment to the generated token. For
other clients, use their equivalent URL/header fields. A second chat can exercise the
tools only after the server is connected to that client's MCP configuration; creating
a chat by itself does not register the service.

## Models and files

All clients sharing the token are trusted users of one file workspace. Their **in-memory
models are separate**, but explicitly saved designs and exports in `/data/files` are shared.
Use distinctive filenames when working concurrently.

Use `save {"path":"bracket.solvecraft"}`, `open {"path":"bracket.solvecraft"}` and
`export {"path":"bracket.step"}`. File parameters are relative to `/data/files`; absolute
paths outside it, parent traversal and symlink paths are rejected. Array-based geometry
and occurrence paths are unchanged. Desktop recovery commands are unavailable through
the network adapter; use explicit open/save. Subdirectories must already exist.

MCP `screenshot` returns a PNG content block; `{"path":"bracket.png"}` also retains it.
Download an export through authenticated GET `/files/bracket.step`, or copy it from the
Unraid share. Upload a design/STEP file by PUT to `/files/NAME`, then use MCP `open`.
Uploads replace files atomically. Uploads and MCP requests are capped at 16 MiB;
downloads at 256 MiB. Larger imports can be placed directly into the trusted data share.
Replacing an existing file by HTTP PUT requires its current strong revision in
`If-Match`: a missing precondition returns **428**, and a stale revision returns **412**.
Successful PUT returns the new `ETag`. New filenames do not require `If-Match`;
`If-None-Match: *` rejects a name that already exists.

Save before closing a session. Unsaved designs are not persisted: idle sessions expire
after 30 minutes, and container restarts end all sessions. Start a new MCP session and
`open` the saved file after a restart. A volume retains saved files, not live RAM state.

## File and session dashboard

Open `/workspace` on the same service and sign in with the existing MCP token. Its
location is reported as `token.location` in the container log. Authentication creates
a separate server-side browser session with a default one-hour lifetime and a random
`HttpOnly`, `SameSite=Strict` cookie scoped to `/workspace`. The token is not stored in
that cookie, a URL or application browser storage. The login offers **7 days**, **30 days**
and **permanent** in addition to the default. Remembered sessions survive container
restarts: only hashed random cookie identifiers, CSRF values and expiry are retained in
the private `/data/workspace-sessions.json` store (mode 0600), outside the shared files.
Logout revokes the session, including its other open tabs; changing the MCP token
invalidates remembered logins. Ordinary logins still end on restart. Permanent means
until logout, token change or browser-cookie removal; browsers impose a maximum cookie
lifetime (400 days here), renewed when the dashboard is visited. A dashboard login never
authenticates `/mcp` or `/files` requests. Remember login only on a trusted device.

Use HTTPS through the configured origin. Login and logout require both a CSRF value
and an explicit `Origin` matching `SOLVECRAFT_ALLOWED_ORIGINS` and the request's `Host`
(including any port). Preserve the public Host and Origin through the reverse proxy.
Cookies use `Secure`, except for an explicitly allowed HTTP loopback origin during
local development. Merely reaching the container over a LAN HTTP address is insufficient
for this exception. Failed logins are rate-limited. Dashboard responses are not cached;
file contents and names require authentication.

The page lists saved models, exports and other files, with size, UTC modification time
and download links. Search by filename, select a file type and choose **Anzeigen**.
Updates arrive through a cookie-authenticated **server-sent events** connection at
`/workspace/events`; the browser does not poll or reload the page. One shared server
monitor checks for file/session changes once per second while viewers are connected,
and sends only meaningful changes. Connection state and automatic reconnection are
visible; collapsed groups, keyboard focus, filters and open details dialogs survive
updates. The manual refresh and standalone details pages remain usable without the
optional Rust/Wasm browser client. Streams are limited to four per login and 32 overall.
The reverse proxy must preserve streaming (no response buffering); the endpoint sends
`X-Accel-Buffering: no` and a keepalive every 15 seconds.
Filters affect displayed file counts, while the session overview remains global.
Downloads under `/workspace/files/<relative-name>` use the dashboard cookie and do
not reveal the Bearer token to the browser.

**Virtual project groups** leave all files and MCP paths unchanged. Existing parent
directories take precedence; in the root, an exact case-sensitive design/export stem
groups `bracket.solidcraft`, `bracket.step` and `bracket.png`. Unrelated names are not
guessed into a project. MCP clients choose filenames freely: there is no global project
naming convention. For reliable grouping, reuse a stem or save to a shared relative
directory. No file migration or design rewrite is performed.

**Details** opens a popup with saved `.solidcraft` properties: title, author, project,
part number, revision, description, units, format and model counts where present.
File size and filesystem timestamps are shown separately. Missing properties are not
invented; unsupported/corrupt/oversized metadata gets an explanatory fallback and the
file remains downloadable. Metadata is read on demand with bounded ZIP/JSON parsing,
without loading geometry or exposing arbitrary design fields. Unsaved session changes
are separate from these saved properties. Original upstream file/folder icons are reused.

PNG rows use a small authenticated image thumbnail, with the original icon as fallback.
The server validates static PNG data (up to 8 MiB and 4096 pixels per side) and generates
a fresh thumbnail no larger than 160 × 120; embedded image metadata is not forwarded.
Other file types retain their original icons. The file explorer uses a subtly distinct
surface from the status and information cards.

**Bearbeiten** opens a saved native design in a new CAD browser tab as an independent
browser copy. A short-lived, one-use handoff is bound to the workspace login; it contains
no MCP token and is removed from the address bar before loading the model. The copy gets
a cryptographically unique local filename, preserving existing browser files and tabs.
Both JSON and native ZIP designs use the original `doc.open` path. Changes and saves in
that tab stay in browser storage: there is no automatic save back to the server, and
running MCP models are unaffected. The source file must remain unchanged during handoff;
otherwise open it again. Links expire after two minutes. Files larger than 256 MiB and
full browser tab capacity are rejected. Companion files of linked assemblies are not
silently fetched; missing dependencies are reported by the original CAD importer.

Sessions show an independent display ID, their current file, confirmed unsaved changes,
the last request and whether a request is running. The display ID is not a usable MCP
session credential. An open file is not locked: multiple sessions can load it into
separate models. Unknown change state is explicitly shown as unknown. New unsaved
models are counted separately from saved files and disappear when their session ends.
The browser CAD app at `/` remains independent; download a saved model and import it
there to inspect it. The dashboard cannot edit files or forcibly end MCP sessions.

The inventory is bounded to 512 files, 4,096 visited directory entries and eight
directory levels; files larger than 256 MiB are excluded. A `truncated` indicator warns
that an inventory is incomplete. Symlinks, hidden names, `.bak` backups and temporary
names (`.tmp`, `.part`, `.upload`, trailing `~`) are excluded and reserved from gateway
file access. Keep private backups outside the public workspace.

### MCP file status and save conflicts

The HTTP gateway adds two read-only tools to its advertised catalog:

- `workspace_list_files {"offset":0,"limit":50}` lists the shared saved files; follow
  `next_offset` when present. Limits range from 1 to 100 within the bounded inventory.
  `workspace_list_files {"path":"bracket.solidcraft"}` requests one visible file and
  its strong content `revision`; a path outside the bounded inventory is unavailable.
- `session_status {}` reports the calling session's current file, dirty state, loaded
  revision, model availability and request state. It does not expose another session's
  in-memory model.

`save`, `export`, `screenshot`, `execute` and `batch` accept an optional
`expected_revisions` object mapping each relative output filename to its revision from
`workspace_list_files`. Pass revision strings unchanged, including their embedded ETag
quotes. Replacing an existing file requires a matching supplied revision or the calling
session's loaded revision for its current file. A conflict refuses the write and keeps
the session model; reopen the newer file or save under a different name. New filenames
remain available without a revision. This applies to command-based writes inside
`execute` and `batch`, as well as the direct tools.

Managed HTTP and MCP writes are serialized with their revision checks. This is not an
operating-system file lock: programs writing directly to the data share can still race
with the gateway. Avoid such concurrent external writes. A failed batch may roll back
its in-memory document but does not undo files already written by earlier steps.

Until every output target can be checked, the network gateway declines multi-file URDF
exports, per-body DXF exports to files and PNG animation frame directories. Use an
ordinary export or GIF animation instead; per-body DXF returned as text without a file
path remains available. These restrictions concern the gateway, not desktop CAD.

## Install the locally built image on Unraid

Build on the development computer, not on Unraid:

```sh
docker save -o solidcraft-unraid-workspace-actions-e75ca796.tar solvecraft-unraid:workspace-actions-e75ca796
```

Transfer that archive to Unraid and load it from the Unraid terminal:

```sh
docker load -i /path/to/solidcraft-unraid-workspace-actions-e75ca796.tar
mkdir -p /mnt/user/appdata/your-cad-service
chown 99:100 /mnt/user/appdata/your-cad-service
```

For the locally loaded prototype, you can start it directly in the Unraid terminal:

```sh
docker run -d --name solidcraft --init --restart unless-stopped \
  --user 99:100 -p 8781:8080 \
  -v /mnt/user/appdata/your-cad-service:/data \
  -e SOLVECRAFT_ALLOWED_ORIGINS=https://cad.example.invalid \
  --read-only --tmpfs /tmp:rw,nosuid,nodev,size=256m,mode=1777 \
  --cap-drop ALL --security-opt no-new-privileges:true \
  --memory 4g --cpus 4 --pids-limit 128 \
  solvecraft-unraid:workspace-actions-e75ca796
```

This installs the already built image; it does not compile anything on Unraid. For a
fixed IP on an existing custom network, replace `-p 8781:8080` with
`--network br0 --ip YOUR_RESERVED_LAN_IP` and point nginx at that IP's port 8080.

Install the supplied `packaging/unraid/solvecraft.xml` as a user template (under
`/boot/config/plugins/dockerMan/templates-user/`), then select it in **Docker → Add
Container**. For this offline archive path, change the template's Repository field from
the registry reference to `solvecraft-unraid:workspace-actions-e75ca796`, the locally loaded image.
Offline installations need a new image archive for subsequent updates. For direct registry
installation, keep `ghcr.io/denara/solidcraft-unraid:latest` instead.

Keep your chosen appdata folder mapped to `/data`. Default runtime UID/GID is 99:100,
with a read-only image, writable `/tmp`, no capabilities, a 4 GiB memory limit and four
CPU cores. Increase limits for large parts only after measuring their requirements.

Use bridge networking with host port 8781, or select an existing Unraid custom network
(e.g. `br0`) and assign an unused fixed LAN IP. In custom networking use container port
8080 directly; port mapping is not used. Nginx must be able to reach that IP; depending
on where it runs, Unraid's host-access-to-custom-networks setting may be relevant.

## Existing nginx and HTTPS

Set up DNS for your hostname (e.g. `cad.example.invalid`) to reach your nginx and configure its TLS
certificate. Include `packaging/unraid/nginx.conf.example` in that HTTPS server block
and replace the backend address. Web and MCP share one backend port. Nginx forwards
Authorization and MCP headers; buffering is disabled and its timeout exceeds the
300-second CAD tool timeout. No WebSocket upgrade is needed.

Web: `https://cad.example.invalid/` (replace the hostname).
MCP: `https://cad.example.invalid/mcp` with the Bearer token.
File dashboard: `https://cad.example.invalid/workspace` with its separate browser login.

Unraid's template initially opens the direct HTTP container address from its **WebUI**
menu. For an HTTPS reverse-proxy deployment, edit the installed container in Unraid,
enable **Advanced View**, and set its **WebUI** field to your own
`https://cad.example.invalid/workspace` (or `/` for browser CAD). This is a local
template setting; keep private hostnames out of the public template. Include only the
origin, `https://cad.example.invalid`, in `SOLVECRAFT_ALLOWED_ORIGINS`, without a path
or trailing slash. A direct HTTP-IP login is deliberately rejected: non-loopback
dashboard cookies require HTTPS. Native authenticated MCP can still work because it
usually does not send a browser Origin or use dashboard cookies.

Rejected dashboard forms display a fixed explanation and an **Anmeldung neu laden**
link instead of an empty 403 page. The log records `auth.rejected` with
`endpoint: workspace` and a fixed reason such as `origin_not_allowed`, `host_mismatch`,
`csrf_cookie_missing`, `csrf_mismatch` or `csrf_expired`; submitted tokens, cookies,
hostnames and headers are never included. Expired (ten-minute) or already-used forms
must be reloaded. Opening another login tab reuses the browser's still-valid challenge
without extending its lifetime. Origin, Host and one-use cookie/CSRF checks remain required.

The web page serves static application code; `/mcp` and `/files` require the token.
The adapter checks any supplied Origin against `SOLVECRAFT_ALLOWED_ORIGINS`, defaulting
to the local development origins when unset. Configure your HTTPS origin for deployment.
Native MCP clients commonly omit Origin, which is allowed
after authentication. If using other origins, configure explicit comma-separated values.
The token is a shared local-service credential, not an OAuth implementation or per-user
permission system. Clients that require OAuth cannot use this prototype directly.

Keep internet exposure consistent with your intended local use (e.g. local DNS/VPN and
nginx access controls). The server does not need to be internet-reachable for local clients.
The token file is a credential and should be excluded from publicly shared backups.

## Container logs

Unraid's **Docker → SolidCraft → Logs** view reads the gateway's standard error output;
`docker logs solidcraft` is the equivalent terminal view (use your container's name).
The gateway emits UTC timestamps, severity/category words and icons. Each operational
line includes ANSI color and a reset by default, including when Docker captures a pipe:
`system` cyan, `login` light magenta, `warn` yellow and `error` red, matching the Unraid
log palette. Plain category labels alone did not color the rows in the Unraid viewer.
Set `SOLVECRAFT_LOG_COLOR=never` for plain output or `auto` for terminal detection;
`always` explicitly enables color. A non-empty `NO_COLOR` also disables color unless
`always` is explicitly selected. Viewers must support ANSI SGR to display the colors;
the gateway cannot change viewer settings or guarantee an exact theme shade.

| Category | Event | Meaning |
| --- | --- | --- |
| `system` 🚀 / 🛑 | `server.ready` / `server.stopping` | Started / received a graceful stop signal |
| `system` 🔑 | `token.location` | Every startup reports the token-file path inside the container, normally `/data/mcp-token`, or the environment variable when explicitly configured; never its value |
| `login` ✅ | `auth.accepted` | Token accepted for MCP session admission or a dashboard login; `endpoint` identifies the entry point |
| `warn` ⛔ | `auth.rejected` / `origin.rejected` | Missing/invalid MCP Bearer token, invalid dashboard login token or rejected MCP origin |
| `system` 🔒 | `session.opened slots=1/4` | A separate CAD process occupies one of four slots |
| `system` ⏳ | `session.closing` | Session closed, but an in-flight request still holds its process slot |
| `system` 🔓 | `session.released slots=0/4` | The process slot is actually free; reason includes explicit DELETE, idle expiry, timeout, failure or shutdown |
| `warn` ⚠️ / ⏳ / ⌛ | `session.capacity_reached`, `session.busy`, `cad.timed_out`, `tool.failed` | Capacity exhausted, concurrent request, timeout or rejected CAD/file operation |
| `error` ❌ | `cad.start_failed` / `cad.exchange_failed` | CAD subprocess or its protocol failed |

The `s-...` identifier is generated separately for logs and cannot be used as the HTTP
session ID. Audit events omit tokens, actual `Mcp-Session-Id` headers, client names,
origins/IP addresses, design/export paths, command arguments and model contents. The
trusted token-file location is intentionally reported on every start; no host volume
mapping or token value is printed. Because all
clients share a Bearer token, these are session records, not authenticated user identities.
Normal successful tool requests, initialized notifications, browser requests and health
checks do not generate audit lines. Dashboard login attempts use the same authentication
events with `endpoint: workspace`. MCP Bearer authentication is stateless;
`auth.accepted` is not repeated for each call in an existing session, and does not
guarantee that a session can be allocated when capacity is full.

`packaging/unraid/log-test.ps1` verifies real container log output after a disposable
candidate's restart: login rejection/success, allocation/release/capacity, silence during
normal requests and absence of credentials, raw session IDs, arbitrary client metadata
and unapproved escape sequences. It checks the category palette and per-line reset
in color mode, and escape-free output with `-ColorMode never`. It assumes four test
slots and must not be run on a real deployment.

## Verification

`packaging/unraid/smoke-test.ps1` runs a complete HTTP MCP session: initialize, command
catalog, sketch/extrusion, measurements, PNG preview, save, STEP export and download,
separate-session isolation, authentication, origin and traversal rejection. Its `-Reopen`
mode reopens the saved proof after a container restart. Output images belong outside
Git; the script defaults to the ignored `target/unraid-proof` directory.

Repository gates remain `cargo xtask ci`, including layering, asset attribution and
WebAssembly. Container deployment does not add a CAD command or change engine behavior.

References: [MCP transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports),
[Unraid container configuration](https://docs.unraid.net/unraid-os/using-unraid-to/run-docker-containers/managing-and-customizing-containers/),
[OpenAI MCP configuration](https://learn.chatgpt.com/docs/extend/mcp).

Network clients may upload font files beneath /data/files and name them in sketch.text;
font file paths are confined there, while the built-in Liberation Sans family still works.
Shared desktop crash-report tools and diagnostics.crash commands are unavailable through
the gateway, like desktop recovery, to avoid disclosing another session's diagnostics.

Linked components may read and refresh designs only under /data/files, including links
stored in uploaded documents. The gateway supplies a per-subprocess link root; resolved
paths and parent folders are checked again by the engine, with symlinks refused. Desktop
linked components retain their normal behavior when no network link root is supplied.
This snapshot writes format 27, with pending boundary-surface-network and rib-normal-direction changes. Keep pre-update server and separate browser-design backups; older builds cannot read newly saved designs.

The HTTP gateway launches native MCP with an explicit /data/files scope and --quiet. This keeps upstream file confinement active without logging private per-session paths. Browser build URLs are relative, matching upstream's offline/update support; Web and HTTP MCP remain independent.

The headless MCP backend also provides bounded CPU image jobs through render.preflight/start/status/cancel/export. Their images and captured scenes belong to the MCP session. Library and render export file parameters stay confined to /data/files; the browser continues to use its own independent design.


### Workspace file management

Workspace deletion is an explicit exception to the otherwise observational dashboard.
Choose **Löschen …** on a file or an **Echter Ordner** action, review its exact workspace
path, total file/subfolder counts and bytes, tick the confirmation and choose
**Endgültig löschen**. There is no recycle bin. The cookie-bound confirmation expires
after five minutes. A changed revision, active MCP project/source file, concurrent
write or ZIP transfer prevents deletion; the CAD session is never ended by this action.
Virtual filename groups have no folder deletion or ZIP action. Search filters do not
change the scope of a real folder action. Empty real folders remain visible. The
live inventory caps directories at 256 and 64 KiB of directory names, marking a
truncated view explicitly; folder confirmations and ZIPs inspect the complete target
within their separate manifest limits.

**Als ZIP herunterladen** streams a real folder and its descendants directly from
Rust as ZIP64 with stored entries (no additional compression), safe relative Unicode
names and empty directories. It creates neither a complete in-memory archive nor a
server-side ZIP file. Limits: two simultaneous downloads, 2048 entries, 1 MiB total
path names, 16 GiB uncompressed data and the existing workspace depth/name limits.
Deletion uses the shared strong file revision limit of 256 MiB per file. Hidden,
reserved, symlinked or special entries cause the whole action to be rejected; they
are never silently included or deleted. Data transfer uses 32 KiB chunks and a queue of two chunks, alongside bounded
read/response buffers and ZIP entry metadata. The browser download must
complete successfully before an archive is used. Logout, expiry, disconnect, a stalled
consumer or an external filesystem change interrupts the response. Matching gateway
writes are rejected for the duration of the transfer; unrelated projects remain usable.

Use `packaging/unraid/workspace-actions-test.ps1` only against a labelled disposable
candidate container and fresh test volume. Existing model previews and independent
browser CAD remain read-only with respect to server models.
