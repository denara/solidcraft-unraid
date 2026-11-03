# Automatic local build-storage maintenance

This is part of the existing daily update task, including runs without upstream
changes. It is agent-executed maintenance at the scheduled run, not a separate
always-running deletion service. Private paths come only from ignored
`target/build-storage.json`. Application/runtime behavior is unchanged.

## When to run

Before deciding that an unchanged upstream needs no work, check storage and
previously completed project build resources. Check again before a build and after
a successful validated update. No new application build or source commit is needed
merely to inspect or maintain storage. Routine successful maintenance stays quiet.
Keep the daily schedule and the user's existing notification policy.

Do not mutate caches/resources while a build, cache consumer, concurrent checkout
operation or unresolved recovery depends on them. Inspect all consumers, not just
containers with the current run label. Preserve useful diagnostics of failed runs;
never silently clean a resource still required for recovery.

## Cache watermarks and safety reserves

- At 90 GiB of cache data, clean obsolete compiler artifacts before the next build,
  aiming for at most 70 GiB. Recheck after a successful update as well.
- At 140 GiB of physical cache-disk size, require maintenance before another build;
  a smaller file tree does not prove that Windows has reclaimed disk-image space.
- The monitored stop limits remain 120 GiB of cache data and 160 GiB of physical
  cache disk. Keep at least 30 GiB available on the system disk and 80 GiB on the
  cache drive. Never raise limits or reduce reserves to force progress.
- Use the registered, marked cache and verified immutable toolchain. Inspect idle
  ownership, mount sources/destinations and activity before any cleanup. Prefer
  Cargo's frozen workspace/package/profile clean, including obsolete CI/dev/release
  outputs as needed. Downloaded registry dependencies and reusable toolchain images
  should stay available. Disable incremental compilation for all guarded builds.
  Always select an explicit package and/or profile for Cargo cleanup. `--target`
  alone is not a safe profile boundary: Cargo can clear the entire target tree,
  then fail trying to remove the bind-mount root. Do not unmount or force-remove
  that root to make cleanup pass. Remeasure after any failure; preserve the
  registry, proof directories and runtime, and rebuild disposable artifacts as needed.
- `cargo clean` is deletion, not compilation: a bounded, explicitly labelled cleanup
  helper may run when the build runner's admission limit has already refused a new
  build. It mounts only the verified disposable caches and a read-only checkout;
  it must not mount runtime data. Recheck sizes and release the helper afterwards.
- If the physical disk remains large, only the verified disposable cache filesystem
  may be reclaimed/recreated with no active consumers and with its useful registry
  preserved. Do not stop Docker/the real runtime, unregister another distribution,
  bypass ownership checks or launch administrator disk maintenance automatically.
  If safe reclamation is unavailable, defer the build and report the needed action.
  Do not let growth continue just because logical compiler files were removed.

The size limits are monitored stop thresholds, not filesystem quotas. The runner
continues checking disk reserves during builds; cleanup does not replace this guard.

## Completed build resources and retained artifacts

After the newest local runtime has passed its preservation checks, remove verified
obsolete project build/check/candidate containers. Also retire project-owned,
unreferenced candidate/application images when they are no longer useful. Keep the
running image, one previous validated rollback image, reusable toolchain images and
resources still referenced by other containers/images. Never use force removal or
global Docker/container/image/volume/builder prune. Shared BuildKit cache must not
be guessed to belong to this project merely from its size or a broad name match.

Keep the newest two successfully validated transferable image archives. Older
archives may be removed only by their verified archive/publication manifests, with
resolved absolute paths inside ignored `target`, and with no current transfer or
recovery depending on them. Keep the small manifests/checksums/publication records.
Do not remove arbitrary tar/zip/VHD files by wildcard.

For completed older runs outside those two retained versions, duplicate build-context
copies named `bin`, `web` and `workspace` may be removed after confirming they are
only generated packaging copies. Check the resolved exact directories, ownership,
content and absence of reparse points first. This is not permission to delete an
entire evidence directory. Old bulky logs/screenshots may be removed after 30 days
only for successfully completed runs; retain compact test/validation/publication
records and all evidence still needed by an unresolved failure. Current and previous
validated-run evidence remains available. Unknown contents must be preserved.

## Protected data and large writes

Projects/designs, MCP tokens, browser/session stores, data volumes, private backups,
rollback resources and unrelated projects remain outside automatic deletion. In
particular, never recursively clear an update evidence directory: it can contain
private runtime or complete Docker-disk backups next to ordinary build artifacts.
The exceptional full Docker-disk recovery backup is not recreated every day.
Normal updates retain their scoped private application-data backup.

Before any large image export, context copy or backup, check the destination's
available space against a conservative estimate plus its reserve. Stream large
writes, fail rather than consume the reserve, and leave existing valid archives
and backups intact. Only a positively identified incomplete temporary output of
the current operation may be discarded on failure. If protected data/backups or
unrelated use consume the available reserve, stop and report it; do not choose
private backups or designs as cleanup victims.

Record before/after sizes and exactly what was removed under ignored `target`.
Notify only for a meaningful update, a failed/unsafe cleanup, storage that cannot
be reclaimed automatically or a required user decision. The machine must never
be filled intentionally to test the protection.

Legacy transfer records may use `archive` instead of `file`, and older names may
contain only the upstream prefix. Include them in the inventory rather than silently
leaving them outside retention. When a sidecar lacks a whole-archive hash, use the
exact historical SHA256/size from the committed update record and verify the tar's
expected repository tag and saved export manifest. Preserve records/diagnoses;
remove only the verified archive at its resolved workspace-target path.
