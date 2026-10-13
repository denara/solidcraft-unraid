#!/usr/bin/env bash
# Build reference cases with FreeCAD (as an external program) and print or write the results.
#
#   tools/freecad-oracle/run.sh [--write] <case-dir>...
#
# Without --write: prints one line per case. With --write: writes <case>/expected.json and
# <case>/reference.stl (refuses to overwrite an expected.json whose source is "analytic").
# freecadcmd is found via $FREECADCMD, then `freecadcmd` on PATH, then the local AppImage.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
write=0
cases=()
for a in "$@"; do
  case "$a" in
    --write) write=1 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) cases+=("$a") ;;
  esac
done
[ ${#cases[@]} -gt 0 ] || { echo "usage: $0 [--write] <case-dir>..." >&2; exit 2; }

fc="${FREECADCMD:-}"
if [ -z "$fc" ]; then
  if command -v freecadcmd >/dev/null 2>&1; then fc="$(command -v freecadcmd)"
  elif command -v FreeCADCmd >/dev/null 2>&1; then fc="$(command -v FreeCADCmd)"
  else fc="$here/squashfs-root/usr/bin/freecadcmd"  # an AppImage extracted here (gitignored)
  fi
fi
[ -x "$fc" ] || { echo "freecadcmd not found (set FREECADCMD)" >&2; exit 2; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
status=0
for c in "${cases[@]}"; do
  c="${c%/}"
  name="$(basename "$c")"
  out="$tmp/$name.json"
  stl=""
  [ $write = 1 ] && stl="$tmp/$name.stl"
  ORACLE_CASE="$c" ORACLE_OUT="$out" ORACLE_STL="$stl" \
    "$fc" -c "exec(open('$here/build.py').read())" </dev/null >"$tmp/$name.log" 2>&1 || true
  if [ ! -s "$out" ]; then
    echo "$name: FreeCAD produced no result"; tail -5 "$tmp/$name.log"; status=1; continue
  fi
  WRITE=$write CASE="$c" OUT="$out" STL="$stl" "$fc" -c "exec(open('$here/emit.py').read())" </dev/null 2>&1 \
    | grep -v '^FreeCAD\|^(c) \|^$' || status=1
done
exit $status
