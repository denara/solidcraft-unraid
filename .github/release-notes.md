SolidCraft is open-source parametric 3D CAD in Rust: sketch, constrain, extrude, and change a
parameter to watch the timeline rebuild. [Documentation](https://bherbruck.github.io/solidcraft/)
· [try it in the browser](https://bherbruck.github.io/solidcraft/app/) ·
[contributing](https://github.com/bherbruck/solidcraft/blob/main/CONTRIBUTING.md) ·
[issues](https://github.com/bherbruck/solidcraft/issues). AI agents can model with it over MCP:
`solidcraft-cli mcp` ([how](https://bherbruck.github.io/solidcraft/guide/mcp.html)).

**Why 0.8:** this is a minor bump because the design file format moved to version 16, and
because it adds many new features.

**What's new**
- **Face actions work on far more faces.** Sketches, offset planes and press-pull now attach to
  Cut, Hole and chamfer faces, each pattern or mirror copy, side walls a Join merged and the
  pieces a later feature split a face into
  ([#625](https://github.com/bherbruck/solidcraft/pull/625)), to moved or pasted components
  ([#610](https://github.com/bherbruck/solidcraft/pull/610)) and beside long thin holes
  ([#629](https://github.com/bherbruck/solidcraft/pull/629)). Across the test models, flat-face
  actions pass 70% of the time, up from 61% (sketch 52% → 71%, offset plane 52% → 71%,
  press-pull 41% → 60%).
- **Fillet and chamfer** handle a single edge whose ends meet curved or slanted faces, and blend
  smooth chains of edges ([#646](https://github.com/bherbruck/solidcraft/pull/646)): 83% → 93%
  of edges.
- **New:** user-defined physical materials stored in the design, linked components (insert
  another design), saved body visibility, named views through `view.apply`, sketch text fonts,
  corner reliefs, flanges on flanges, edge flanges that trim the base along their edge, grouped
  sketch tools, Home thumbnails, and an improved Select Other list.
- **Fixes:** filleting beside a modeled thread no longer freezes the app, a loft standing on a
  face fuses with it, and many sheet metal, hole and sketch fixes.
- **Booleans are checked.** A Boolean retry that would change the part away from the tool is now
  refused instead of returning wrong geometry, which fixes stray cuts
  ([#616](https://github.com/bherbruck/solidcraft/issues/616)). MCP feature replies include
  warnings.
- **Opening a design** keeps the window draggable, shows progress, and can be cancelled with
  Cancel or Esc.
- **Under the hood:** compact MCP replies, a kernel-neutral native B-rep format, and an opt-in
  preview from the native cache while a design rebuilds.
- **Quality:** every ordinary action is run on every face, edge, vertex and body of the test
  models; CI blocks regressions and the full matrix runs nightly.
- **Security:** private vulnerability reporting is on; see
  [SECURITY.md](https://github.com/bherbruck/solidcraft/blob/main/SECURITY.md).

**File format 16.** Designs are now format 16 (`solidcraft/16`). SolidCraft 0.7 and older can't
open designs saved by 0.8, as a container or as JSON; keep a copy of a file you may still open
in 0.7. Files from every earlier version still open in 0.8.

**Linux app ID.** The Linux app ID is now `ai.storyteller.solidcraft` (it was
`ai.storyteller.solvecraft`). Flatpak users from the SolidCraft feed: `flatpak update` reports
the old app as end-of-life and asks to replace it; say yes, and your settings come along. The
`.deb`, `.rpm` and AppImage remove the old menu entry on upgrade or first launch.

**Known limitations**
- Some faces still can't take an attached sketch: after Revolve, Offset Face or Boss
  ([#494](https://github.com/bherbruck/solidcraft/issues/494)), sheet metal thickness faces
  ([#495](https://github.com/bherbruck/solidcraft/issues/495)), Loft, Sweep and Coil faces
  ([#561](https://github.com/bherbruck/solidcraft/issues/561)), faces after a Draft or a tapered
  Extrude ([#563](https://github.com/bherbruck/solidcraft/issues/563)) and a Shell's rim
  ([#564](https://github.com/bherbruck/solidcraft/issues/564)).
- A Hole on a sheet metal face is refused
  ([#560](https://github.com/bherbruck/solidcraft/issues/560)); some through-all holes along a
  wall or beside curved faces still fail
  ([#559](https://github.com/bherbruck/solidcraft/issues/559)).
- Some fillets are still refused, such as along an outline of lines and arcs
  ([#431](https://github.com/bherbruck/solidcraft/issues/431)).
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
flatpak install --user solidcraft ai.storyteller.solidcraft
flatpak run ai.storyteller.solidcraft
```

Flatpak installs from 0.7 or earlier use the old ID `ai.storyteller.solvecraft`: `flatpak update --user`
offers to replace it with `ai.storyteller.solidcraft`, and settings move along.

Single-file `.flatpak` bundles include the runtime repository address; missing runtimes
still need an internet connection. [Linux installation details](https://bherbruck.github.io/solidcraft/guide/getting-started.html#linux-flatpak).

The builds aren't signed yet. On macOS, right-click `SolidCraft.app` in Applications and choose
**Open** the first time (or run `xattr -dr com.apple.quarantine /Applications/SolidCraft.app`).
On Windows, SmartScreen may say "Windows protected your PC": **More info**, then **Run anyway**.
Licences are built in (Help ▸ About ▸ Licences, `solidcraft-cli licences`); the packages also
install `THIRD-PARTY-LICENSES.txt`.

