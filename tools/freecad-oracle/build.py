# FreeCAD oracle builder for SolidCraft reference cases (tests/oracle/<case>/recipe.json).
#
# Run under FreeCAD's interpreter (freecadcmd); see run.sh. FreeCAD is used only as an external
# program through its public Python scripting API (Part, PartDesign, Sketcher, Mesh/MeshPart).
#
# Inputs (environment):
#   ORACLE_CASE   case directory (contains recipe.json)
#   ORACLE_OUT    path of a JSON file to write the measured result to
#   ORACLE_STL    optional: path of a binary STL to write (reference.stl)
#
# The recipe format and its meaning are SolidCraft's (apps/solidcraft-cli/src/recipe.rs and the
# engine in crates/); this file re-implements that meaning with FreeCAD's modelling operations.

import json
import math
import os
import re
import sys
import traceback

import FreeCAD as App
import Part
from FreeCAD import Vector as V

TOL = 1e-7
BIG = 1000.0

# Per-case additions to expected.json: "compare", "tolerance", "note", "method_extra".
# (Empty: every case written so far is built by the generic mapping and agrees with SolidCraft
# within the default tolerances, so no comparison is relaxed.)
CASE_META = {}


def vec(a):
    return V(float(a[0]), float(a[1]), float(a[2]) if len(a) > 2 else 0.0)


class RecipeError(Exception):
    pass


# --------------------------------------------------------------------------------------------
# Expressions (user parameters)
# --------------------------------------------------------------------------------------------

class Exprs:
    def __init__(self, params):
        self.vals = {}
        for p in params or []:
            self.vals[p["name"]] = self.eval(p.get("expression", p.get("value")))

    def eval(self, v, kind="length"):
        if v is None:
            return None
        if isinstance(v, (int, float)):
            return float(v)
        s = str(v).strip()
        s = re.sub(r"(?<=[0-9.\s)])\s*(mm|deg|°)\b", "", s)
        s = s.replace("°", "")
        names = dict(self.vals)
        names.update({"pi": math.pi, "sqrt": math.sqrt, "sin": math.sin, "cos": math.cos, "tan": math.tan})
        return float(eval(s, {"__builtins__": {}}, names))


# --------------------------------------------------------------------------------------------
# Sketches
# --------------------------------------------------------------------------------------------

class Plane:
    def __init__(self, origin, xd, yd, normal=None):
        self.o = vec(origin)
        self.x = vec(xd).normalize()
        self.y = vec(yd).normalize()
        n = self.x.cross(self.y)
        self.n = n.normalize()
        if normal is not None:
            given = vec(normal)
            if given.Length > 0 and given.normalize().dot(self.n) < 0.999:
                raise RecipeError("sketch plane normal disagrees with x_dir x y_dir")

    def w(self, p):
        return self.o + self.x * float(p[0]) + self.y * float(p[1])

    def placement(self):
        m = App.Matrix(self.x.x, self.y.x, self.n.x, self.o.x,
                       self.x.y, self.y.y, self.n.y, self.o.y,
                       self.x.z, self.y.z, self.n.z, self.o.z,
                       0, 0, 0, 1)
        return App.Placement(m)


def circumcenter(a, b, c):
    """2D circumcentre of three points (tuples)."""
    ax, ay = a
    bx, by = b
    cx, cy = c
    d = 2 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by))
    if abs(d) < 1e-14:
        raise RecipeError("collinear 3-point arc")
    ux = ((ax * ax + ay * ay) * (by - cy) + (bx * bx + by * by) * (cy - ay) + (cx * cx + cy * cy) * (ay - by)) / d
    uy = ((ax * ax + ay * ay) * (cx - bx) + (bx * bx + by * by) * (ax - cx) + (cx * cx + cy * cy) * (bx - ax)) / d
    return (ux, uy)


def ccw3(a, b, c):
    return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0


class Sketch:
    """A solved sketch: world-space edges per entity id and named points."""

    def __init__(self, f, ex):
        pl = f["plane"]
        self.name = f.get("name")
        self.plane = Plane(pl.get("origin", [0, 0, 0]), pl["x_dir"], pl["y_dir"], pl.get("normal"))
        self.ents = {e["id"]: e for e in f.get("entities", [])}
        self.order = [e["id"] for e in f.get("entities", [])]
        self.edges = {}  # id -> Part.Edge (world)
        self.pts = {"origin": (0.0, 0.0)}  # "id.start" etc -> local 2D
        self.ex = ex
        if f.get("constraints") or f.get("dimensions"):
            self.solved = True
            self._solve_with_sketcher(f)
        else:
            self.solved = False
            self._direct()

    # -- direct (unconstrained): coordinates as recorded, shared points reused -----------------
    def _p(self, spec):
        if spec is None:
            return None
        if isinstance(spec, list):
            return (float(spec[0]), float(spec[1]))
        r = spec.get("ref")
        if r and r in self.pts:
            return self.pts[r]
        at = spec.get("at")
        return (float(at[0]), float(at[1]))

    def _direct(self):
        P = self.plane
        for i in self.order:
            e = self.ents[i]
            t = e["type"]
            if t == "line":
                a, b = self._p(e["start"]), self._p(e["end"])
                self.pts[i + ".start"], self.pts[i + ".end"] = a, b
                self.edges[i] = Part.LineSegment(P.w(a), P.w(b)).toShape()
            elif t == "circle":
                c = self._p(e["center"])
                r = self.ex.eval(e["radius"])
                self.pts[i + ".center"] = c
                self.edges[i] = Part.Circle(P.w(c), P.n, r).toShape()
            elif t in ("arc_three_point", "arc") and e.get("center") is None:
                a, b = self._p(e["start"]), self._p(e["end"])
                m = self._p(e.get("mid") or e.get("through"))
                self.pts[i + ".start"], self.pts[i + ".end"] = a, b
                self.pts[i + ".center"] = circumcenter(a, m, b)
                self.edges[i] = Part.Arc(P.w(a), P.w(m), P.w(b)).toShape()
            elif t in ("arc_center_start_sweep", "arc"):
                c, a = self._p(e["center"]), self._p(e["start"])
                r = math.hypot(a[0] - c[0], a[1] - c[1])
                a0 = math.atan2(a[1] - c[1], a[0] - c[0])
                if t == "arc":
                    b = self._p(e["end"])
                    a1 = math.atan2(b[1] - c[1], b[0] - c[0])
                    sw = (a1 - a0) % (2 * math.pi)
                else:
                    sw = math.radians(self.ex.eval(e.get("sweep_deg", e.get("sweep"))))
                b = (c[0] + r * math.cos(a0 + sw), c[1] + r * math.sin(a0 + sw))
                m = (c[0] + r * math.cos(a0 + sw / 2), c[1] + r * math.sin(a0 + sw / 2))
                self.pts[i + ".start"], self.pts[i + ".end"], self.pts[i + ".center"] = a, b, c
                self.edges[i] = Part.Arc(P.w(a), P.w(m), P.w(b)).toShape()
            elif t == "point":
                self.pts[i] = self.pts[i + ".start"] = self._p(e.get("at") if isinstance(e.get("at"), list) else e)
            elif t == "fitted_spline":
                fp = e.get("fit_points", [])
                if fp and len(fp[0]) == 3:
                    self.edges[i] = ("helix3d", [vec(p) for p in fp])
                else:
                    pts = [P.w(p) for p in fp]
                    bs = Part.BSplineCurve()
                    bs.interpolate(pts)
                    self.edges[i] = bs.toShape()
            else:
                raise RecipeError("sketch entity type %s not supported" % t)

    # -- constrained: FreeCAD's sketch solver ---------------------------------------------------
    def _solve_with_sketcher(self, f):
        import Sketcher
        doc = App.newDocument("sk")
        sk = doc.addObject("Sketcher::SketchObject", "S")
        gid = {}  # entity id -> geo index
        pos = {}  # "id.start" -> (geo, posid)
        pos["origin"] = (-1, 1)
        coinc = []  # (refname_a, refname_b)
        swapped = {}

        def at(spec):
            if isinstance(spec, list):
                return (float(spec[0]), float(spec[1]))
            a = spec.get("at")
            return (float(a[0]), float(a[1]))

        def V2(p):
            return V(p[0], p[1], 0)

        for i in self.order:
            e = self.ents[i]
            t = e["type"]
            cons = bool(e.get("construction", False))
            if t == "line":
                g = Part.LineSegment(V2(at(e["start"])), V2(at(e["end"])))
                k = sk.addGeometry(g, cons)
                pos[i + ".start"], pos[i + ".end"] = (k, 1), (k, 2)
                for nm in ("start", "end"):
                    if isinstance(e[nm], dict) and e[nm].get("ref"):
                        coinc.append((i + "." + nm, e[nm]["ref"]))
            elif t == "circle":
                g = Part.Circle(V2(at(e["center"])), V(0, 0, 1), self.ex.eval(e["radius"]))
                k = sk.addGeometry(g, cons)
                pos[i + ".center"] = (k, 3)
                if isinstance(e["center"], dict) and e["center"].get("ref"):
                    coinc.append((i + ".center", e["center"]["ref"]))
            elif t in ("arc_three_point", "arc_center_start_sweep", "arc"):
                if t == "arc_center_start_sweep" or (t == "arc" and e.get("center") is not None):
                    c, a = at(e["center"]), at(e["start"])
                    r = math.hypot(a[0] - c[0], a[1] - c[1])
                    a0 = math.atan2(a[1] - c[1], a[0] - c[0])
                    if t == "arc":
                        b = at(e["end"])
                        sw = (math.atan2(b[1] - c[1], b[0] - c[0]) - a0) % (2 * math.pi)
                    else:
                        sw = math.radians(self.ex.eval(e.get("sweep_deg", e.get("sweep"))))
                    ccw = sw > 0
                    s0, s1 = (a0, a0 + sw) if ccw else (a0 + sw, a0)
                else:
                    a, b = at(e["start"]), at(e["end"])
                    m = at(e.get("mid") if isinstance(e.get("mid"), list) else (e.get("mid") or e.get("through")))
                    c = circumcenter(a, m, b)
                    r = math.hypot(a[0] - c[0], a[1] - c[1])
                    ccw = ccw3(a, m, b)
                    aa = math.atan2(a[1] - c[1], a[0] - c[0])
                    bb = math.atan2(b[1] - c[1], b[0] - c[0])
                    if ccw:
                        s0, s1 = aa, aa + (bb - aa) % (2 * math.pi)
                    else:
                        s0, s1 = bb, bb + (aa - bb) % (2 * math.pi)
                g = Part.ArcOfCircle(Part.Circle(V2(c), V(0, 0, 1), r), s0, s1)
                k = sk.addGeometry(g, cons)
                swapped[i] = not ccw
                pos[i + ".start"] = (k, 1) if ccw else (k, 2)
                pos[i + ".end"] = (k, 2) if ccw else (k, 1)
                pos[i + ".center"] = (k, 3)
                for nm in ("start", "end", "center"):
                    if isinstance(e.get(nm), dict) and e[nm].get("ref"):
                        coinc.append((i + "." + nm, e[nm]["ref"]))
            elif t == "point":
                p = e["at"] if isinstance(e.get("at"), list) else at(e)
                k = sk.addGeometry(Part.Point(V2(p)), cons)
                pos[i] = pos[i + ".start"] = (k, 1)
            else:
                raise RecipeError("constrained sketch entity type %s not supported" % t)
            gid[i] = k

        # union-find of points known to coincide
        parent = {}

        def find(x):
            parent.setdefault(x, x)
            while parent[x] != x:
                x = parent[x]
            return x

        def union(a, b):
            parent[find(a)] = find(b)

        def key(r):
            return pos[r] if r in pos else None

        explicit = []
        for c in f.get("constraints", []):
            if c["type"].lower() == "coincident":
                a, b = c["refs"]
                if a in pos and b in pos:
                    explicit.append((a, b))
        for a, b in coinc + explicit:
            union(key(a), key(b))

        cons_list = []
        drop_coinc = set()

        def geo_of(r):
            if r in gid:
                return gid[r]
            return pos[r][0]

        def ispt(r):
            return r in pos and r not in gid

        def ptof(r):
            # a circle/arc named where a point is wanted: its centre
            if r in pos:
                return pos[r]
            if r + ".center" in pos:
                return pos[r + ".center"]
            raise RecipeError("unknown point " + r)

        def ends(eid):
            out = []
            for nm in ("start", "end"):
                if eid + "." + nm in pos:
                    out.append(eid + "." + nm)
            return out

        for c in f.get("constraints", []):
            t = c["type"].lower()
            r = c.get("refs", [])
            if t in ("horizontal", "vertical") and len(r) == 1:
                cons_list.append(Sketcher.Constraint("Horizontal" if t == "horizontal" else "Vertical", gid[r[0]]))
            elif t in ("horizontal", "vertical", "horizontal_points", "vertical_points"):
                mode = "Horizontal" if t.startswith("horizontal") else "Vertical"
                (g1, p1), (g2, p2) = ptof(r[0]), ptof(r[1])
                cons_list.append(Sketcher.Constraint(mode, g1, p1, g2, p2))
            elif t in ("coincident", "point_on_curve"):
                a, b = r
                if ispt(a) and ispt(b):
                    coinc.append((a, b))
                else:
                    if not ispt(a):
                        a, b = b, a
                    g, p = ptof(a)
                    cons_list.append(Sketcher.Constraint("PointOnObject", g, p, gid[b]))
            elif t == "midpoint":
                a, b = r
                if not ispt(a):
                    a, b = b, a
                g, p = ptof(a)
                cons_list.append(Sketcher.Constraint("Symmetric", gid[b], 1, gid[b], 2, g, p))
            elif t in ("symmetric", "symmetry"):
                a, b, ln = r
                if ispt(a) and ispt(b):
                    (g1, p1), (g2, p2) = ptof(a), ptof(b)
                    cons_list.append(Sketcher.Constraint("Symmetric", g1, p1, g2, p2, gid[ln]))
                elif self.ents[a]["type"] == "line" and self.ents[b]["type"] == "line":
                    # two lines: their end points mirror
                    for nm in ("start", "end"):
                        (g1, p1), (g2, p2) = pos[a + "." + nm], pos[b + "." + nm]
                        cons_list.append(Sketcher.Constraint("Symmetric", g1, p1, g2, p2, gid[ln]))
                else:
                    # two circles/arcs: mirrored centres and equal radii
                    (g1, p1), (g2, p2) = ptof(a), ptof(b)
                    cons_list.append(Sketcher.Constraint("Symmetric", g1, p1, g2, p2, gid[ln]))
                    cons_list.append(Sketcher.Constraint("Equal", gid[a], gid[b]))
            elif t == "tangent":
                a, b = r
                pair = None
                for pa in ends(a):
                    for pb in ends(b):
                        if find(pos[pa]) == find(pos[pb]):
                            pair = (pa, pb)
                if pair:
                    (g1, p1), (g2, p2) = pos[pair[0]], pos[pair[1]]
                    cons_list.append(Sketcher.Constraint("Tangent", g1, p1, g2, p2))
                    drop_coinc.add(frozenset(pair))
                else:
                    cons_list.append(Sketcher.Constraint("Tangent", gid[a], gid[b]))
            elif t == "equal":
                cons_list.append(Sketcher.Constraint("Equal", gid[r[0]], gid[r[1]]))
            elif t == "parallel":
                cons_list.append(Sketcher.Constraint("Parallel", gid[r[0]], gid[r[1]]))
            elif t == "perpendicular":
                cons_list.append(Sketcher.Constraint("Perpendicular", gid[r[0]], gid[r[1]]))
            elif t == "concentric":
                coinc.append((r[0] + ".center", r[1] + ".center"))
            elif t == "collinear":
                cons_list.append(Sketcher.Constraint("Parallel", gid[r[0]], gid[r[1]]))
                cons_list.append(Sketcher.Constraint("PointOnObject", gid[r[1]], 1, gid[r[0]]))
            elif t == "fix":
                cons_list.append(Sketcher.Constraint("Block", gid[r[0]]))
            else:
                raise RecipeError("constraint %s not supported" % t)

        def cur(gp):
            g, p = gp
            if g == -1:
                return (0.0, 0.0)
            v = sk.getPoint(g, p)
            return (v.x, v.y)

        for d in f.get("dimensions", []):
            t = d["type"].lower()
            r = d.get("refs", d.get("entities"))
            val = self.ex.eval(d["value"])
            ori = d.get("orientation", "aligned")
            if t in ("distance", "length"):
                if len(r) == 1:
                    r = [r[0] + ".start", r[0] + ".end"]
                A, B = ptof(r[0]), ptof(r[1])
                pa, pb = cur(A), cur(B)
                if ori == "horizontal":
                    if pb[0] < pa[0]:
                        A, B = B, A
                    cons_list.append(Sketcher.Constraint("DistanceX", A[0], A[1], B[0], B[1], val))
                elif ori == "vertical":
                    if pb[1] < pa[1]:
                        A, B = B, A
                    cons_list.append(Sketcher.Constraint("DistanceY", A[0], A[1], B[0], B[1], val))
                else:
                    cons_list.append(Sketcher.Constraint("Distance", A[0], A[1], B[0], B[1], val))
            elif t == "offset":
                a, b = r
                if not ispt(a) and a + ".center" not in pos or a in gid and self.ents[a]["type"] == "line":
                    a, b = b, a
                g, p = ptof(a)
                cons_list.append(Sketcher.Constraint("Distance", g, p, gid[b], val))
            elif t == "radius":
                cons_list.append(Sketcher.Constraint("Radius", gid[r[0]], val))
            elif t == "diameter":
                cons_list.append(Sketcher.Constraint("Diameter", gid[r[0]], val))
            elif t == "angle":
                g1, g2 = gid[r[0]], gid[r[1]]
                d1 = sk.Geometry[g1].EndPoint - sk.Geometry[g1].StartPoint
                d2 = sk.Geometry[g2].EndPoint - sk.Geometry[g2].StartPoint
                curang = math.atan2(d1.x * d2.y - d1.y * d2.x, d1.x * d2.x + d1.y * d2.y)
                v = math.radians(val)
                cands = [v, -v, math.pi - v, -(math.pi - v)]
                best = min(cands, key=lambda q: abs(math.remainder(q - curang, 2 * math.pi)))
                cons_list.append(Sketcher.Constraint("Angle", g1, g2, best))
            else:
                raise RecipeError("dimension %s not supported" % t)

        seen = set()
        for a, b in coinc:
            if frozenset((a, b)) in drop_coinc:
                continue
            A, B = key(a), key(b)
            if A is None or B is None:
                raise RecipeError("unknown point ref %s / %s" % (a, b))
            k = frozenset((A, B))
            if k in seen or A == B:
                continue
            seen.add(k)
            cons_list.insert(0, Sketcher.Constraint("Coincident", A[0], A[1], B[0], B[1]))
        sk.addConstraint(cons_list)
        rc = sk.solve()
        doc.recompute()
        if rc != 0:
            raise RecipeError("sketch %s: FreeCAD sketch solver failed (%d)" % (self.name, rc))
        P = self.plane
        M = P.placement().toMatrix()
        for i in self.order:
            g = sk.Geometry[gid[i]]
            e = self.ents[i]
            if e["type"] == "point":
                p = g.X, g.Y
                self.pts[i] = self.pts[i + ".start"] = (p[0], p[1])
                continue
            sh = g.toShape()
            sh.transformShape(M)
            self.edges[i] = sh
            for nm in ("start", "end", "center"):
                if i + "." + nm in pos:
                    v = sk.getPoint(*pos[i + "." + nm])
                    self.pts[i + "." + nm] = (v.x, v.y)
        App.closeDocument(doc.Name)

    # -- profiles --------------------------------------------------------------------------------
    def loop_wire(self, ids):
        edges = []
        for i in ids:
            e = self.edges.get(i)
            if e is None:
                raise RecipeError("unknown curve %s in sketch %s" % (i, self.name))
            edges.append(e.copy())
        w = Part.Wire(Part.__sortEdges__(edges))
        if not w.isClosed():
            # fix tiny gaps (recorded coordinates rounded)
            w = Part.Wire(Part.__sortEdges__(edges))
            w.fixWire(None, 1e-5)
            if not w.isClosed():
                raise RecipeError("loop %s of sketch %s is not closed" % (ids, self.name))
        return w

    def regions(self):
        """The sketch's minimal regions (faces of the arrangement of its curves), each with the
        set of curve ids its outer boundary runs along."""
        if getattr(self, "_regions", None) is not None:
            return self._regions
        import BOPTools.SplitAPI as SA
        ids = [i for i in self.order if i in self.edges and not isinstance(self.edges[i], tuple)
               and not self.ents[i].get("construction")]
        edges = [self.edges[i] for i in ids]
        c = self.plane.o
        big = Part.Plane(c, self.plane.n).toShape(-1e4, 1e4, -1e4, 1e4)
        frag = SA.booleanFragments([big] + edges, "Standard", 1e-7)
        out = []
        for fc in frag.Faces:
            ow = fc.OuterWire
            if any(v.Point.distanceToPoint(c) > 5e3 for v in ow.Vertexes):
                continue  # the unbounded outside
            used = set()
            for e in ow.Edges:
                m = e.valueAt((e.FirstParameter + e.LastParameter) / 2)
                for i, x in zip(ids, edges):
                    if x.distToShape(Part.Vertex(m))[0] < 1e-6:
                        used.add(i)
            out.append((fc, used))
        self._regions = out
        return out

    def profile_face(self, prof):
        try:
            return self._profile_face_loops(prof)
        except Exception:
            pass
        # Loops that are not closed chains of whole curves (e.g. pieces of circles between
        # tangent lines): the region whose outer boundary runs along exactly these curves.
        want = set(c for lp in prof.get("loops", []) if lp.get("outer", True) for c in lp["curves"])
        hits = [fc for fc, used in self.regions() if used == want]
        if len(hits) != 1:
            raise RecipeError("profile %s of sketch %s matches %d regions" % (sorted(want), self.name, len(hits)))
        return [hits[0]]

    def _profile_face_loops(self, prof):
        outer, inner = None, []
        for lp in prof.get("loops", []):
            if len(set(lp["curves"])) != len(lp["curves"]):
                raise RecipeError("curve repeated in a loop")
            if len(lp["curves"]) > 1 and any(self.edges[c].isClosed() for c in lp["curves"]):
                raise RecipeError("a full circle in a multi-curve loop")
            f = Part.Face(self.loop_wire(lp["curves"]))
            if lp.get("outer", True):
                outer = f if outer is None else outer.fuse(f)
            else:
                inner.append(f)
        for h in inner:
            outer = outer.cut(h)
        faces = outer.Faces
        return faces


# --------------------------------------------------------------------------------------------
# The model
# --------------------------------------------------------------------------------------------

def xform(shape, m):
    """Rigid motion or reflection of a shape (exact; no conversion to B-splines)."""
    if m.determinant() < 0:
        # reflection: m = mirror about a plane; recover it (n from the eigenvector of -1)
        import numpy
        a = numpy.array([[m.A11, m.A12, m.A13], [m.A21, m.A22, m.A23], [m.A31, m.A32, m.A33]])
        w, vv = numpy.linalg.eig(a)
        k = int(numpy.argmin(w.real))
        n = V(*[float(x) for x in vv[:, k].real]).normalize()
        t = V(m.A14, m.A24, m.A34)
        o = n * (t.dot(n) / 2)
        return shape.mirror(o, n)
    s = shape.copy()
    s.transformShape(m)
    return s


def solids_of(s):
    return list(s.Solids) if s is not None else []


def compound(shapes):
    sol = []
    for s in shapes:
        sol += solids_of(s)
    return Part.makeCompound(sol)


def refine(s):
    try:
        # removeSplitter() can damage the shape it is called on: work on a deep copy
        r = s.copy().removeSplitter()
        if r.isValid() and s.isValid() and abs(r.Volume - s.Volume) < 1e-6 * max(1.0, abs(s.Volume)):
            return r
    except Exception:
        pass
    return s


def as_solid_shape(s):
    sol = s.Solids
    if len(sol) == 1:
        return sol[0]
    return Part.makeCompound(sol)


def nearest(items, p, key=None):
    vp = Part.Vertex(p)
    best, bd = None, None
    for it in items:
        d = (key(it) if key else it).distToShape(vp)[0]
        if bd is None or d < bd - 1e-9:
            best, bd = it, d
    return best, bd


def edge_tangent_at(e, v):
    """Unit tangent of edge e at vertex point v, oriented along the edge."""
    u = e.FirstParameter if (e.Vertexes[0].Point - v).Length < (e.Vertexes[-1].Point - v).Length else e.LastParameter
    t = e.tangentAt(u)
    return t.normalize() if t.Length > 0 else t


def tangent_chain(shape, seeds):
    """Grow edge selection along G1-continuous neighbours."""
    edges = list(shape.Edges)
    sel = [e for e in seeds]
    changed = True
    while changed:
        changed = False
        for e in edges:
            if any(e.isSame(s) for s in sel):
                continue
            for s in sel:
                for va in e.Vertexes:
                    for vb in s.Vertexes:
                        if va.isSame(vb):
                            ta = edge_tangent_at(e, va.Point)
                            tb = edge_tangent_at(s, vb.Point)
                            if abs(abs(ta.dot(tb)) - 1) < 1e-6:
                                sel.append(e)
                                changed = True
                                break
                    if changed:
                        break
                if changed:
                    break
    return sel


class Model:
    def __init__(self, recipe):
        self.r = recipe
        self.ex = Exprs(recipe.get("parameters"))
        self.sketches = {}
        self.planes = {
            "XY": (V(0, 0, 0), V(0, 0, 1)),
            "XZ": (V(0, 0, 0), V(0, 1, 0)),
            "YZ": (V(0, 0, 0), V(1, 0, 0)),
        }
        self.bodies = {}  # name -> shape
        self.order = []
        self.feats = {}  # feature name -> replay record (for patterns / mirror)
        self.notes = []

    # ---- bodies --------------------------------------------------------------------------------
    def set_body(self, name, shape):
        if name not in self.bodies:
            self.order.append(name)
        self.bodies[name] = shape

    def all_shape(self):
        return compound([self.bodies[n] for n in self.order if self.bodies.get(n) is not None])

    def size(self):
        s = self.all_shape()
        if not s.Solids:
            return 100.0
        return s.BoundBox.DiagonalLength

    def targets(self, f, tool=None):
        names = [n for n in f.get("result_bodies", []) if n in self.bodies]
        if names:
            return names[:1] if f.get("operation") == "join" else names
        if tool is not None:
            hit = [n for n in self.order if self.bodies[n] is not None and self.bodies[n].common(tool).Volume > 1e-9]
            if hit:
                return hit
        return list(self.order)

    def apply(self, op, tools, targets, new_name=None):
        tool = tools[0]
        for t in tools[1:]:
            tool = tool.fuse(t)
        if op == "new":
            self.set_body(new_name or "Body%d" % (len(self.order) + 1), as_solid_shape(refine(tool)))
            return
        if op == "join":
            if not targets:
                self.set_body(new_name or "Body%d" % (len(self.order) + 1), tool)
                return
            n0 = targets[0]
            s = self.bodies[n0]
            for n in targets[1:]:
                s = s.fuse(self.bodies.pop(n))
                self.order.remove(n)
            self.bodies[n0] = as_solid_shape(refine(s.fuse(tool)))
            return
        for n in targets:
            s = self.bodies[n]
            if op == "cut":
                r = s.cut(tool)
            elif op == "intersect":
                r = s.common(tool)
            else:
                raise RecipeError("operation " + op)
            self.bodies[n] = as_solid_shape(refine(r))

    # ---- selections ------------------------------------------------------------------------------
    def pick_edges(self, shape, f, transform=None):
        out = []

        def tp(v):
            return transform.multiply(v) if transform is not None else v

        for e in f.get("edges", []):
            p = vec(e.get("point") or e.get("mid"))
            if transform is not None:
                p = transform.multiply(p)
            best, d = nearest(shape.Edges, p)
            # ties (the point sits on a vertex shared with other edges, e.g. a cylinder seam):
            # prefer the edge that also passes through the recipe's start/end and is the same kind
            vp = Part.Vertex(p)
            cands = [x for x in shape.Edges if x.distToShape(vp)[0] <= d + 1e-6]
            if len(cands) > 1:
                kind = {"line": "Line", "circle": "Circle", "arc": "Circle"}.get(e.get("type"))

                def score(x):
                    sc = 0.0
                    for k in ("start", "end", "center"):
                        if k in e and k != "center":
                            sc += x.distToShape(Part.Vertex(tp(vec(e[k]))))[0]
                    if kind and x.Curve.__class__.__name__ != kind:
                        sc += 1e6
                    return sc

                best = min(cands, key=score)
            if d > 1e-3 * max(1.0, self.size()) + 1e-3:
                self.notes.append("edge pick at %s is %.3g mm from the nearest edge" % (p, d))
            sel = [best]
            # The recipe's edge runs start..end; if FreeCAD split it, take the G1 neighbours that
            # complete it.
            if "start" in e and "end" in e:
                a, b = tp(vec(e["start"])), tp(vec(e["end"]))
                closed = (a - b).Length < 1e-6

                def spans(es):
                    w = es
                    vs = []
                    for x in w:
                        vs += [v.Point for v in x.Vertexes]
                    if closed:
                        # a closed run: a closed edge, or every vertex used twice
                        if len(es) == 1 and es[0].isClosed():
                            return True
                        return all(sum(1 for q in vs if (q - v).Length < 1e-6) % 2 == 0 for v in vs)
                    ok_a = any((q - a).Length < 1e-4 for q in vs)
                    ok_b = any((q - b).Length < 1e-4 for q in vs)
                    return ok_a and ok_b

                if not spans(sel) and e.get("type") in ("line", "arc", "circle", "nurbs"):
                    sel = tangent_chain(shape, sel)
            if f.get("tangent_chain"):
                sel = tangent_chain(shape, sel)
            for s in sel:
                if not any(s.isSame(o) for o in out):
                    out.append(s)
        return out

    def pick_face(self, shape, p):
        best, d = nearest(shape.Faces, vec(p))
        return best

    def body_with(self, p):
        best, d = None, None
        for n in self.order:
            s = self.bodies[n]
            if s is None:
                continue
            dd = s.distToShape(Part.Vertex(p))[0]
            if d is None or dd < d:
                best, d = n, dd
        return best

    # ---- features --------------------------------------------------------------------------------
    def profiles(self, f):
        profs = f.get("profiles")
        if profs is None and f.get("profile") is not None:
            profs = [f["profile"]]
        faces = []
        sk = None
        for p in profs or []:
            sk = self.sketches[p["sketch"]]
            faces += sk.profile_face(p)
        if not faces:
            raise RecipeError("feature %s: no profiles" % f.get("name"))
        return faces, sk

    def extrude_tools(self, f):
        faces, sk = self.profiles(f)
        n = sk.plane.n
        e = f.get("extent", {})
        ty = e.get("type", "distance")
        d = self.ex.eval(e.get("distance", f.get("distance")))
        direction = e.get("direction", "positive")
        if ty == "symmetric":
            if e.get("is_full_length"):
                d = d / 2
            lo, hi = -d, d
        elif ty == "two_sides":
            lo, hi = -self.ex.eval(e.get("side2_opposite", e.get("side2"))), self.ex.eval(e.get("side1"))
        elif ty == "through_all":
            L = 4 * self.size() + BIG
            lo, hi = (0, L) if direction == "positive" else (-L, 0)
            if direction in ("both", "symmetric"):
                lo, hi = -L, L
        else:
            lo, hi = (0, d) if direction == "positive" else (-d, 0)
            if direction == "symmetric":
                lo, hi = -d, d
        taper = e.get("taper_deg")
        tools = []
        for fc in faces:
            if taper:
                if lo != 0:
                    raise RecipeError("taper only from the sketch plane")
                tools.append(self.tapered(fc, n, hi, math.radians(self.ex.eval(taper))))
                continue
            g = fc.copy()
            g.translate(n * lo)
            tools.append(g.extrude(n * (hi - lo)))
        return tools

    def tapered(self, face, n, length, ang):
        d = length * math.tan(ang)
        bottom = face.OuterWire
        off = Part.Face(bottom).makeOffset2D(d, 2, False, False, False)
        top = off.OuterWire.copy()
        top.translate(n * length)
        solid = Part.makeLoft([bottom, top], True, True)
        for h in face.Wires[1:]:
            hoff = Part.Face(h).makeOffset2D(-d, 2, False, False, False).OuterWire.copy()
            hoff.translate(n * length)
            solid = solid.cut(Part.makeLoft([h, hoff], True, True))
        return solid

    def revolve_tools(self, f):
        faces, sk = self.profiles(f)
        a = f.get("axis")
        if isinstance(a, dict) and a.get("dir") is not None:
            o, d = vec(a.get("origin", [0, 0, 0])), vec(a["dir"])
        else:
            ln = a.get("line", a.get("ref")) if isinstance(a, dict) else a
            ed = sk.edges[ln]
            o, d = ed.Vertexes[0].Point, ed.Vertexes[-1].Point - ed.Vertexes[0].Point
        ang = self.ex.eval(f.get("angle", 360))
        return [fc.revolve(o, d, ang) for fc in faces]

    def sweep_tools(self, f):
        faces, sk = self.profiles(f)
        path = f["path"]
        psk = self.sketches[path["sketch"]]
        ents = [psk.edges[c] for c in path["curves"]]
        if isinstance(ents[0], tuple) and ents[0][0] == "helix3d":
            return [self.helix_sweep(faces, ents[0][1])]
        pw = Part.Wire(Part.__sortEdges__([e.copy() for e in ents]))
        tools = []
        for fc in faces:
            outer = pw.makePipeShell([fc.OuterWire], True, True)
            for h in fc.Wires[1:]:
                outer = outer.cut(pw.makePipeShell([h], True, True))
            tools.append(outer)
        return tools

    def helix_sweep(self, faces, pts):
        # The fitted 3D spline of the recipe is read as the helix it samples (as SolidCraft does).
        n = len(pts)
        # least squares circle in xy
        sx = sy = sxx = syy = sxy = sxz = syz = sz = 0.0
        A = [[0.0] * 3 for _ in range(3)]
        bvec = [0.0] * 3
        for p in pts:
            row = [p.x, p.y, 1.0]
            rhs = -(p.x * p.x + p.y * p.y)
            for i in range(3):
                for j in range(3):
                    A[i][j] += row[i] * row[j]
                bvec[i] += row[i] * rhs
        import numpy
        D, E, F = numpy.linalg.solve(numpy.array(A), numpy.array(bvec))
        cx, cy = -D / 2, -E / 2
        rm = sum(math.hypot(p.x - cx, p.y - cy) for p in pts) / n
        tot = 0.0
        prev = math.atan2(pts[0].y - cy, pts[0].x - cx)
        start = prev
        for p in pts[1:]:
            a = math.atan2(p.y - cy, p.x - cx)
            d = math.remainder(a - prev, 2 * math.pi)
            tot += d
            prev = a
        turns = abs(tot) / (2 * math.pi)
        rise = pts[-1].z - pts[0].z
        pitch = abs(rise) / turns
        lefthand = (tot < 0) != (rise < 0)
        h = Part.makeHelix(pitch, abs(rise), rm, 0, lefthand)
        h.rotate(V(0, 0, 0), V(0, 0, 1), math.degrees(start))
        h.translate(V(cx, cy, pts[0].z))
        if rise < 0:
            raise RecipeError("downward helix not supported")
        hw = Part.Wire(h.Edges)
        tools = []
        for fc in faces:
            tools.append(hw.makePipeShell([fc.OuterWire], True, True))
        return tools[0]

    def loft_tools(self, f):
        secs = []
        for s in f["sections"]:
            sk = self.sketches[s["sketch"]]
            if s.get("point") is not None:
                if s.get("world"):
                    secs.append(Part.Vertex(vec(s["world"])))
                else:
                    secs.append(Part.Vertex(sk.plane.w(sk.pts[s["point"]])))
                continue
            loops = s.get("loops", [])
            w = sk.loop_wire(loops[0]["curves"])
            secs.append(w)
        ruled = bool(f.get("ruled", False))
        return [Part.makeLoft(secs, True, ruled, False)]

    def hole_tools(self, f, p, d):
        r = self.ex.eval(f["diameter"]) / 2
        size = self.size()
        top = max(size * 0.01, 0.1)
        through = f.get("extent") == "through_all" or f.get("depth") is None
        depth = 4 * size + BIG if through else self.ex.eval(f["depth"])
        ty = f.get("hole_type", "simple")
        tip = f.get("tip_angle")
        tools = []
        start = p - d * top

        def cyl(rad, dep):
            return Part.makeCylinder(rad, dep + top, start, d)

        if ty == "counterbore":
            rc = self.ex.eval(f["counterbore_diameter"]) / 2
            dc = self.ex.eval(f["counterbore_depth"])
            tools.append(cyl(rc, dc))
            tools.append(cyl(r, depth))
        elif ty == "countersink":
            rc = self.ex.eval(f["countersink_diameter"]) / 2
            half = math.radians(self.ex.eval(f["countersink_angle"])) / 2
            r_top = rc + top * math.tan(half)
            ln = (r_top - r * 0.9) / math.tan(half)
            tools.append(Part.makeCone(r_top, r * 0.9, ln, start, d))
            tools.append(cyl(r, depth))
        elif tip is not None and not through and ty in ("simple", "drilled"):
            half = math.radians(self.ex.eval(tip)) / 2
            tl = r / math.tan(half)
            # does the drill exit exactly at the shoulder (a through hole as drilled)?
            sh = self.all_shape()
            eps = size * 1e-6
            q = lambda h: p + d * h
            exits = sh.isInside(q(depth - eps), 1e-9, True) and not sh.isInside(q(depth + eps), 1e-9, True)
            tools.append(cyl(r, depth))
            if not exits:
                tools.append(Part.makeCone(r, 0, tl, p + d * depth, d))
        else:
            tools.append(cyl(r, depth))
        t = tools[0]
        for x in tools[1:]:
            t = t.fuse(x)
        return [refine(t)]

    def on_surface(self, p, d):
        sh = self.all_shape()
        L = 4 * self.size() + BIG
        line = Part.LineSegment(p - d * L, p + d * L).toShape()
        sec = sh.section(line)
        ts = [(v.Point - p).dot(d) for v in sec.Vertexes]
        if not ts:
            return p
        if any(abs(t) < 1e-6 for t in ts):
            return p
        if sh.isInside(p, 1e-9, False):
            neg = [t for t in ts if t < 0]
            return p + d * max(neg) if neg else p
        pos = [t for t in ts if t > 0]
        return p + d * min(pos) if pos else p

    def hole(self, f):
        spots = []
        n = None
        fc = f.get("face")
        if fc is not None:
            nn = fc.get("plane_normal_outward") or fc.get("normal_at_point_on_face")
            if nn:
                n = vec(nn) * -1
        if f.get("positions_from"):
            w = f["positions_from"].split()
            sk = self.sketches[w[w.index("sketch") + 1]]
            rng = w[w.index("points") + 1]
            if ".." in rng:
                a, b = rng.split("..")
                pre = re.match(r"\D*", a).group(0)
                ids = ["%s%d" % (pre, k) for k in range(int(a[len(pre):]), int(b[len(pre):]) + 1)]
            else:
                ids = rng.split(",")
            spots = [sk.plane.w(sk.pts[i]) for i in ids]
            n = sk.plane.n * -1
        else:
            spots = [vec(f["position"])]
        if fc is not None and fc.get("type") == "cylinder":
            o, ax, p = vec(fc["axis_origin"]), vec(fc["axis"]).normalize(), spots[0]
            dd = p - o
            rr = dd - ax * dd.dot(ax)
            if rr.Length > 1e-9:
                n = rr.normalize() * -1
        if n is None:
            raise RecipeError("hole direction unknown")
        n = n.normalize()
        tools = []
        for p in spots:
            p = self.on_surface(p, n)
            tools += self.hole_tools(f, p, n)
        return tools

    def fillet(self, f, transform=None):
        pts = []
        for e in f.get("edges", []):
            p = vec(e.get("point") or e.get("mid"))
            pts.append(transform.multiply(p) if transform else p)
        name = self.body_with(pts[0])
        s = self.bodies[name]
        edges = self.pick_edges(s, f, transform)
        if f["op"] == "fillet":
            r = self.ex.eval(f["radius"])
            out = s.makeFillet(r, edges)
        else:
            out = s.makeChamfer(self.ex.eval(f["distance"]), edges)
        if not out.isValid():
            out.fix(1e-7, 1e-7, 1e-7)
        self.bodies[name] = as_solid_shape(out)

    def shell(self, f):
        pts = [vec(x["point"]) for x in f.get("faces_removed", [])]
        name = self.body_with(pts[0])
        s = self.bodies[name]
        solid = s.Solids[0]
        faces = [self.pick_face(solid, p) for p in pts]
        t = self.ex.eval(f.get("inside_thickness"))
        out = solid.makeThickness(faces, -t, 1e-6, False, False, 0, 2)
        self.bodies[name] = as_solid_shape(out)

    def draft(self, f):
        pts = [vec(x["point"]) for x in f["faces"]]
        np_ = f["neutral_plane"]
        nrm = vec(np_.get("plane_normal_outward") or np_.get("normal_at_point_on_face") or [0, 0, 1]).normalize()
        origin = vec(np_.get("point") or np_.get("centroid") or [0, 0, 0])
        ang = float(f["angle"]) * (-1 if f.get("flipped") else 1)
        name = self.body_with(pts[0])
        s = self.bodies[name]
        out = self._draft_partdesign(s, pts, origin, nrm, ang)
        self.bodies[name] = as_solid_shape(out)

    def _draft_partdesign(self, s, pts, origin, pull, ang):
        # SolidCraft: faces lean in (toward the material) by `ang` going along `pull`, pivoting on
        # their line in the neutral plane. FreeCAD's PartDesign Draft does the same tilt; which
        # of its two senses matches is checked on the result below.
        doc = App.newDocument("dr")
        base = doc.addObject("Part::Feature", "Base")
        base.Shape = s
        faces = [self.pick_face(s, p) for p in pts]
        names = []
        for fc in faces:
            for i, g in enumerate(s.Faces):
                if g.isSame(fc):
                    names.append("Face%d" % (i + 1))
        nface = None
        for i, g in enumerate(s.Faces):
            if g.Surface.__class__.__name__ == "Plane" and abs(abs(g.normalAt(0, 0).dot(pull)) - 1) < 1e-9 and abs((g.Vertexes[0].Point - origin).dot(pull)) < 1e-9:
                nface = "Face%d" % (i + 1)
                break
        if nface is None:
            # a datum plane would do; the recipes use body faces
            raise RecipeError("neutral face not found")
        body = doc.addObject("PartDesign::Body", "Body")
        body.BaseFeature = base
        results = []
        for rev in (False, True):
            dr = body.newObject("PartDesign::Draft", "Draft%d" % rev)
            dr.Base = (body.BaseFeature, names)
            dr.NeutralPlane = (body.BaseFeature, [nface])
            dr.Angle = abs(ang)
            dr.Reversed = rev
            doc.recompute()
            sh = dr.Shape.copy()
            results.append(sh)
            body.removeObject(dr)
            doc.removeObject(dr.Name)
        # expected: a point a little toward the material from the face at height h > 0 along pull
        # ends up outside; pick the variant that does this.
        fc0 = faces[0]
        u0, u1, v0, v1 = fc0.ParameterRange
        c = fc0.valueAt((u0 + u1) / 2, (v0 + v1) / 2)
        nf = fc0.normalAt((u0 + u1) / 2, (v0 + v1) / 2)
        h = (c - origin).dot(pull)
        t = math.tan(math.radians(ang))
        delta = -h * t / 2  # half the expected displacement along the outward normal
        q = c + nf * delta
        pick = None
        for sh in results:
            inside = sh.isInside(q, 1e-9, True)
            if inside == (delta > 0):
                pick = sh
        App.closeDocument(doc.Name)
        if pick is None:
            raise RecipeError("draft: no FreeCAD variant matches the recipe's sense")
        return pick

    def split(self, f):
        name = f["body"]
        tool = f["tool"]
        o, n = self.planes[tool]
        s = self.bodies[name]
        L = 4 * self.size() + BIG
        pl = Part.Plane(o, n)
        face = pl.toShape(-L, L, -L, L)
        import BOPTools.SplitAPI as SA
        res = SA.slice(s, [face], "Split")
        sols = res.Solids
        names = f.get("result_bodies") or [name]
        self.bodies.pop(name)
        self.order.remove(name)
        for i, so in enumerate(sols):
            self.set_body(names[i] if i < len(names) else "%s (%d)" % (name, i), so)

    def construction_plane(self, f):
        if f.get("method") == "offset":
            o, n = self.planes[f["base"]] if isinstance(f["base"], str) else (vec(f["base"]["origin"]), vec(f["base"]["normal"]))
            self.planes[f["name"]] = (o + n * self.ex.eval(f["offset"]), n)
        else:
            self.planes[f["name"]] = (vec(f.get("result_origin", [0, 0, 0])), vec(f["result_normal"]).normalize())

    def transforms_for(self, f):
        op = f["op"]
        mats = []
        if op == "rectangular_pattern":
            d1 = f["direction1"]
            d2 = f.get("direction2")
            a1 = vec(d1["axis"]).normalize()
            c1, s1 = int(d1["count"]), self.ex.eval(d1["spacing"])
            c2, s2, a2 = 1, 0.0, V(0, 0, 0)
            if d2:
                a2, c2, s2 = vec(d2["axis"]).normalize(), int(d2["count"]), self.ex.eval(d2["spacing"])
            for j in range(c2):
                for i in range(c1):
                    if i == 0 and j == 0:
                        continue
                    m = App.Matrix()
                    m.move(a1 * (i * s1) + a2 * (j * s2))
                    mats.append(m)
        elif op == "circular_pattern":
            ax = f["axis"]
            o, dr = vec(ax.get("origin", [0, 0, 0])), vec(ax["dir"]).normalize()
            n = int(f["count"])
            tot = self.ex.eval(f.get("total_angle", 360))
            full = abs(abs(tot) - 360) < 1e-9
            step = tot / n if full else (tot / (n - 1) if n > 1 else 0)
            for i in range(1, n):
                pl = App.Placement(V(0, 0, 0), App.Rotation(dr, step * i), o)
                mats.append(pl.toMatrix())
        elif op == "path_pattern":
            path = f["path"]
            psk = self.sketches[path["sketch"]]
            edges = [psk.edges[c] for c in path["curves"]]
            n = int(f["count"])
            sp = self.ex.eval(f["spacing"]) if f.get("spacing") is not None else self.ex.eval(f["distance"]) / (n - 1)
            # path(0) is the start of the first curve as drawn; the curves are chained from there
            first = edges[0]
            p0 = first.valueAt(first.FirstParameter)
            if len(edges) > 1:
                nxt = edges[1]
                ends = [v.Point for v in nxt.Vertexes]
                if min((first.valueAt(first.FirstParameter) - q).Length for q in ends) < min((first.valueAt(first.LastParameter) - q).Length for q in ends):
                    p0 = first.valueAt(first.LastParameter)
            for k in range(1, n):
                s_ = k * sp
                cur, acc, pk = p0, 0.0, None
                for e in edges:
                    fwd = (e.valueAt(e.FirstParameter) - cur).Length < 1e-6
                    if acc + e.Length >= s_ - 1e-9 or e is edges[-1]:
                        ll = min(max(s_ - acc, 0.0), e.Length)
                        u = e.getParameterByLength(ll if fwd else e.Length - ll)
                        pk = e.valueAt(u)
                        break
                    acc += e.Length
                    cur = e.valueAt(e.LastParameter if fwd else e.FirstParameter)
                m = App.Matrix()
                m.move(pk - p0)
                mats.append(m)
        elif op == "mirror":
            pl = f["plane"]
            if pl.get("ref") in ("XY", "XZ", "YZ"):
                o, n = self.planes[pl["ref"]]
            else:
                o, n = vec(pl.get("origin", [0, 0, 0])), vec(pl["normal"])
            n = n.normalize()
            m = App.Matrix(1 - 2 * n.x * n.x, -2 * n.x * n.y, -2 * n.x * n.z, 0,
                           -2 * n.y * n.x, 1 - 2 * n.y * n.y, -2 * n.y * n.z, 0,
                           -2 * n.z * n.x, -2 * n.z * n.y, 1 - 2 * n.z * n.z, 0,
                           0, 0, 0, 1)
            t = o - m.multiply(o)
            m.move(t)
            mats.append(m)
        return mats

    def replay(self, f, mats):
        names = []
        for x in f.get("features", []):
            x = re.sub(r" \(\d+\)$", "", x.strip())
            names.append(x)
        for m in mats:
            for nm in names:
                rec = self.feats[nm]
                if rec["kind"] == "tool":
                    tools = []
                    for t in rec["tools"]:
                        tools.append(xform(t, m))
                    op = rec["op"]
                    if op == "new":
                        op = "join"
                    tg = [n for n in rec["targets"] if n in self.bodies] or self.targets({"operation": op})
                    self.apply(op, tools, tg)
                elif rec["kind"] == "edges":
                    self.fillet(rec["f"], transform=m)

    def mirror(self, f):
        mats = self.transforms_for(f)
        if f.get("mirror_of") == "bodies":
            for b in f["bodies"]:
                s = self.bodies[b]
                ms = xform(s, mats[0])
                if f.get("combine"):
                    self.bodies[b] = as_solid_shape(refine(s.fuse(ms)))
                else:
                    self.set_body(b + " (mirror)", ms)
            return
        self.replay(f, mats)

    def combine(self, f):
        tgt = f.get("target_body") or f.get("target")
        tools = [self.bodies[t] for t in (f.get("tool_bodies") or f.get("tools"))]
        op = f.get("operation", "join")
        self.apply(op, tools, [tgt])
        if not f.get("keep_tools"):
            for t in (f.get("tool_bodies") or f.get("tools")):
                self.bodies.pop(t, None)
                self.order.remove(t)

    def run(self):
        for f in self.r["features"]:
            if os.environ.get("ORACLE_DEBUG"):
                print("..", f["op"], f.get("name"), [(n, round(self.bodies[n].Volume, 4), len(self.bodies[n].Solids)) for n in self.order])
            op = f["op"]
            name = f.get("name")
            if op == "sketch":
                self.sketches[name] = Sketch(f, self.ex)
            elif op in ("extrude", "revolve", "sweep", "loft", "hole"):
                if op == "extrude":
                    tools = self.extrude_tools(f)
                elif op == "revolve":
                    tools = self.revolve_tools(f)
                elif op == "sweep":
                    tools = self.sweep_tools(f)
                elif op == "loft":
                    tools = self.loft_tools(f)
                else:
                    tools = self.hole(f)
                o = "cut" if op == "hole" else f.get("operation", "new")
                if o == "new":
                    rb = f.get("result_bodies") or [name]
                    self.apply("new", tools, [], rb[0])
                    tg = [rb[0]]
                else:
                    tg = self.targets(f, tools[0] if o != "join" else None)
                    self.apply(o, tools, tg)
                self.feats[name] = {"kind": "tool", "op": o, "tools": tools, "targets": tg}
            elif op in ("fillet", "chamfer"):
                self.fillet(f)
                self.feats[name] = {"kind": "edges", "f": f}
            elif op == "shell":
                self.shell(f)
            elif op == "draft":
                self.draft(f)
            elif op == "construction_plane":
                self.construction_plane(f)
            elif op == "split_body":
                self.split(f)
            elif op == "combine":
                self.combine(f)
            elif op in ("rectangular_pattern", "circular_pattern", "path_pattern"):
                if f.get("pattern_of") == "bodies":
                    raise RecipeError("body patterns not supported")
                self.replay(f, self.transforms_for(f))
            elif op == "mirror":
                self.mirror(f)
            elif op == "thread":
                pass  # cosmetic thread: no geometry
            else:
                raise RecipeError("feature op %s not supported" % op)


def measure(shape):
    sol = shape.Solids
    vol = sum(s.Volume for s in sol)
    area = sum(s.Area for s in sol)
    com = V(0, 0, 0)
    for s in sol:
        com += s.CenterOfMass * s.Volume
    com = com * (1.0 / vol) if vol else com
    bb = shape.optimalBoundingBox()
    return {
        "body_count": len(sol),
        "volume_mm3": vol,
        "area_mm2": area,
        "center_of_mass": [com.x, com.y, com.z],
        "bbox": {"min": [bb.XMin, bb.YMin, bb.ZMin], "max": [bb.XMax, bb.YMax, bb.ZMax]},
        "valid": all(s.isValid() for s in sol),
        "faces": len(shape.Faces),
    }


def write_stl(shape, path, limit=400 * 1024):
    import MeshPart
    for lin in (0.005, 0.01, 0.015, 0.02):
        m = MeshPart.meshFromShape(Shape=shape, LinearDeflection=lin, AngularDeflection=0.2, Relative=False)
        size = 84 + 50 * m.CountFacets
        if size <= limit:
            m.write(path)
            return {"stl_deflection_mm": lin, "stl_bytes": size, "stl_facets": m.CountFacets}
    return {"stl_skipped": "over %d bytes even at 0.02 mm deflection (%d facets)" % (limit, m.CountFacets)}


def main():
    case = os.environ["ORACLE_CASE"]
    out = os.environ["ORACLE_OUT"]
    stl = os.environ.get("ORACLE_STL") or None
    res = {"case": os.path.basename(case.rstrip("/"))}
    try:
        recipe = json.load(open(os.path.join(case, "recipe.json")))
        m = Model(recipe)
        m.run()
        shape = m.all_shape()
        res.update(measure(shape))
        res["version"] = ".".join(App.Version()[:3])
        res["notes"] = m.notes
        res["method"] = construction_note(recipe)
        meta = CASE_META.get(recipe.get("id"), {})
        for k in ("compare", "tolerance", "note", "method_extra"):
            if k in meta:
                res[k] = meta[k]
        if stl:
            res.update(write_stl(shape, stl))
    except Exception as e:
        res["error"] = "%s: %s" % (type(e).__name__, e)
        res["trace"] = traceback.format_exc()
    with open(out, "w") as fh:
        json.dump(res, fh, indent=1)


def construction_note(recipe):
    ops = []
    for f in recipe["features"]:
        op = f["op"]
        if op in ("sketch", "construction_plane"):
            if op == "sketch" and (f.get("constraints") or f.get("dimensions")):
                ops.append("Sketcher-solved sketch")
            continue
        word = {
            "extrude": "Part extrude",
            "revolve": "Part revolve",
            "loft": "Part.makeLoft",
            "sweep": "makePipeShell sweep",
            "hole": "hole tool (cylinder/cone) cut",
            "fillet": "Part makeFillet",
            "chamfer": "Part makeChamfer",
            "shell": "makeThickness shell",
            "draft": "PartDesign Draft",
            "rectangular_pattern": "pattern (tools transformed and re-applied)",
            "circular_pattern": "pattern (tools transformed and re-applied)",
            "path_pattern": "pattern (tools transformed and re-applied)",
            "mirror": "mirror",
            "combine": "boolean combine",
            "split_body": "split by plane",
        }.get(op, op)
        if op == "extrude" and f.get("operation") in ("cut", "join", "intersect"):
            word += " " + f["operation"]
        if not ops or ops[-1] != word:
            ops.append(word)
    return "FreeCAD (OpenCASCADE) build of recipe.json by tools/freecad-oracle/build.py: " + ", ".join(ops)


if not os.environ.get("ORACLE_NO_MAIN"):
    main()
