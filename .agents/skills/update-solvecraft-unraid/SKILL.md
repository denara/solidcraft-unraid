---
name: update-solvecraft-unraid
description: "Check or prepare upstream updates for the SolidCraft/SolveCraft Unraid fork, preserving its HTTP MCP gateway, independent browser app, private local configuration and saved designs. Use when the user requests an upstream update, rebuild or deployment update; not for ordinary CAD modelling."
---

# Update the SolveCraft Unraid fork

Maintain the deployment addition while advancing one explicitly selected upstream snapshot.
Use the repository's current instructions and configuration, not remembered hostnames,
filesystem paths or the previous chat's installation state.

## Establish the requested scope

- A request to check for changes authorizes fetching and comparison, not integration or deployment.
- A request to prepare an update authorizes the local integration, fixes, build, validation
  and deliverable artifact. Follow repository rules for committing and publishing the work branch.
- Updating a running Unraid installation is a distinct scope. Perform it when the user has
  requested it and access is available; preserve existing authorization rather than asking again.
- This skill is a reusable workflow, not a scheduler. An upstream commit by itself does not
  authorize a production restart, image publication, release or recurring automation.

## Repository and deployment context

Read root `AGENTS.md`, `docs/unraid.md`, and the applicable local update instructions.
SolidCraft is the upstream display name; `solvecraft` binaries, crate names, environment
variables and `.solvecraft` designs retain compatibility.

The repository may still contain the deployment only on `codex/unraid-mcp-web`, rather
than on `origin/main`. Find the revision containing `apps/solvecraft-server`, `Dockerfile`,
`compose.yaml` and `packaging/unraid/` before choosing an update base.
Do not start from an upstream-only main branch and accidentally remove the integration.

Use [the update workflow](references/upstream-update.md) for integration, validation
and delivery. It covers published branches, license conflicts, isolated MCP tests and rollback.

## Invariants

- Web and MCP remain independent: browser designs live in browser storage; server designs
  saved explicitly live under the persistent `/data/files` directory.
- Preserve the existing gateway's authentication, origin checks, bounded sessions, timeouts,
  file confinement and subprocess isolation. Do not replace it with the unauthenticated
  desktop control channel or connect the browser to server models as part of an update.
- Keep actual hostnames, IP addresses, deployment paths, tokens and private examples in
  ignored local files. Commit only generic examples; exclude private files from build contexts.
- Build and develop on the development computer or a suitable build runner, not on Unraid.
- Select and record the target upstream commit once. Report later upstream movement separately
  instead of continually restarting integration and tests to chase a moving main branch.
- An explicit update request may advance the recorded prototype baseline after validation.
  The original pinned baseline is not a permanent prohibition on requested updates.

## Completion

Report the old and selected new upstream revisions, integration branch/commit, conflicts
resolved, quality gates and real MCP/web checks, image/archive location, and deployment status.
Distinguish a prepared artifact from an installed update. If no upstream changes are present,
report that without creating empty commits or rebuilding unchanged artifacts.
