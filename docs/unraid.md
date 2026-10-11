# SolidCraft on Unraid: independent web app and MCP CAD service

This fork adds deployment packaging and an HTTP adapter. The modelling engine and the
existing stdio MCP server retain their behaviour. The first target is Linux x86_64 (AMD/Intel),
tested locally using Docker Desktop Linux containers, then deployable on Unraid.
Upstream renamed the displayed product from SolveCraft to SolidCraft. Technical names
(`solvecraft-cli`, `.solvecraft`, environment variables and this fork's image name) stay
compatible. Hostnames and deployment paths below are examples: substitute your own
values locally, outside Git. `.env` and `target/` are ignored by Git and Docker builds.
The first prototype used upstream commit `af536a7`. The current validated snapshot is
`4af2d060073b17d3a6dcea2018ac9490740f8f4e`, packaged as
`solvecraft-unraid:upstream-4af2d06`. See [update records](unraid-updates.md) for checks,
the transferable image archive and installation instructions for this update.

For future requested updates, use the repository skill
[update-solvecraft-unraid](../.agents/skills/update-solvecraft-unraid/SKILL.md).
Keep this repository as the primary local project folder so new chats discover root
`AGENTS.md` and `.agents/skills/`. Invoke it with `$update-solvecraft-unraid`, or request
an upstream update in natural language. The skill checks changes, preserves the deployment
addition, validates an isolated candidate and prepares the image for installation.
It does not schedule monitoring or automatically replace the running Unraid container.

| Address | Purpose |
| --- | --- |
| `/` | Original browser CAD app; calculations and designs stay in the browser |
| `/mcp` | Bearer-authenticated Streamable HTTP MCP; a separate CAD process per session |
| `/files/NAME` | Authenticated GET download / PUT upload of persistent server files |
| `/health` | Container health check; no authentication required |

The web app does **not** display or edit MCP models. Browser storage is browser-specific
(OPFS, with local-storage fallback); it is not the Unraid data volume. Download browser
designs to back them up. HTTPS is recommended for browser storage and WebGPU support.
MCP previews work headlessly on the CPU; no GPU passthrough or remote desktop is needed.

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

Save before closing a session. Unsaved designs are not persisted: idle sessions expire
after 30 minutes, and container restarts end all sessions. Start a new MCP session and
`open` the saved file after a restart. A volume retains saved files, not live RAM state.

## Install the locally built image on Unraid

Build on the development computer, not on Unraid:

```sh
docker save -o solvecraft-unraid-prototype.tar solvecraft-unraid:prototype
```

Transfer that archive to Unraid and load it from the Unraid terminal:

```sh
docker load -i /path/to/solvecraft-unraid-prototype.tar
mkdir -p /mnt/user/appdata/your-cad-service
chown 99:100 /mnt/user/appdata/your-cad-service
```

For the locally loaded prototype, you can start it directly in the Unraid terminal:

```sh
docker run -d --name solvecraft --init --restart unless-stopped \
  --user 99:100 -p 8781:8080 \
  -v /mnt/user/appdata/your-cad-service:/data \
  -e SOLVECRAFT_ALLOWED_ORIGINS=https://cad.example.invalid \
  --read-only --tmpfs /tmp:rw,nosuid,nodev,size=256m,mode=1777 \
  --cap-drop ALL --security-opt no-new-privileges:true \
  --memory 4g --cpus 4 --pids-limit 128 \
  solvecraft-unraid:prototype
```

This installs the already built image; it does not compile anything on Unraid. For a
fixed IP on an existing custom network, replace `-p 8781:8080` with
`--network br0 --ip YOUR_RESERVED_LAN_IP` and point nginx at that IP's port 8080.

Install the supplied `packaging/unraid/solvecraft.xml` as a user template (under
`/boot/config/plugins/dockerMan/templates-user/`), then select it in **Docker → Add
Container**. It references the locally loaded `solvecraft-unraid:prototype` image;
there is no published registry image for this prototype. Do not ask Unraid to update
it from a registry; load a newly built archive for subsequent prototype updates.

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
