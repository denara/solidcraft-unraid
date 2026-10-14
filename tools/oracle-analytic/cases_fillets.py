#!/usr/bin/env python3
"""Closed-form expected values for the fillets whose radius varies along a straight edge
(cases 77-81 of tests/oracle: variable, closing at a corner, chord length, taper to fit, a
B-spline radius).

Standalone, Python 3 stdlib only (it reuses the helpers of cases_features.py). Run from the
worktree root:

    python3 tools/oracle-analytic/cases_fillets.py

Each case rounds one straight edge where two faces meet at a right angle: the edge along x at
y = 0, z = H, between the front face (y = 0) and the top face (z = H). Every section square to
the edge is a quarter-circle round of radius r(x), so with the spandrel of radius r (the r x r
corner minus the quarter disc, area (1 - pi/4) r^2, centroid r (10 - 3 pi) / (3 (4 - pi)) from
both faces):

    V removed = int (1 - pi/4) r^2 dx
    moments   = int (x, off(r), H - off(r)) (1 - pi/4) r^2 dx
    area      = - 2 int r dx               (a strip r wide off each face)
                - (1 - pi/4) (r(0)^2 + r(L)^2)  (the spandrel off each end face)
                + the round's own area, int int |P_x x P_t| dt dx over
                  P(x, t) = (x, r (1 - cos t), H - r (1 - sin t)), t in [0, pi/2]

r is a polynomial on each piece of the edge, so the volume and moments are integrated exactly
by Gauss-Legendre per piece; the round's area (a square root) to round-off with 24 x 24 points.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cases_features import GL_W, GL_X, MP, PI, box, extrude, extrude_area, poly, region, spandrel_area, spandrel_offset, write  # noqa: E402

K = 1 - PI / 4


def removed(radius, slope, pieces, H):
    """What rounding the edge (x from 0 to L, corner at y = 0, z = H) with radius(x) (and its
    slope(x)) takes away: its mass properties and the change of area. `pieces` are the x intervals
    on which r is one polynomial."""
    v = mx = my = mz = strips = blend = 0.0
    for a, b in pieces:
        h = (b - a) / 2
        for xg, wg in zip(GL_X, GL_W):
            x = (a + b) / 2 + h * xg
            w = wg * h
            r, s = radius(x), slope(x)
            sec = spandrel_area(r)
            v += w * sec
            mx += w * x * sec
            my += w * spandrel_offset(r) * sec
            mz += w * (H - spandrel_offset(r)) * sec
            strips += w * 2 * r
            # The round: |P_x x P_t| with P_x = (1, s (1 - cos t), -s (1 - sin t)),
            # P_t = (0, r sin t, r cos t).
            for tg, tw in zip(GL_X, GL_W):
                t = PI / 4 * (1 + tg)
                px = (1.0, s * (1 - math.cos(t)), -s * (1 - math.sin(t)))
                pt = (0.0, r * math.sin(t), r * math.cos(t))
                c = (px[1] * pt[2] - px[2] * pt[1], px[2] * pt[0] - px[0] * pt[2], px[0] * pt[1] - px[1] * pt[0])
                blend += w * tw * PI / 4 * math.sqrt(sum(q * q for q in c))
    ends = spandrel_area(radius(pieces[0][0])) + spandrel_area(radius(pieces[-1][1]))
    return MP(v, [mx, my, mz]), blend - strips - ends


def linear(a, b, length):
    """r running straight from a (x = 0) to b (x = length)."""
    return (lambda x: a + (b - a) * x / length), (lambda x: (b - a) / length)


def c77():
    r, dr = linear(2.0, 5.0, 40.0)
    cut, da = removed(r, dr, [(0.0, 40.0)], 20.0)
    write("77-fillet-variable-linear",
          "box 40x30x20 with its top front edge (y=0, z=20) rounded at a radius running straight from 2 (x=0) to 5 (x=40): "
          "sections are quarter rounds, V = abc - (1 - pi/4) L (a^2 + ab + b^2) / 3; moments and the round's area "
          "integrated along the edge (tools/oracle-analytic/cases_fillets.py)",
          1, box(0, 0, 0, 40, 30, 20) - cut, 5200 + da, [(0, 0, 0), (40, 30, 20)], faces=7)


def c78():
    r, dr = linear(4.0, 0.0, 40.0)
    cut, da = removed(r, dr, [(0.0, 40.0)], 20.0)
    write("78-fillet-variable-to-zero",
          "as 77 with the radius running from 4 (x=0) down to 0 (x=40), where the round closes to the corner: "
          "V = abc - (1 - pi/4) L a^2 / 3; the round is three-sided and the corner at x=40 stays",
          1, box(0, 0, 0, 40, 30, 20) - cut, 5200 + da, [(0, 0, 0), (40, 30, 20)], faces=7)


def c79():
    rc = 4.0 / (2 * math.cos(PI / 4))
    cut, da = removed(lambda x: rc, lambda x: 0.0, [(0.0, 40.0)], 20.0)
    write("79-fillet-chord-length",
          "box 40x30x20, top front edge rounded 4 wide across (chord c): r = c / (2 cos 45 deg) = 2 sqrt 2; "
          "V = abc - (1 - pi/4) r^2 L, A = 2(ab + bc + ca) - 2 r L + (pi/2) r L - 2 (1 - pi/4) r^2",
          1, box(0, 0, 0, 40, 30, 20) - cut, 5200 + da, [(0, 0, 0), (40, 30, 20)], faces=7)


def bspline(degree, knots, ctrl, length):
    """A clamped B-spline radius in u = x / length (the kernel's `RadiusSpline`): r(x), its slope
    dr/dx (de Boor on the span holding u, the slope from the differenced controls) and the x
    pieces between distinct knots, on which r is one polynomial."""
    n = len(ctrl)

    def deboor(p, kn, c, u):
        k = max(i for i in range(p, len(c)) if kn[i] <= u) if u < kn[len(c)] else len(c) - 1
        d = [c[j + k - p] for j in range(p + 1)]
        for rr in range(1, p + 1):
            for j in range(p, rr - 1, -1):
                i = j + k - p
                al = (u - kn[i]) / (kn[i + p + 1 - rr] - kn[i])
                d[j] = (1 - al) * d[j - 1] + al * d[j]
        return d[p]

    dctrl = [degree * (ctrl[i + 1] - ctrl[i]) / (knots[i + degree + 1] - knots[i + 1]) for i in range(n - 1)]
    breaks = sorted(set(knots))
    pieces = [(length * a, length * b) for a, b in zip(breaks, breaks[1:])]
    return (lambda x: deboor(degree, knots, ctrl, x / length)), (lambda x: deboor(degree - 1, knots[1:-1], dctrl, x / length) / length), pieces


# The taper law (crates/kernel/src/blend_variable.rs `taper`): a clamped cubic B-spline in
# u = x / L on 8 equal spans, its control radii min(R, 0.9 room) at the Greville abscissae.
SPANS, SHARE = 8, 0.9


def taper(radius, room, length):
    knots = [0.0] * 4 + [k / SPANS for k in range(1, SPANS)] + [1.0] * 4
    greville = [sum(knots[j + 1:j + 4]) / 3 for j in range(SPANS + 3)]
    ctrl = [min(radius, SHARE * room(u)) for u in greville]
    return (*bspline(3, knots, ctrl, length), ctrl)


def c80():
    # Plate: top face narrowing from 20 (x=0) to 2 (x=40), 15 high. The room for the round at u:
    # the top face's width and the front face's height, times tan 45 deg.
    r, dr, pieces, ctrl = taper(3.0, lambda u: min(20 - 18 * u, 15.0), 40.0)
    reg = region([poly([(0, 0), (40, 0), (40, 2), (0, 20)])])
    cut, da = removed(r, dr, pieces, 15.0)
    write("80-fillet-taper-to-fit",
          "plate (top 20 -> 2 wide over x 0..40, 15 high) with its top front edge filleted at 3, fit: taper. The radius "
          "is the taper law: a clamped cubic B-spline in u = x/40 on 8 equal spans, control radii min(3, 0.9 room) at the "
          "Greville abscissae, room = min(20 - 18u, 15) tan 45 deg (top face width, front face height): "
          + ", ".join(f"{c:.4f}" for c in ctrl) + ". Sections are quarter rounds; V, moments and areas integrated along the edge",
          1, extrude(reg, 0, 15) - cut, extrude_area(reg, 15) + da, [(0, 0, 0), (40, 20, 15)], faces=7)


def c81():
    knots = [0.0, 0.0, 0.0, 0.0, 0.25, 0.6, 1.0, 1.0, 1.0, 1.0]
    ctrl = [2.0, 5.0, 1.5, 4.0, 1.0, 3.0]
    r, dr, pieces = bspline(3, knots, ctrl, 40.0)
    cut, da = removed(r, dr, pieces, 20.0)
    write("81-fillet-spline-radius",
          "box 40x30x20 with its top front edge (y=0, z=20) rounded at a radius that is a clamped cubic B-spline in "
          "u = x/40, knots 0,0,0,0,0.25,0.6,1,1,1,1, control radii 2, 5, 1.5, 4, 1, 3 (r(0) = 2, r(40) = 3). Sections are "
          "quarter rounds; V, moments and areas integrated along the edge, exactly per knot span",
          1, box(0, 0, 0, 40, 30, 20) - cut, 5200 + da, [(0, 0, 0), (40, 30, 20)], faces=7)


CASES = [c77, c78, c79, c80, c81]

if __name__ == "__main__":
    for fn in CASES:
        fn()
