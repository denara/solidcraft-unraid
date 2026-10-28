---
name: solidcraft-crash-safety
description: "Use when writing or reviewing non-test Rust in SolidCraft, handling design files, command parameters or control-channel input, calling the kernel, or investigating a crash report. A crash loses someone's work, so this outranks feature work."
---

# Never crash

People trust SolidCraft with their designs. This outranks feature work.

## In non-test code
- No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!` or `unimplemented!`. Every production crate root carries `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`; `cargo xtask ci` runs clippy with `-D warnings`.
- No `unsafe` (`unsafe_code = "forbid"`). This container fork omits the upstream native driver, macOS file-open and Windows/macOS crash/window/HID bridges. There is no platform exception here; preserve the compatibility stubs. See `docs/unraid-agent-rules.md`.
- Errors are `Result<T, E>` through the crate's error type and `?`. An unfinished feature returns an error ("not supported yet: ..."); it never panics.
- Input-derived numbers are hostile (command parameters, design files, control-channel input): `get()` not `[i]`, finite checks, caps on sizes and loop counts.
- Everything that calls `truck` goes through `solidcraft_kernel::guard`, which turns a panic into `KernelError::Internal`. Only `crates/kernel` may use `truck`.
- Last resort: `Session::execute` runs every command under `catch_unwind`; an escaped panic restores the design and reports `EngineError::Internal`. Do not rely on it.
- Files are written with `crates/io/src/safe_write.rs` (temporary file, flush, one rename), so a crash leaves the old file or the new one.

## Proving a crash fix
- Every crash fix lands with a regression test. `hostile_params_never_panic` in `crates/engine/src/tests.rs` runs every command with junk parameters; a new command gets this for free.
- A crash report's `design.sc` and `replay.json` are the test's starting point. Put the test in the area's existing test file (see `skills/solidcraft-tests/SKILL.md`).

## When someone says the app crashed
The desktop app writes every panic and every unclean exit to a local crash folder, with the design and the commands that reproduce it (`solidcraft_engine::crash`, `docs/crash-reports.md`). Look there first:
- `solidcraft-cli crashes`, `solidcraft-cli crashes show <id>`, `solidcraft-cli crashes replay <id>`
- the `diagnostics.crash_reports` command, or the MCP `crash_reports` tool

Crashes and data loss are taken as issues even while kernel work is paused (CONTRIBUTING.md, "Kernel work is paused").
