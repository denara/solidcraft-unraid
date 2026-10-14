"""Compare two STEP files exactly with OCCT: volume, area, centre of mass, and the volume of their
symmetric difference (xor) with where each piece sits. Exits 1 when xor exceeds the tolerance.

Run: uv run --no-project --with build123d python compare_step.py ACTUAL.step EXPECTED.step
"""

import argparse
import sys

from build123d import CenterOf, Shape, import_step

from run import number, point

PIECES_SHOWN = 5


def fused(shape: Shape) -> Shape:
    """Overlapping solids in one file would otherwise count their overlap twice."""
    solids = shape.solids()
    return solids[0].fuse(*solids[1:]) if len(solids) > 1 else shape


def pieces(shape: Shape) -> list[tuple[float, tuple, tuple]]:
    found = [(s.volume, tuple(s.bounding_box().min), tuple(s.bounding_box().max)) for s in shape.solids()]
    return sorted(found, reverse=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("actual")
    parser.add_argument("expected")
    parser.add_argument("--tolerance", type=float, default=1e-3, help="largest xor volume that counts as equal, mm3")
    args = parser.parse_args()

    actual, expected = fused(import_step(args.actual)), fused(import_step(args.expected))
    shift = max(abs(a - b) for a, b in zip(actual.center(CenterOf.MASS), expected.center(CenterOf.MASS)))
    print(f"volume {number(actual.volume)} vs {number(expected.volume)} mm3")
    print(f"area {number(actual.area)} vs {number(expected.area)} mm2")
    print(f"centre of mass shift {number(shift)} mm")

    sides = {"only in actual": pieces(actual - expected), "only in expected": pieces(expected - actual)}
    xor = sum(volume for found in sides.values() for volume, _, _ in found)
    count = sum(map(len, sides.values()))
    print(f"xor {number(xor)} mm3" + (f" in {count} pieces" if count else ""))
    for side, found in sides.items():
        for volume, low, high in found[:PIECES_SHOWN]:
            print(f"  {side}: {number(volume)} mm3 at {point(low)}..{point(high)}")
        if len(found) > PIECES_SHOWN:
            print(f"  {side}: {len(found) - PIECES_SHOWN} smaller pieces")
    sys.exit(1 if xor > args.tolerance else 0)


if __name__ == "__main__":
    main()
