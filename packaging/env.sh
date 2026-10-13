# shellcheck shell=bash
# Shared setup for the packaging scripts. Source it: `. "$(dirname "$0")/../env.sh"`.
# Adapted from PhotoCraft's packaging (storytold/photocraft, Apache-2.0; see ATTRIBUTION.md).
#
# Exports:
#   ROOT                    workspace root
#   VERSION                 [workspace.package] version from Cargo.toml (override: SOLIDCRAFT_VERSION)
#
# Every SOLIDCRAFT_* override also accepts the pre-rename SOLVECRAFT_* name when the new one is
# unset (compatibility shim, see docs/compatibility.md).
#   DIST                    output directory for release artifacts (default: $ROOT/dist/release)
#   SOLIDCRAFT_BUILD_DATE   UTC build date, YYYY-MM-DD
#   CARGO_TARGET_DIR        cargo's target dir (default: $ROOT/target)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

# The version lives in one place: `[workspace.package] version` in the root Cargo.toml.
workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
}

VERSION="${SOLIDCRAFT_VERSION:-${SOLVECRAFT_VERSION:-$(workspace_version)}}"
if [ -z "$VERSION" ]; then
  echo "error: could not read [workspace.package] version from $ROOT/Cargo.toml" >&2
  exit 1
fi
export VERSION

DIST="${DIST:-$ROOT/dist/release}"
mkdir -p "$DIST"
DIST="$(cd "$DIST" && pwd)"
export DIST

export SOLIDCRAFT_BUILD_DATE="${SOLIDCRAFT_BUILD_DATE:-${SOLVECRAFT_BUILD_DATE:-$(date -u +%Y-%m-%d)}}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
# Compatibility shim, see docs/compatibility.md: the reverse-DNS id (Flatpak app id, desktop,
# metainfo and MIME file names, icon name, Wayland app_id) keeps its pre-rename value so Flatpak
# installs keep updating and desktop integration keeps working.
APP_ID=ai.storyteller.solvecraft
export APP_ID

# Emit a GitHub Actions warning (plain stderr outside Actions).
warn() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
}

# Copy the licence, notice and readme files into a package directory. The programs also carry
# them (Help > About > Licences, `solidcraft-cli licences`).
copy_docs() {
  local dest="$1" f
  for f in README.md LICENSE-MIT LICENSE-APACHE NOTICE THIRD-PARTY-LICENSES.txt; do
    if [ -f "$ROOT/$f" ]; then cp "$ROOT/$f" "$dest/"; fi
  done
}

# The app icon in every size a package needs, drawn by the CLI (no image files are committed):
# $1 = the solidcraft-cli to run, $2 = output dir. Writes hicolor/<n>x<n>/apps/$APP_ID.png,
# solidcraft.ico and solidcraft-1024.png.
make_icons() {
  local cli="$1" out="$2" s
  for s in 16 24 32 48 64 128 256 512; do
    mkdir -p "$out/hicolor/${s}x${s}/apps"
    "$cli" icon --size "$s" --out "$out/hicolor/${s}x${s}/apps/$APP_ID.png" >/dev/null
  done
  "$cli" icon --out "$out/solidcraft.ico" >/dev/null
  "$cli" icon --size 1024 --out "$out/solidcraft-1024.png" >/dev/null
}
