# Getting started

The desktop executable is `solidcraft`, the CLI is `solidcraft-cli`, and designs are saved as
`.solidcraft`. Designs, settings and setups from builds released under the previous product name keep
working (see the [compatibility notes](https://github.com/bherbruck/solidcraft/blob/main/docs/compatibility.md)).

## Desktop

Download the installer for your system from the
[Download section of the README](https://github.com/bherbruck/solidcraft#download) or the
[releases page](https://github.com/bherbruck/solidcraft/releases), or build it yourself with
stable Rust (1.95 or newer):

```sh
git clone https://github.com/bherbruck/solidcraft && cd solidcraft
cargo run --release -p solidcraft -- --sample   # opens with a sample part
```

`solidcraft part.step` opens a STEP, IGES, 3MF, STL or OBJ file as a new design, and
`solidcraft design.solidcraft` opens a saved design. Windows builds cross-compile from Linux with
`cargo xwin build --release --target x86_64-pc-windows-msvc -p solidcraft`.

### macOS

Open the release `.dmg` and drag `SolidCraft.app` onto Applications.

When updating from a build released under the previous product name, follow the
[macOS upgrade notes](https://github.com/bherbruck/solidcraft/blob/main/docs/compatibility.md#macos) so you replace it rather than install a second copy.

### Linux Flatpak

Install Flatpak using your distribution's package manager. For an installation that receives
updates, add Flathub for the required Freedesktop runtime and the SolidCraft application feed
to the same user installation:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists solidcraft https://bherbruck.github.io/solidcraft/flatpak/solidcraft.flatpakrepo
flatpak install --user solidcraft ai.storyteller.solidcraft
flatpak run ai.storyteller.solidcraft
```

Releases up to 0.7 used the application ID `ai.storyteller.solvecraft`. If you installed one
of those from the SolidCraft remote, `flatpak update --user` reports it as end-of-life and offers
to replace it with `ai.storyteller.solidcraft`; accept, and your settings and autosaves move with
it. If you installed a `.flatpak` bundle, install the new bundle, launch it once (it copies your
settings), then run `flatpak uninstall --user ai.storyteller.solvecraft` (see the
[compatibility notes](https://github.com/bherbruck/solidcraft/blob/main/docs/compatibility.md#linux-app-id)).

Alternatively, download the release's `.flatpak` file for your architecture and run
`flatpak install --user ./solidcraft-VERSION-linux-ARCH.flatpak`. The bundle supplies the
Flathub runtime repository address; installation still needs internet access to fetch any
missing runtime. Launch with `flatpak run ai.storyteller.solidcraft`, rather than looking for
a host `solidcraft` binary. Files chosen through Open/Save use the desktop file-chooser portal;
command-line paths outside Documents may need a specific additional filesystem permission.

### Linux Snap

On Ubuntu and other distributions with snapd:

```sh
sudo snap install solidcraft
solidcraft                # or open SolidCraft from the app menu
solidcraft.cli --version  # the CLI, for scripts and `solidcraft.cli mcp`
```

The snap can open designs in your home folder (not hidden folders) and, through the file
chooser, anywhere else. For USB drives and other mounted disks, run
`sudo snap connect solidcraft:removable-media` once. A release's `.snap` file installs without
the store with `sudo snap install --dangerous ./solidcraft-VERSION-linux-ARCH.snap`; it then
doesn't update automatically.

## Web

The same app runs in the browser: [open SolidCraft on the web](../app/) (`?sample` opens the sample
part, `?webgl` forces WebGL2 where WebGPU is missing). Designs are saved in the browser's private
file storage, which needs HTTPS or localhost.

After the first visit the web app works offline: the browser keeps a copy of it. Online, each
launch gets the latest published build (the browser downloads it once when it changes); offline,
when the network doesn't answer within a few seconds, or when a new build can't be downloaded
whole, the kept copy starts (and the download carries on for the next launch). To build it
yourself:

```sh
rustup target add wasm32-unknown-unknown
cd apps/solidcraft-web && trunk serve --release --no-autoreload
```

Offline use needs `--no-autoreload`: Trunk's live reload changes the page as it serves it, so it
no longer matches the build the browser would keep. Debug builds (`trunk serve`) never keep a copy.

## CLI

`solidcraft-cli` runs the same commands without a window:

```sh
cargo run --release -p solidcraft-cli -- commands                       # every command, as JSON
cargo run --release -p solidcraft-cli -- run examples/bracket.json --out bracket.step
cargo run --release -p solidcraft-cli -- eval examples/bracket.json     # volume, area, faces…
cargo run --release -p solidcraft-cli -- snapshot examples/bracket.json --out bracket.png
cargo run --release -p solidcraft-cli -- exec solid.box '{"length": 20, "width": 10, "height": 5}'
```

A script is a list of commands:

```json
{"commands": [
  {"command": "sketch.create", "params": {"plane": "XY"}},
  {"command": "sketch.rectangle.two_point", "params": {"p0": [0, 0], "p1": [40, 30]}},
  {"command": "sketch.finish", "params": {}},
  {"command": "solid.extrude", "params": {"distance": 20}},
  {"command": "solid.fillet", "params": {"edges": [[0, 0, 10]], "radius": 3}}
]}
```

![The bracket example, rendered by solidcraft-cli snapshot](../generated/bracket.png)

A command that fails stops the script. A feature can also fail later, when a parameter change
rebuilds it; `run` then exits with status 1, lists the failing features and writes none of
its `--out` and `--save` files, because the bodies lack what those features should have made.
`--allow-errors` writes them anyway.

A design's history turns into a script that rebuilds it, which makes a bug that only shows
on one design a one-file reproduction:

```sh
cargo run --release -p solidcraft-cli -- script part.solidcraft --out repro.json
cargo run --release -p solidcraft-cli -- eval repro.json               # the same part
```

The script starts with the user parameters (`parameters.add`) and the design's units, sheet
metal and plastic rules and materials, then adds each timeline feature with `timeline.add`,
exactly as the design file holds it: same ids, names and parameter names, so later steps
still find the sketches and bodies they use, and a feature that failed in the design fails
the same way. Edit a step's values to try a change, or delete the steps after a feature to
see the design as it was there. Appearances, folders, named views and configurations are
left out (the CLI notes which), and designs with components or joints are not supported
yet. In the app and over MCP the same script comes from `timeline.export_script`.

Units are millimetres and degrees unless an expression says otherwise (`"1 in"`, `"30 deg"`),
and Z is up.

## Name and compatibility

SolidCraft was renamed from an earlier working name. Since 0.5.0 everything uses the new
name: the `solidcraft` and `solidcraft-cli` programs, `.solidcraft` designs, `SOLIDCRAFT_*`
environment variables, settings folders and the `solidcraft` MCP server. Upgrading keeps
working: designs, settings, recovery files, environment variables and CLI/MCP setups from
earlier builds carry over. Every compatibility shim is listed in the
[compatibility notes](https://github.com/bherbruck/solidcraft/blob/main/docs/compatibility.md).

Use the SolidCraft documentation and web-app URLs. GitHub redirects links to the renamed
repository; old-name Pages and Flatpak URLs are not maintained as permanent aliases.
