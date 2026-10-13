SolidCraft is open-source parametric 3D CAD in Rust: sketch, constrain, extrude, and change a
parameter to watch the timeline rebuild. [Documentation](https://bherbruck.github.io/solidcraft/)
· [try it in the browser](https://bherbruck.github.io/solidcraft/app/) ·
[contributing](https://github.com/bherbruck/solidcraft/blob/main/CONTRIBUTING.md) ·
[issues](https://github.com/bherbruck/solidcraft/issues). AI agents can model with it over MCP:
`solidcraft-cli mcp` ([how](https://bherbruck.github.io/solidcraft/guide/mcp.html)).

**Why 0.7:** this is a minor bump because the design file format moved to version 15 (sketches
on cylinder caps and sheet metal panels need new references), and because it adds new features.

**What's new**
- **Sketching on stacked extrudes works again.** Creating a sketch on the top face of an extrude
  stacked on another failed with "operation failed" since 0.3
  ([#489](https://github.com/bherbruck/solidcraft/issues/489)). It's fixed, and every flat face
  of the test models is now checked for this on every change.
- **Select Other:** hold or right-click to pick something hidden under something else; click
  again to cycle through what's underneath.
- **Tabs are named after their file** instead of "Untitled".
- **Modelling:** a flange angle and height per edge, Patch fills non-planar loops, sketches on
  cylinder caps and on sheet metal faces, and a feature rename updates everything that refers to
  it.
- **Fixes:** revolve with edges on the axis, shell and fillet with small rounds, Join keeps face
  attachments, and STEP export writes each lump of a body as its own solid.
- **Navigation and UI:** orbiting from empty space turns around the visible geometry, Ctrl+drag
  orbits in every preset, toolbar drops land across labels and gaps, Combine previews before OK,
  and sketch constraints accept the sketch's own x / y axes.
- Opening a design reads its metadata and preview through a bounded, seekable reader; the docs
  and web app have link preview cards.

**File format.** Designs are now format 15 (`solidcraft/15`). SolidCraft 0.6 and older can't
open designs saved by 0.7, as a container or as JSON; keep a copy of a file you may still open
in 0.6. Files from every earlier version still open in 0.7.

**Known limitations**
- Some faces still need the fixed-plane fallback for sketches: side faces Join merged from two
  extrudes ([#491](https://github.com/bherbruck/solidcraft/issues/491)), faces split by a later
  feature ([#492](https://github.com/bherbruck/solidcraft/issues/492)), Cut walls and chamfer
  faces ([#493](https://github.com/bherbruck/solidcraft/issues/493)), faces after Revolve,
  Offset Face or Boss ([#494](https://github.com/bherbruck/solidcraft/issues/494)) and sheet
  metal thickness faces ([#495](https://github.com/bherbruck/solidcraft/issues/495)).
- Cutting a coaxial cylinder from a thin disc can lose a little volume
  ([#500](https://github.com/bherbruck/solidcraft/issues/500)); an edge flange with different
  per-edge angles leaves gaps at the corners
  ([#501](https://github.com/bherbruck/solidcraft/issues/501)).
- More in [ROADMAP.md](https://github.com/bherbruck/solidcraft/blob/main/ROADMAP.md#known-limitations).

**Upgrading from 0.5 or earlier.** 0.6 renamed SolveCraft to SolidCraft and made the zip
container the default save format. `.solvecraft` designs, old settings folders and
`SOLVECRAFT_*` variables are still picked up, and `solvecraft-cli` still forwards to
`solidcraft-cli`. Want plain JSON? Use **File ▸ Save As JSON…** or `--save design.json`. On
macOS, replace `SolveCraft.app` with `SolidCraft.app`. Details:
[compatibility notes](https://github.com/bherbruck/solidcraft/blob/main/docs/compatibility.md).

Documentation, the browser app and the Flatpak feed use the SolidCraft project Pages URLs.

**Which file?**
- **Windows:** `SolidCraft-{version}-windows-x64.exe` runs as is. Or install the `.msi`, or unzip the
  `-portable.zip` (x64, x86 and arm64).
- **macOS 11+** (Apple silicon and Intel): open the `.dmg` and drag `SolidCraft.app` onto Applications.
  The CLI is in `solidcraft-cli-{version}-macos-universal.zip`.
- **Linux** (x86_64 and aarch64, glibc 2.35+): the `.AppImage` (`chmod +x`, then run it), the
  `.deb` or `.rpm`, the `.flatpak` (`flatpak install --user <file>`), the `.snap`
  (`sudo snap install --dangerous <file>`, or `sudo snap install solidcraft` from the Snap Store), or the `.tar.gz`.
- **FreeBSD:** `tar -xzf solidcraft-{version}-freebsd-x86_64.tar.gz --strip-components 1 -C /usr/local`.
- **Web:** `solidcraft-web-{version}.zip` is a static site to host yourself.
- `SHA256SUMS.txt`: `sha256sum -c SHA256SUMS.txt --ignore-missing`.
- Files without a version in the name (`solidcraft-windows-x64.msi`, `solidcraft-linux-x86_64.AppImage`, …)
  are the same packages under fixed names, for download links that always fetch the latest release.

For Flatpak feed installation and updates, add Flathub for the required runtime before
installing SolidCraft; keep both remotes in the same user installation:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists solidcraft https://bherbruck.github.io/solidcraft/flatpak/solidcraft.flatpakrepo
flatpak install --user solidcraft ai.storyteller.solvecraft
flatpak run ai.storyteller.solvecraft
```

Single-file `.flatpak` bundles include the runtime repository address; missing runtimes
still need an internet connection. [Linux installation details](https://bherbruck.github.io/solidcraft/guide/getting-started.html#linux-flatpak).

The builds aren't signed yet. On macOS, right-click `SolidCraft.app` in Applications and choose
**Open** the first time (or run `xattr -dr com.apple.quarantine /Applications/SolidCraft.app`).
On Windows, SmartScreen may say "Windows protected your PC": **More info**, then **Run anyway**.
Licences are built in (Help ▸ About ▸ Licences, `solidcraft-cli licences`); the packages also
install `THIRD-PARTY-LICENSES.txt`.

