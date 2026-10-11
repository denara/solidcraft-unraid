# SolidCraft — instructions for agents

SolidCraft is a clean-room, open-source, pure-Rust parametric 3D CAD application:
sketches with a constraint solver, a parametric timeline of features, a B-rep
kernel, and a desktop UI that agents can drive. Siblings with the same conventions live next to
it in `storytold/`: `cadcraft` (AutoCAD-style, the closest relative), `photocraft`, `vectorcraft`,
`gridcraft`, `designcraft`, `wordcraft`, `deckcraft`, `soundcraft`. Repos don't share code; when
code is adapted from a sibling (MIT OR Apache-2.0) it is noted in ATTRIBUTION.md.

## Product name and compatibility
SolidCraft is the displayed product name; `solidcraft` is the repository name. Keep existing
`solvecraft` crate/binary names, `.solvecraft` files, format markers, environment variables,
settings/recovery paths and package IDs compatible. Published SolveCraft release artifacts
and attribution records keep their historical names. Public docs/web/Flatpak deployments
use project Pages `/solidcraft/`; do not add permanent old-name Pages aliases. See README's
compatibility notes.

## Container prototype baseline
The first Unraid prototype used upstream commit `af536a7`. The current validated
container baseline is `4af2d060073b17d3a6dcea2018ac9490740f8f4e`.
Keep later upstream CAD/UI feature updates separate from this deployment addition;
advance the baseline only as a separate, validated update. See [update records](docs/unraid-updates.md).

## Requested upstream updates
For an explicit upstream update, use
[update-solvecraft-unraid](.agents/skills/update-solvecraft-unraid/SKILL.md).
Select one target upstream commit at the start and finish validation against that snapshot;
report later upstream movement separately. A requested and validated update may advance
the recorded prototype baseline. The sole working branch for this fork is
`codex/unraid-mcp-web`, for both manual and scheduled updates. Synchronize that branch
with its origin counterpart and merge the selected upstream snapshot directly into it.
Do not create separate update branches. The fork's `main` is not the deployment branch.
Build and test on the development computer. Keep web and MCP sessions independent,
preserve persistent designs and the existing MCP token, and leave private deployment values
in ignored local configuration. Preparing an update does not itself install it on Unraid
or publish an application release; follow the user's actual requested scope.
After a successful update, retain one local development/test container on the newest
validated image and remove obsolete project build/check/candidate containers. Preserve
the local runtime's data volume and MCP token, useful build images/caches and a rollback
image. Check ownership and current activity before removal; do not prune unrelated Docker
resources or delete data volumes. This authorizes refreshing the local test container,
not installing on Unraid. See the update skill for verification and cleanup details.
Public container distribution is authorized through the tested container workflow to
`ghcr.io/denara/solidcraft-unraid`, with Community Applications metadata in
`denara/unraid-templates`. Publish only after the image's modelling, authentication,
file/origin, persistence and web checks pass. Keep `latest` on a passing build and retain
commit-specific tags. Verify anonymous pulls before claiming public availability.
This is container packaging, not an upstream application release or an Unraid deployment.

## Start every session here
1. Read `plan/STATUS.md` (current milestone, next task), then the task in `plan/execution-plan.md`
   and the relevant `plan/architecture.md` section. Behaviour reference: `plan/fusion/`
   (`menu-tree.json` is Fusion's toolbar dump; `oracle/*/` are parts built in Fusion with their
   recipes and measurements).
2. Kernel and library decisions: `plan/adr/0001-geometry-kernel.md` (what we may depend on, what
   is rejected for licence reasons).
3. Follow the autonomous operation protocol (`plan/execution-plan.md` §6). Don't stop to ask
   unless it lists the decision as the owner's.

`plan/` is gitignored (local only).

## Never crash
People trust SolidCraft with their designs; a crash loses their work. **This outranks feature work.**
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`).
- **Errors are `Result<T, E>`** through the crate's error type and `?`. An unfinished feature
  returns an error ("not supported yet: …"); it never panics.
- **Input-derived numbers are hostile** (command parameters, design files, control-channel
  input): `get()` not `[i]`, finite checks, caps on sizes and loop counts.
- **Third-party kernel calls are guarded:** everything that calls `truck` goes through
  `solvecraft_kernel::guard`, which turns a panic into `KernelError::Internal`.
- **Last-resort guard:** `Session::execute` runs every command under `catch_unwind`; an escaped
  panic restores the design and reports `EngineError::Internal`.
- **Prove it:** every crash fix lands with a regression test (see `hostile_params_never_panic`).
- Every production crate root carries
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.

## Non-negotiables
- **Private deployment configuration stays local.** Commit only generic example hostnames,
  network addresses and filesystem paths. Keep actual deployment values and credentials in
  ignored `.env` files or `target/`; exclude them from container build contexts as well.
- **Clean-room.** Fusion may be *observed* black-box (its UI, its toolbar dump, parts built in it
  and their measurements, kept under `plan/fusion/`, never committed). Never copy Autodesk icons,
  artwork, help text, tooltips, templates or materials, and never commit files produced by
  Fusion. Command ids and command names are used as identifiers only (`xtask/data/fusion-catalog.tsv`
  holds ids and names, nothing else). File formats come from public specifications. **Never read,
  copy or link GPL/LGPL/AGPL code**: OpenCASCADE, FreeCAD (incl. planegcs), SolveSpace,
  CadQuery/OCP, LibreCAD, QCAD — don't even read their sources. The sketch solver and the blend
  operations are our own.
- **Assets — absolutely essential.** SolidCraft contains **no Autodesk iconography, images,
  fonts or artwork — ever.** Every icon is drawn in code (`crates/ui-egui/src/icons.rs`). Any
  file asset must be original or openly licensed and **must have a row in ATTRIBUTION.md**
  (`cargo xtask assets` enforces it). Screenshots of Autodesk software are never committed.
  Breaking this rule is the most serious mistake you can make in this repo.
- **Everything is a command.** User-visible behaviour = a command in `crates/engine/src/cmd/*`
  (`CommandSpec`: id = our own dotted, lower-case id `<area>.<thing>[.<variant>]` such as
  `solid.extrude` or `sketch.rectangle.two_point`, never another product's command id; label;
  toolbar tab and panel; icon; shortcut; params doc; `enabled`; JSON `run`) + tests. The toolbar,
  dialogs, sketch tools, palette, scripts, CLI and control channel all reach the same commands.
- **Programmatic calls never open dialogs.** `engine.execute` runs the JSON form; only toolbar
  clicks (`ui.start`) start interactive tools and dialogs, which then run commands.
- **Layering** is enforced by `cargo xtask layers`: L0 `geom` → L1 `sketch`, `kernel`, `render`
  → L2 `doc` → L3 `io` → L4 `engine` → L5 `ui-egui` → apps. Nothing below L5 depends on
  egui/eframe/winit/wgpu/rfd. **The UI crate is swappable.** Only `crates/kernel` may use `truck`.
- **The UI is thin**: panels read engine state and act through `app.run(id, params)` /
  `app.start(id)`. Colours come from `theme::Tokens`.
- **Rust only.** Units are millimetres and radians internally; Z is up.
- **Quality gates** before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests,
  assets, layers). Commit after every feature arc that builds.
- **Publishing in this fork**: `origin` is github.com/denara/solidcraft-unraid;
  `upstream` is github.com/bherbruck/solidcraft. Publish deployment work only on
  `codex/unraid-mcp-web`. Fetch origin before pushing and incorporate changes from
  `origin/codex/unraid-mcp-web` by fast-forward or merge, preserving published history.
  Compare origin/main separately and merge genuine new changes when needed; do not
  run `git pull --rebase origin main` on this integration branch. Revalidate if integration
  changes the tested result. Never force-push. Never commit `plan/`, build dirs (`target*/`) or anything
  produced by Fusion (oracle STEP/STL/PNG); check `git ls-files` before pushing.
- **Never break wasm**: everything below the apps must build for `wasm32-unknown-unknown`
  (`cargo xtask wasm`, part of `cargo xtask ci`).

## Showing and following through on changes

- Help reviewers see what is broken or changed. For observable CAD, UI or model behavior,
  issues and PRs must include actual descriptive screenshots or a short recording from a
  small reproduction. This includes kernel, geometry, assembly and backend changes whose
  effects can be shown in the app. Coding agents should run the app and capture this evidence
  themselves; do not ask a human for evidence the agent can produce. Show the actual failure
  or result, preferably before and after, with brief reproduction steps, expected/actual
  behavior and annotations that point out the relevant detail.
- Flowcharts and Mermaid diagrams cannot substitute for screenshots or recordings of visible
  behavior. Use diagrams for architectural reasoning when actual screenshots cannot illustrate
  the point. Changes with no useful observable behavior do not need screenshot or video evidence.
- Use a small public-safe example and check for private designs, paths, account details or
  other sensitive content before sharing. Attach media to the issue/PR or link to an accessible
  recording; do not commit generated screenshots or videos just to provide review evidence.
- Throughout the work and before handoff, check current upstream commits and the relevant
  PR's merge status. Keep your branch current with upstream merges, resolve conflicts without
  discarding others' work, and re-run validation when integration changes the result. Use
  the fork-specific fetch/merge workflow above before pushing; never force-push. Report the checked
  upstream/PR state, any conflicts resolved and the validation performed. If a PR remains open,
  keep monitoring it while the task is active and make its pending status clear at handoff;
  do not claim it merged without checking.

## Application releases
Development pushes are not application releases. Follow `docs/releasing.md` when asked to
prepare or ship a release: choose the version from the actual changes, update Cargo.toml and
Cargo.lock, pass the quality gates, advance `release` without force-pushing, and verify the
resulting draft and its artifacts. When shipping is authorized, publish that verified draft
and confirm its tag and downstream deployment. Do not auto-release every push to `main`, infer
version bumps solely from commit prefixes, overwrite published versions, or push release tags
before the draft is verified.

## Running and looking at the app
- `cargo run --release -p solvecraft -- --sample --control PORT` (sample design + control channel).
  Pick a free port. Without a display, run it under `Xvfb :99` with `DISPLAY=:99`.
- Drive it with JSON lines on `127.0.0.1:PORT` (see `docs/control-protocol.md`):
  - `{"id":1,"method":"engine.execute","params":{"command":"solid.extrude","params":{"distance":20}}}`
  - `{"id":2,"method":"ui.start","params":{"command":"sketch.create"}}` then `ui.click {x, y}`
  - `{"id":3,"method":"ui.screenshot","params":{"path":"/tmp/shot.png"}}` then read the PNG.
- **For UI work, look at the result** (screenshot, read the PNG). Without a window use
  `ui.render` or `solvecraft-cli snapshot`.
- Headless: `solvecraft-cli run script.json --out part.step`, `solvecraft-cli eval design.solvecraft`,
  `solvecraft-cli snapshot script.json --out shot.png`, `solvecraft-cli commands`.
- Fusion oracle: `cargo xtask oracle` replays `plan/fusion/oracle/*/recipe.json` and compares
  with Fusion's `measure.json` (writes `docs/oracle.md`).
- Build with `CARGO_TARGET_DIR=<repo>/target`. Windows: `cargo xwin build --release --target x86_64-pc-windows-msvc -p solvecraft`.

## Roadmap
`ROADMAP.md` (committed) tracks status, milestones, parity and estimates. Update it whenever a
milestone task lands. `cargo xtask parity` recomputes command parity in `docs/parity.md`;
`cargo xtask oracle` recomputes `docs/oracle.md`.
