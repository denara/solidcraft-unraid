# SolidCraft: instructions for agents

Clean-room Rust CAD. See CONTRIBUTING.md.

## This Unraid fork
[Fork rules](docs/unraid-agent-rules.md) first. Keep `codex/unraid-mcp-web`, private config, HTTP MCP and separate web/Workspace. No unsafe bridges. Validated baseline: `a5ef51f39270f206eb73cacece05663dc6cb1b1b`. Fork rules override upstream.

## Start here
See ROADMAP.md, CONTRIBUTING.md, issues and docs/architecture.md. Ask for user-visible decisions.

## Hard rules (what enforces each)
- Platform integrations need maintainer approval: `.agents/skills/solidcraft-packaging/SKILL.md`.
- Never copy other products' code, icons, text or templates; never drive or take data from commercial CAD. `.agents/skills/solidcraft-ground-rules/SKILL.md`.
- No GPL/LGPL/AGPL code, not even reading it (OpenCASCADE, FreeCAD, SolveSpace, CadQuery, LibreCAD, QCAD): `cargo xtask licences --check`.
- Original/openly licensed assets only, with ATTRIBUTION.md rows; read `assets/AGENTS.md` before icon work: `cargo xtask assets`, `cargo xtask icons`.
- Never crash: no unwrap/expect/panic/unsafe outside tests; errors use `Result`; input is hostile: clippy `-D warnings`, `hostile_params_never_panic`.
- Model decisions (operations, bodies, face/edge picks and names, depths, nearness) use exact geometry; meshes only for display, export, previews, mesh bodies (`xtask/mesh-decisions/` only shrinks): `cargo xtask mesh-decisions`.
- Everything is a command; programmatic calls never open dialogs: `.agents/skills/solidcraft-commands/SKILL.md`.
- Kernel ops and math go in `solidcore/`; features call its API: `.agents/skills/solidcraft-layers/SKILL.md`.
- Only `crates/kernel` uses truck; solidcore only via `solidcore-ops`/`-tess`/`-record` (`xtask/solidcore-seam.txt`): `cargo xtask layers`/`app-geometry`, `docs/layers.md`.
- One outer UI container, one camera path (`navigation::handle`), colours from theme tokens: `cargo xtask ui-containers`.
- Rust-only app code; builds for wasm: `cargo xtask wasm`, `cargo xtask web-offline-test`.
- Name: SolidCraft; old name: `docs/compatibility.md`.
- Sort message catalogs, `mod` lines and languages: `cargo xtask lists`. No generated-file edits (`docs/features.md`, `THIRD-PARTY-LICENSES.txt`).
- One new file per design-format change in `crates/doc/src/format/pending/`: `scripts/test-format-pending.sh`.
- Commands need I/O schemas (soon required; `crates/engine/src/schema/unconverted.txt` only shrinks): `.agents/skills/solidcraft-commands/SKILL.md`.
- New UI strings go in `assets/locales/pending/<slug>.json`, ATTRIBUTION rows in `attribution/pending/<slug>.md`: `cargo xtask lists`.
- Changed UX defaults (not kernel results): preserve old designs via a format legacy value; runtime choices in tool options; fill `Changes a default:`.
- Regression tests in existing area files, never per-bug files (#3226).
- Check open PRs, claim by draft (lowest number wins), one topic, English, newcomer-readable title, own screenshots for visible UI/model changes: `.agents/skills/solidcraft-contributing/SKILL.md`.
- Before pushing: `cargo xtask ci`, which runs what your change needs. `cargo xtask skills` checks router and both skill folders.

## Skills: read the one for your task
Dev skills: `.agents/skills/` (`.claude/skills` links there). In-app MCP skills: `skills/`.

| Skill | When |
|---|---|
| `.agents/skills/solidcraft-contributing/SKILL.md` | a feature, a fork, a claim, every PR's rules |
| `.agents/skills/solidcraft-file-pr/SKILL.md` | opening a PR |
| `.agents/skills/solidcraft-babysit-pr/SKILL.md` | after a push: CI, review |
| `.agents/skills/solidcraft-full-stop/SKILL.md` | a change end to end |
| `.agents/skills/solidcraft-takeover/SKILL.md` | finishing someone else's PR |
| `.agents/skills/solidcraft-modernize/SKILL.md` | an old PR |
| `.agents/skills/solidcraft-ground-rules/SKILL.md` | licences, assets, the name |
| `.agents/skills/solidcraft-crash-safety/SKILL.md` | non-test Rust, crashes |
| `.agents/skills/solidcraft-commands/SKILL.md` | a command, a crate boundary |
| `.agents/skills/solidcraft-layers/SKILL.md` | SolidCraft or solidcore |
| `.agents/skills/solidcraft-kernel-rules/SKILL.md` | geometry, test expectations |
| `.agents/skills/solidcraft-ui/SKILL.md` | a dialog, panel, camera, icon, colour |
| `.agents/skills/solidcraft-shared-controls/SKILL.md` | selection, keys, tool options |
| `.agents/skills/solidcraft-repro-first/SKILL.md` | OS, graphics, updater fixes |
| `skills/solidcraft-sheetmetal/SKILL.md` | sheet metal |
| `.agents/skills/solidcraft-ui-text/SKILL.md` | labels, messages, translations |
| `.agents/skills/solidcraft-file-formats/SKILL.md` | design saving, import, export |
| `.agents/skills/solidcraft-preferences/SKILL.md` | a changed default, a new option |
| `.agents/skills/solidcraft-tests/SKILL.md` | writing or running tests |
| `.agents/skills/solidcraft-performance/SKILL.md` | slow work, the window thread, wasm |
| `.agents/skills/solidcraft-packaging/SKILL.md` | releases, installers, packages |
| `.agents/skills/solidcraft-agent-tools/SKILL.md` | running the app, MCP |
| `skills/solidcraft-modeling/SKILL.md` | building a part over MCP |
| `skills/solidcraft-editing/SKILL.md` | changing an existing design |
| `skills/solidcraft-reporting/SKILL.md` | a bug report |
| `skills/solidcraft-checking/SKILL.md` | checking, porting, designs in git |
| `skills/solidcraft-mechanisms/SKILL.md` | assemblies, joints, motion |
| `.agents/skills/update-solvecraft-unraid/SKILL.md` | upstream updates for this Unraid fork |
