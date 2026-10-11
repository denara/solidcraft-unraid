# Upstream update workflow

## 1. Inspect and choose one target

Check `git status`, current branch, remotes, the recorded baseline and any relevant PR state.
Read repository instructions; the upstream author's local `plan/` directory may be absent.
For deployment-only updates, use the committed architecture and deployment docs rather
than inventing missing milestone plans. Preserve existing user edits and local configuration.

Fetch `upstream` and `origin`. Inspect the upstream log and diff from the recorded baseline
and record the exact target SHA at the beginning of the update. Use an explicit user ref
when provided; otherwise use the fetched main head for this requested update.
Do not continuously advance that target while validation is running.

Classify the changes that matter to this fork: toolchain/MSRV, dependencies/licenses, binary
or format compatibility, engine behavior, MCP protocol/tools, browser build, packaging and
assets. Review license metadata before adding dependencies; never read or integrate the
prohibited GPL/LGPL/AGPL CAD sources listed in `AGENTS.md`.

For check-only requests, return the comparison and required integration work here.

## 2. Integrate without losing the fork

Use the single working branch `codex/unraid-mcp-web` for every requested or scheduled
update. Fetch origin and synchronize with `origin/codex/unraid-mcp-web`: fast-forward
when possible, or merge divergent commits without discarding anyone's work. Verify that
the deployment gateway and packaging remain present. Do not create an update branch.

Merge the selected upstream SHA directly into `codex/unraid-mcp-web`. Preserve published
history: no rebases of integration merges, no force-pushes, and no resets of user work.
Check `origin/main` separately by ancestry comparison; merge genuinely new changes when
needed. Do not use `git pull --rebase origin main`, which can flatten the validated merge
history even when main is already an ancestor. Revalidate when integration changes code.
If user edits or concurrent project work prevent safe integration, leave them intact and
report or defer the update instead of inventing another branch.

Resolve conflicts by preserving both the upstream behavior and the deployment addition.
In particular, retain fork-specific privacy, snapshot and publishing rules in `AGENTS.md`.
Keep the server package in the dependency-layer table and shipped-license roots.
Regenerate `THIRD-PARTY-LICENSES.txt` using `cargo xtask licences`; do not choose an entire
'ours' or 'theirs' generated license file. Keep unrelated dependency upgrades out of Cargo.lock.

Keep existing compatibility identifiers unless the chosen upstream change requires a
documented migration. Adapt Docker toolchain, Trunk version or runtime dependencies only
when required by the selected changes.

## 3. Validate using the development environment

Use the Dockerfile's toolchain stage when host Rust is too old. Build Linux x86_64 images
locally and place Cargo output under the repository's target directory or its mounted
development cache. Run `cargo xtask ci`, including licenses, assets, layers and WebAssembly.
Fix integration failures and add regression coverage when changing crash behavior.

Build a candidate image with a distinct tag so the currently working image remains available.
Test it with a fresh named volume and a free loopback port; do not replace an existing
container or reuse a volume containing real designs just to run smoke tests.
Use a disposable test credential of at least 32 visible characters, not a production token.

Run `packaging/unraid/smoke-test.ps1` against the isolated candidate using its `-BaseUrl`,
`-Token` and ignored `-OutputDirectory` parameters. Verify the actual tool catalog, independent
sessions, sketch/extrusion, 24000 mm3 volume, PNG preview, STEP download, save, authentication
and path/origin rejection. Restart the test container with the same test volume and run
the script with `-Reopen`. If MCP compatibility changed, also exercise a real client SDK.

Open the candidate's independent browser app, wait for it to load and inspect a screenshot.
Use only public-safe example designs. Keep screenshots and logs out of Git; attach suitable
evidence when creating an issue or PR. Confirm that the browser has not become a client of
the server's active CAD session.

Review design-format migrations in the upstream diff. When relevant, open a public-safe
design from the old version in the candidate and verify parameters, rebuild, measurements
and export. Test a copied fixture, never an original private design.

After validation, update the recorded tested baseline in `AGENTS.md`, `docs/unraid.md` and
the fork's roadmap/status note. Record the exact image tag or digest and the checks run.

## 4. Publish code or prepare an installation artifact

Before committing, inspect the staged diff, tracked files and author/committer identity.
Exclude `.env`, local overrides, private hostnames/addresses/paths, real design data, tokens,
logs, screenshots, build outputs and generated image archives. Verify ignore rules for
local files and build-context exclusions. Use the user's verified public commit identity;
if a private-email push is rejected, correct the unpublished commit rather than disabling
GitHub's privacy protection or force-pushing.

Apply repository quality and publication rules. Recheck the fork's main and relevant PR
before pushing. Keep other contributors' changes; validate again if integration changes
the tested result. Respect the selected upstream snapshot and never force-push.

Export a verified image with `docker save` to an ignored target path for transfer to Unraid.
Image registry publication, GitHub release branches/tags and production installation are
not implied by preparing an update. Use `docs/releasing.md` only when a release is requested.
Do not bump versions from commit prefixes or turn every development push into a release.

## 5. Install only within the requested deployment scope

Read actual deployment values from local ignored configuration or the connected target.
Nginx Proxy Manager is managed by the user through its UI; provide the changed settings
only if this update requires them. Do not commit or replace private nginx/DNS configuration.

Before an authorized installation, preserve the existing image digest/tag, a restorable
copy of server designs and the existing MCP token. Keep these backups private. Do not
delete volumes or rotate credentials as part of a routine image replacement.
The browser's OPFS/local-storage designs are separate; Unraid volume backup does not
back up browser designs. Arrange browser design export when an upstream migration needs it.

Load or pull the verified candidate and replace the container using the same persistent
data mapping, ownership, ports/network and authorization settings. Preserve the old image
until the replacement passes health, MCP, web and design-reopen checks.

If installation verification fails, revert to the previous compatible image when possible.
An old image may not read designs saved in a newer format: use the pre-update data backup
for recovery without silently overwriting subsequent user work. Stop after one failed
replacement and recovery attempt; report the failure and remaining action rather than
retrying production mutations indefinitely.

Report verified installation/rollback state. A copied archive, restarted process or HTTP
health response alone is not proof that existing designs and MCP clients still work.
