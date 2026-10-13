//! Booleans of prisms in 2D: when the target is a prism (two caps square to one direction,
//! walls of planes and cylinders along it) and the tool a prism along the same direction
//! spanning the target's height, the result is the prism over the 2D boolean of their
//! sections. Plates with holes, slots and patterns of them: exact, and immune to the mesh
//! intersection's trouble with walls meeting along lines.
//! Stepped extrusions (case floors and walls) use sections between every cap level, so a
//! circular boss can meet the rim without shifting either operand off its exact plane.

use solidcraft_geom::{Loop2, Plane, Region2, Seg2, Vec2, Vec3};
use truck_modeling as mt;

use crate::body::{Body, from_p3};
use crate::{BoolOp, KernelError, Result};

#[path = "prism_lineage.rs"]
mod lineage;

const MAX_PLANAR_SEGMENTS: usize = 8192;
const MAX_PLANAR_EVENTS: usize = 131_072;
const MAX_PLANAR_PAIRS: usize = 16_000_000;

/// Boolean of planar regions bounded by straight lines and circular arcs. Boundaries stay
/// analytic; disjoint results and retained islands are returned as separate regions.
pub fn planar_regions_boolean(a: &[Region2], b: &[Region2], op: BoolOp, tolerance: f64) -> Result<Vec<Region2>> {
    crate::guard("planar region boolean", || {
        if !tolerance.is_finite() || !(1e-9..=1.0).contains(&tolerance) {
            return Err(KernelError::Invalid("planar boolean tolerance must be finite and between 1e-9 and 1 mm".into()));
        }
        let loops = || a.iter().chain(b).flat_map(|r| std::iter::once(&r.outer).chain(&r.holes));
        let mut edges = 0usize;
        let mut samples = 0usize;
        for lp in loops() {
            edges = edges.saturating_add(lp.segs.len());
            if edges > MAX_PLANAR_SEGMENTS || lp.segs.is_empty() || !lp.is_closed(tolerance * 10.0) {
                return Err(KernelError::Invalid("planar boolean needs closed loops within its geometry budget".into()));
            }
            for s in &lp.segs {
                let finite = |p: Vec2| p.x.is_finite() && p.y.is_finite() && p.x.abs() < 1e9 && p.y.abs() < 1e9;
                let valid = match *s {
                    Seg2::Line { a, b } => finite(a) && finite(b) && a.dist(b) > 1e-12,
                    Seg2::Arc { center, radius, start, sweep } => {
                        finite(center)
                            && radius.is_finite()
                            && radius > 0.0
                            && radius < 1e9
                            && start.is_finite()
                            && sweep.is_finite()
                            && sweep.abs() > 1e-12
                            && sweep.abs() <= std::f64::consts::TAU + 1e-9
                    }
                    _ => false,
                };
                if !valid {
                    return Err(KernelError::Invalid("planar boolean supports finite nondegenerate lines and circular arcs".into()));
                }
                samples = samples.saturating_add(s.polyline(1e-3).len());
                if samples > 65_536 {
                    return Err(KernelError::Invalid("planar boolean sampled geometry exceeds its budget".into()));
                }
            }
        }
        boolean2d(a, b, op, tolerance)
            .ok_or_else(|| KernelError::Failed("planar region arrangement failed or exceeded its bounded processing budget".into()))
    })
}

/// Extrude normalized planar material as one logical compound, retaining every disjoint
/// region and its holes. Planar arrangement certifies separation before shells are joined.
pub fn extrude_planar_regions(plane: &Plane, regions: &[Region2], start: f64, end: f64) -> Result<Body> {
    crate::guard("planar material extrusion", || {
        // Validate aggregate inputs first; an arrangement compares opposite operands,
        // so regions in the same operand must be normalized through balanced unions.
        planar_regions_boolean(regions, &[], BoolOp::Union, 1e-7)?;
        let mut groups: Vec<Vec<Region2>> = regions.iter().cloned().map(|r| vec![r]).collect();
        let mut remaining_pairs = 32_000_000usize;
        while groups.len() > 1 {
            let mut next = Vec::new();
            let mut iter = groups.into_iter();
            while let Some(a) = iter.next() {
                if let Some(b) = iter.next() {
                    let count = |rs: &[Region2]| {
                        rs.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).fold(0usize, |n, l| n.saturating_add(l.segs.len()))
                    };
                    remaining_pairs = remaining_pairs
                        .checked_sub(count(&a).saturating_mul(count(&b)))
                        .ok_or_else(|| KernelError::Invalid("planar extrusion arrangement exceeds its work budget".into()))?;
                    next.push(planar_regions_boolean(&a, &b, BoolOp::Union, 1e-7)?);
                } else {
                    next.push(a);
                }
            }
            groups = next;
        }
        let normalized = groups.pop().unwrap_or_default();
        if normalized.is_empty() {
            return Err(KernelError::Invalid("empty planar material extrusion".into()));
        }
        for (i, a) in normalized.iter().enumerate() {
            for b in normalized.iter().skip(i + 1) {
                for s in std::iter::once(&a.outer).chain(&a.holes).flat_map(|l| &l.segs) {
                    for t in std::iter::once(&b.outer).chain(&b.holes).flat_map(|l| &l.segs) {
                        remaining_pairs = remaining_pairs
                            .checked_sub(1)
                            .ok_or_else(|| KernelError::Invalid("planar extrusion separation exceeds its work budget".into()))?;
                        let endpoints_touch = [s.start(), s.end()].iter().any(|p| t.point_at(t.closest_param(*p)).dist(*p) <= 1e-7)
                            || [t.start(), t.end()].iter().any(|p| s.point_at(s.closest_param(*p)).dist(*p) <= 1e-7);
                        if endpoints_touch || !crossings(s, t, 1e-7).is_empty() {
                            return Err(KernelError::Invalid("planar material pieces touch at a boundary".into()));
                        }
                    }
                }
            }
        }
        let pieces = crate::extrude(plane, &normalized, start, end)?;
        crate::ops::join_pieces(pieces)?.ok_or_else(|| KernelError::Failed("empty planar material extrusion".into()))
    })
}

/// A prism: the plane of its bottom cap (normal along the walls), its height and section (its
/// segments' tags alongside).
struct Prism {
    lo: f64,
    hi: f64,
    section: Section,
}

/// What a section's segment was made from, for the history of what is built over it: an
/// operand's cap edge (`edge`, by its place in the operand's edge order), the wall face beside it
/// (`wall`), and the operand's vertices (by [`Index`]) the segment starts and ends at (`None`
/// where it was split off a longer one). Where the wall ends with a slab of a stepped operand, at
/// a cap whose outline runs along the whole segment, `far` is the operand's edge there (the
/// segment may have been read off a cap at another level). A segment carried through levels
/// beside the same wall keeps the vertices it was read at, which may lie at another level: they
/// say which line two walls meet along, not which edge of it. `None` fields: not known (no
/// history then).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tag {
    pub operand: usize,
    pub wall: Option<usize>,
    pub edge: Option<usize>,
    pub far: Option<usize>,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

/// A segment nothing is known of.
const BLANK: Tag = Tag { operand: 0, wall: None, edge: None, far: None, start: None, end: None };

impl Tag {
    /// The segment run the other way.
    fn reversed(self) -> Tag {
        Tag { start: self.end, end: self.start, ..self }
    }

    /// Piece `k` of `n` the segment was split into, in its direction.
    fn piece(self, k: usize, n: usize) -> Tag {
        Tag { start: if k == 0 { self.start } else { None }, end: if k + 1 == n { self.end } else { None }, ..self }
    }

    /// The segment beside `wall` instead (`None`: not known). Its edge and vertices were read
    /// beside the wall it leaves, so they are dropped with it.
    fn beside(self, wall: Option<usize>) -> Tag {
        if wall == self.wall { self } else { Tag { operand: self.operand, wall, ..BLANK } }
    }
}

/// An operand's [`Index`], built when first needed (only bodies that are prisms are read).
pub(crate) struct LazyIndex<'a> {
    body: &'a Body,
    operand: usize,
    index: std::cell::OnceCell<Index>,
}

impl<'a> LazyIndex<'a> {
    fn new(body: &'a Body, operand: usize) -> LazyIndex<'a> {
        LazyIndex { body, operand, index: std::cell::OnceCell::new() }
    }

    fn get(&self) -> &Index {
        self.index.get_or_init(|| Index::new(self.body, self.operand))
    }
}

/// Regions with a tag for each segment: per region, per loop (outer, then holes), per segment.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Section {
    pub regions: Vec<Region2>,
    pub tags: Vec<Vec<Vec<Tag>>>,
}

impl Section {
    /// Regions whose segments nothing is known of.
    fn untagged(regions: &[Region2]) -> Section {
        let tags = regions.iter().map(|r| std::iter::once(&r.outer).chain(&r.holes).map(|l| vec![BLANK; l.segs.len()]).collect()).collect();
        Section { regions: regions.to_vec(), tags }
    }

    /// The tag of segment `s` of loop `l` of region `r` (blank where missing).
    fn tag(&self, r: usize, l: usize, s: usize) -> Tag {
        self.tags.get(r).and_then(|t| t.get(l)).and_then(|t| t.get(s)).copied().unwrap_or(BLANK)
    }

    /// The tags of region `r`'s loops, as long as its loops (blank where missing).
    fn region_tags(&self, r: usize) -> Vec<Vec<Tag>> {
        self.regions
            .get(r)
            .map(|reg| {
                std::iter::once(&reg.outer)
                    .chain(&reg.holes)
                    .enumerate()
                    .map(|(l, lp)| (0..lp.segs.len()).map(|s| self.tag(r, l, s)).collect())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One region from its loops (each segment tagged): the loop with the largest area is the
    /// outer one.
    fn from_loops(loops: Vec<(Loop2, Vec<Tag>)>) -> Option<Section> {
        let ((outer, outer_tags), holes) = split_outer(loops)?;
        let (holes, hole_tags): (Vec<Loop2>, Vec<Vec<Tag>>) = holes.into_iter().unzip();
        Some(Section { regions: vec![Region2 { outer, holes }], tags: vec![std::iter::once(outer_tags).chain(hole_tags).collect()] })
    }

    /// Region `r` alone.
    fn region(&self, r: usize) -> Section {
        Section { regions: self.regions.get(r).cloned().into_iter().collect(), tags: vec![self.region_tags(r)] }
    }

    /// Every segment in order (regions, then loops) with its tag.
    fn segments(&self) -> (Vec<Seg2>, Vec<Tag>) {
        let mut out = (Vec::new(), Vec::new());
        for (r, reg) in self.regions.iter().enumerate() {
            for (l, lp) in std::iter::once(&reg.outer).chain(&reg.holes).enumerate() {
                for (s, seg) in lp.segs.iter().enumerate() {
                    out.0.push(*seg);
                    out.1.push(self.tag(r, l, s));
                }
            }
        }
        out
    }

    /// Each segment's tag mapped by `f`.
    fn retag(&mut self, f: impl Fn(Tag) -> Tag) {
        for t in self.tags.iter_mut().flatten().flatten() {
            *t = f(*t);
        }
    }

    /// Mark each segment of a slab with its `far` edge: the operand's edge where its wall meets
    /// one of `caps` (the outlines of the operand's caps ending the slab).
    ///
    /// The operand's topology settles it where `top` gives the wall's edge (the wall ends at the
    /// caps in that one edge, [`Stations::top_edge`]): that edge is over every segment beside it.
    ///
    /// Otherwise, for a wall meeting the caps in several edges (its top split at vertices an
    /// earlier operation left, or beside a wall across it there) or carrying on past the caps
    /// elsewhere, which edge is over a segment depends on where the segment lies along the wall,
    /// and nothing in the topology says that. Only there is position compared: the segment takes
    /// the edge of the cap segment beside the same wall it runs along end to end. A segment
    /// along none, or along segments of different edges, stays unmarked; so do those past
    /// `budget` (segment pairs compared).
    fn mark_far(&mut self, caps: &[&Section], top: impl Fn(usize) -> Option<usize>, tol: f64, budget: &mut usize) {
        let mut tops: std::collections::HashMap<usize, Option<usize>> = std::collections::HashMap::new();
        // The caps' segments by wall, gathered only once a wall needs position.
        let mut by_wall: Option<std::collections::HashMap<usize, Vec<(Seg2, Option<usize>)>>> = None;
        for (reg, tags) in self.regions.iter().zip(&mut self.tags) {
            for (lp, tags) in std::iter::once(&reg.outer).chain(&reg.holes).zip(tags) {
                for (seg, tag) in lp.segs.iter().zip(tags) {
                    let Some(w) = tag.wall else { continue };
                    if let Some(e) = *tops.entry(w).or_insert_with(|| top(w)) {
                        tag.far = Some(e);
                        continue;
                    }
                    let by_wall = by_wall.get_or_insert_with(|| {
                        let mut by_wall: std::collections::HashMap<usize, Vec<(Seg2, Option<usize>)>> = std::collections::HashMap::new();
                        for (segs, tags) in caps.iter().map(|c| c.segments()) {
                            for (seg, tag) in segs.into_iter().zip(tags) {
                                if let Some(w) = tag.wall {
                                    by_wall.entry(w).or_default().push((seg, tag.edge));
                                }
                            }
                        }
                        by_wall
                    });
                    let Some(beside) = by_wall.get(&w) else { continue };
                    let Some(left) = budget.checked_sub(beside.len()) else { continue };
                    *budget = left;
                    // The one edge it runs along (`Some(None)`: along none, or not one edge).
                    let mut along: Option<Option<usize>> = None;
                    for (_, edge) in beside.iter().filter(|(t, _)| runs_along(seg, t, tol)) {
                        along = match along {
                            None => Some(*edge),
                            Some(e) if e == *edge => Some(e),
                            Some(_) => Some(None),
                        };
                    }
                    if let Some(Some(e)) = along {
                        tag.far = Some(e);
                    }
                }
            }
        }
    }

    /// Segments beside a wall another crosses between levels `lo` and `hi` are not beside it
    /// all the way: their walls are not known.
    fn drop_crossed(&mut self, stations: &Stations, lo: f64, hi: f64, tol: f64) {
        self.retag(|t| match t.wall {
            Some(w) if stations.across(w, lo, hi, tol) => t.beside(None),
            _ => t,
        });
    }

    /// Loops oriented: outers counter-clockwise, holes clockwise; the tags following their
    /// segments.
    fn oriented(&self) -> Section {
        let turn = |lp: &Loop2, tags: Vec<Tag>, reverse: bool| -> (Loop2, Vec<Tag>) {
            if reverse { (lp.reversed(), tags.into_iter().rev().map(Tag::reversed).collect()) } else { (lp.clone(), tags) }
        };
        let mut out = Section::default();
        for (r, reg) in self.regions.iter().enumerate() {
            let mut tags = self.region_tags(r).into_iter();
            let (outer, outer_tags) = turn(&reg.outer, tags.next().unwrap_or_default(), reg.outer.signed_area() < 0.0);
            let mut loop_tags = vec![outer_tags];
            let mut holes = Vec::new();
            for h in &reg.holes {
                let (hole, hole_tags) = turn(h, tags.next().unwrap_or_default(), h.signed_area() > 0.0);
                holes.push(hole);
                loop_tags.push(hole_tags);
            }
            out.regions.push(Region2 { outer, holes });
            out.tags.push(loop_tags);
        }
        out
    }
}

/// An operand's faces, edges and vertices by identity, to tag the section segments read off its
/// caps ([`Tag`]).
pub(crate) struct Index {
    operand: usize,
    /// Each edge's place in the edge order.
    edges: std::collections::HashMap<mt::EdgeID, usize>,
    /// Per edge (edge order): the faces it bounds, and its ends (by vertex number).
    bounds: Vec<(Vec<usize>, Option<usize>, Option<usize>)>,
    /// The edges between each pair of faces (lesser first; a face twice for a seam).
    between: std::collections::HashMap<(usize, usize), Vec<usize>>,
    /// Each vertex's number (first seen along the edge order).
    vertices: std::collections::HashMap<mt::VertexID, usize>,
    /// Each vertex's point, by number.
    points: Vec<Vec3>,
    /// Per edge (edge order): whether it is a line.
    lines: Vec<bool>,
}

impl Index {
    fn new(b: &Body, operand: usize) -> Index {
        let (edges, bounds) = crate::body::edge_order(b.solid.face_iter());
        let mut vertices = std::collections::HashMap::new();
        let mut points = Vec::new();
        for e in &edges {
            for v in [e.absolute_front(), e.absolute_back()] {
                if let std::collections::hash_map::Entry::Vacant(slot) = vertices.entry(v.id()) {
                    slot.insert(points.len());
                    points.push(from_p3(v.point()));
                }
            }
        }
        let lines = edges.iter().map(|e| matches!(e.curve(), mt::Curve::Line(_))).collect();
        let mut between: std::collections::HashMap<(usize, usize), Vec<usize>> = std::collections::HashMap::new();
        for (i, fs) in bounds.iter().enumerate() {
            match fs.as_slice() {
                [f] => between.entry((*f, *f)).or_default().push(i),
                [f, g] => between.entry((*f.min(g), *f.max(g))).or_default().push(i),
                _ => {}
            }
        }
        let bounds = edges
            .iter()
            .zip(bounds)
            .map(|(e, fs)| (fs, vertices.get(&e.absolute_front().id()).copied(), vertices.get(&e.absolute_back().id()).copied()))
            .collect();
        Index { operand, edges: crate::record::edge_index(&b.solid), bounds, between, vertices, points, lines }
    }

    /// The tag of the segments read off edge `e` (as it runs in a wire) of cap face `cap`.
    fn tag(&self, e: &mt::Edge, cap: usize) -> Tag {
        let edge = self.edges.get(&e.id()).copied();
        let wall = edge.and_then(|i| self.bounds.get(i)).and_then(|(fs, _, _)| match fs.as_slice() {
            [x, y] if *x == cap => Some(*y),
            [x, y] if *y == cap => Some(*x),
            _ => None,
        });
        Tag {
            operand: self.operand,
            wall,
            edge,
            far: None,
            start: self.vertices.get(&e.front().id()).copied(),
            end: self.vertices.get(&e.back().id()).copied(),
        }
    }

    /// The loops of cap face `f` (number `cap`) as segments, each with its tag; `None` where an
    /// edge isn't a line or an arc in the plane.
    fn loops(&self, f: &mt::Face, cap: usize, plane: &Plane, tol: f64, work: &mut usize, segments: &mut usize) -> Option<Vec<(Loop2, Vec<Tag>)>> {
        let mut loops = Vec::new();
        for w in f.boundaries() {
            let (mut segs, mut tags) = (Vec::new(), Vec::new());
            for e in w.edge_iter() {
                let pieces = crate::prism_curve::segment(e, plane, tol, work)?;
                *segments = segments.checked_sub(pieces.len())?;
                let tag = self.tag(e, cap);
                tags.extend((0..pieces.len()).map(|k| tag.piece(k, pieces.len())));
                segs.extend(pieces);
            }
            loops.push((Loop2 { segs }, tags));
        }
        Some(loops)
    }
}

/// Where an operand's walls along a direction end other than at its caps: at edges between two
/// walls that do not run along the direction (between pieces of one surface an earlier operation
/// left apart). A section segment carried through a level keeps the wall it was read beside
/// only where these say so, as its operand's prism or slab is read.
struct Stations {
    /// Each wall's highest level.
    tops: std::collections::HashMap<usize, f64>,
    /// Per wall, the edges across it: the other wall and the edge's lowest and highest levels.
    /// A curve other than a line spans every level.
    cross: std::collections::HashMap<usize, Vec<(usize, f64, f64)>>,
}

impl Stations {
    /// From the operand's walls (by face number, with their highest levels).
    fn new(index: &Index, tops: std::collections::HashMap<usize, f64>, d: Vec3, plane: &Plane, tol: f64) -> Stations {
        let mut cross: std::collections::HashMap<usize, Vec<(usize, f64, f64)>> = std::collections::HashMap::new();
        for (i, (fs, front, back)) in index.bounds.iter().enumerate() {
            let [f, g] = fs[..] else { continue };
            if f == g || !tops.contains_key(&f) || !tops.contains_key(&g) {
                continue;
            }
            let ends = front.and_then(|v| index.points.get(v)).zip(back.and_then(|v| index.points.get(v)));
            let (lo, hi) = match ends {
                Some((p, q)) if index.lines.get(i) == Some(&true) => {
                    let run = *q - *p;
                    if (run - d * run.dot(d)).len() < tol {
                        continue; // Along the direction: between walls over two segments.
                    }
                    let (h, k) = (plane.height(*p), plane.height(*q));
                    (h.min(k), h.max(k))
                }
                _ => (f64::NEG_INFINITY, f64::INFINITY),
            };
            cross.entry(f).or_default().push((g, lo, hi));
            cross.entry(g).or_default().push((f, lo, hi));
        }
        Stations { tops, cross }
    }

    /// The edges across wall `w`.
    fn of(&self, w: usize) -> &[(usize, f64, f64)] {
        self.cross.get(&w).map_or(&[], Vec::as_slice)
    }

    /// The operand's edge (in `index`) where wall `w` ends at level `z`, when it ends there in
    /// one edge: `z` is its top, no other wall crosses it there, and it meets the faces `caps`
    /// (the caps at `z`) in exactly one edge. Found by topology alone, no position compared. The wall reaching no higher, every segment beside
    /// it in a slab up to `z` ends on its edges at `z`, which are that one edge alone.
    fn top_edge(&self, index: &Index, w: usize, caps: &[usize], z: f64, tol: f64) -> Option<usize> {
        let ends = self.tops.get(&w).is_some_and(|t| (*t - z).abs() < tol);
        if !ends || self.of(w).iter().any(|(_, l, h)| *l < z + tol && *h > z - tol) {
            return None;
        }
        let mut edges = caps.iter().filter_map(|c| index.between.get(&(w.min(*c), w.max(*c)))).flatten();
        match (edges.next(), edges.next()) {
            (Some(e), None) => Some(*e),
            _ => None,
        }
    }

    /// Does wall `w` meet another wall across it between levels `lo` and `hi` (not at them)?
    fn across(&self, w: usize, lo: f64, hi: f64, tol: f64) -> bool {
        self.of(w).iter().any(|(_, l, h)| *l < hi - tol && *h > lo + tol)
    }

    /// The wall over a section segment beside wall `w` just past level `z`, when the segment
    /// runs on through `z` without meeting a cap there: `w` itself when nothing crosses it at
    /// `z`, and it reaches past; the one wall across it at `z` when it ends there; else not
    /// known.
    fn past(&self, w: usize, z: f64, tol: f64) -> Option<usize> {
        let at: std::collections::BTreeSet<usize> =
            self.of(w).iter().filter(|(_, l, h)| (*l - z).abs() < tol && (*h - z).abs() < tol).map(|(y, _, _)| *y).collect();
        let reaches = self.tops.get(&w).is_some_and(|t| *t > z + tol);
        match (at.len(), reaches) {
            (0, true) => Some(w),
            (1, false) => at.first().copied(),
            _ => None,
        }
    }
}

/// The highest level of face `f`'s vertices.
fn top_of(f: &mt::Face, plane: &Plane) -> f64 {
    f.vertex_iter().map(|v| plane.height(from_p3(v.point()))).fold(f64::NEG_INFINITY, f64::max)
}

/// Certify that a curved wall is swept along `d` from its surface representation.
/// Seam vertices alone cannot establish this: a half-cylinder's seam normals can
/// both be perpendicular to a transverse direction, silently filling its hole.
fn wall_along(surface: &mt::Surface, d: Vec3, tol: f64, remaining: &mut usize) -> bool {
    if let mt::Surface::RevolutedCurve(processor) = surface {
        let revolution = processor.entity();
        let mt::Curve::Line(line) = revolution.entity_curve() else { return false };
        let axis = revolution.axis();
        let meridian = line.1 - line.0;
        let local_axis = Vec3::new(axis.x, axis.y, axis.z);
        let local_meridian = Vec3::new(meridian.x, meridian.y, meridian.z);
        let (Some(a), Some(m)) = (local_axis.normalized(), local_meridian.normalized()) else { return false };
        if a.cross(m).len() > 1e-9 {
            return false; // A conical meridian is not a cylindrical wall.
        }
        let placed = *processor.transform() * mt::Vector4::new(axis.x, axis.y, axis.z, 0.0);
        return placed.w.abs() < 1e-12 && Vec3::new(placed.x, placed.y, placed.z).normalized().is_some_and(|a| a.cross(d).len() < 1e-9);
    }
    let dimensions = match surface {
        mt::Surface::BSplineSurface(s) => (s.control_points().len(), s.control_points().first().map_or(0, Vec::len)),
        mt::Surface::NurbsSurface(s) => (s.control_points().len(), s.control_points().first().map_or(0, Vec::len)),
        _ => return false,
    };
    let (rows, cols) = dimensions;
    let count = rows.saturating_mul(cols);
    let rectangular = match surface {
        mt::Surface::BSplineSurface(s) => s.control_points().iter().all(|r| r.len() == cols),
        mt::Surface::NurbsSurface(s) => s.control_points().iter().all(|r| r.len() == cols),
        _ => false,
    };
    if rows < 2 || cols < 2 || count > 16_384 || !rectangular {
        return false;
    }
    let Some(left) = remaining.checked_sub(count.saturating_mul(2)) else { return false };
    *remaining = left;
    let points: Vec<Vec<(Vec3, f64)>> = match surface {
        mt::Surface::BSplineSurface(s) => s.control_points().iter().map(|r| r.iter().map(|p| (from_p3(*p), 1.0)).collect()).collect(),
        mt::Surface::NurbsSurface(s) => {
            s.control_points().iter().map(|r| r.iter().map(|p| (Vec3::new(p.x / p.w, p.y / p.w, p.z / p.w), p.w)).collect()).collect()
        }
        _ => return false,
    };
    if points.iter().any(|r| r.len() != cols)
        || points.iter().flatten().any(|(p, w)| !w.is_finite() || *w <= 0.0 || [p.x, p.y, p.z].iter().any(|x| !x.is_finite()))
    {
        return false;
    }
    let max_weight = points.iter().flatten().map(|p| p.1).fold(0.0, f64::max);
    if points.iter().flatten().any(|p| p.1 / max_weight < 1e-12) {
        return false;
    }
    // Either parameter can be the extrusion direction. Projected positions must
    // be invariant in that parameter; rational weights must also separate into
    // a product of one factor per parameter, otherwise mixed weights bend the wall.
    for transpose in [false, true] {
        let (groups, length) = if transpose { (cols, rows) } else { (rows, cols) };
        let at = |i: usize, j: usize| if transpose { points[j][i] } else { points[i][j] };
        let mut valid = true;
        let mut extended = false;
        for i in 0..groups {
            let (base, base_w) = at(i, 0);
            for j in 0..length {
                let (p, w) = at(i, j);
                let delta = p - base;
                let magnitude = [p.x.abs(), p.y.abs(), p.z.abs(), base.x.abs(), base.y.abs(), base.z.abs()].into_iter().fold(1.0, f64::max);
                if delta.cross(d).len() > tol * 1e-3 + magnitude * f64::EPSILON * 128.0 {
                    valid = false;
                    break;
                }
                extended |= delta.dot(d).abs() > tol * 1e-3;
                let lhs = w / max_weight * (at(0, 0).1 / max_weight);
                let rhs = base_w / max_weight * (at(0, j).1 / max_weight);
                if (lhs - rhs).abs() > 1e-12 * lhs.abs().max(rhs.abs()) {
                    valid = false;
                    break;
                }
            }
            if !valid {
                break;
            }
        }
        if valid && extended {
            return true;
        }
    }
    false
}

/// The body as a prism along `d` (unit), seen in `plane` (normal `d`).
#[cfg(test)]
fn prism_of(b: &Body, d: Vec3, plane: &Plane, tol: f64) -> Option<Prism> {
    prism_tagged(b, d, plane, tol, &LazyIndex::new(b, 0))
}

/// [`prism_of`], its section's segments tagged from `index` (the body's).
fn prism_tagged(b: &Body, d: Vec3, plane: &Plane, tol: f64, index: &LazyIndex) -> Option<Prism> {
    // A simple extrusion with N section edges has N walls plus its cap faces.
    // Even disconnected two-arc sections need at most 2N faces. Keep that
    // complete planar arrangement envelope, independently of stepped limits.
    let mut remaining_faces = MAX_PLANAR_SEGMENTS * 2;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut tops: Vec<(usize, &mt::Face)> = Vec::new();
    let mut top_h = Vec::new();
    let mut bottom_h = Vec::new();
    let mut walls = std::collections::HashMap::new();
    let mut wall_work = 262_144;
    let mut curve_work = 2_000_000usize;
    for (fi, f) in b.solid.face_iter().enumerate() {
        remaining_faces = remaining_faces.checked_sub(1)?;
        curve_work = curve_work.checked_sub(1)?;
        let surf = f.oriented_surface();
        if let mt::Surface::Plane(p) = &surf {
            let n = p.normal();
            let n = Vec3::new(n.x, n.y, n.z).normalized()?;
            let h = plane.height(from_p3(p.origin()));
            if n.dot(d) > 1.0 - 1e-9 {
                tops.push((fi, f));
                top_h.push(h);
                hi = hi.max(h);
                continue;
            }
            if n.dot(d) < -1.0 + 1e-9 {
                bottom_h.push(h);
                lo = lo.min(h);
                continue;
            }
            if n.dot(d).abs() > 1e-9 {
                return None;
            }
            walls.insert(fi, top_of(f, plane));
            continue;
        }
        if !wall_along(&surf, d, tol, &mut wall_work) {
            return None;
        }
        walls.insert(fi, top_of(f, plane));
    }
    // One height for every top (and bottom) cap.
    if top_h.iter().any(|h| (h - hi).abs() > tol) || bottom_h.iter().any(|h| (h - lo).abs() > tol) || !(lo.is_finite() && hi > lo + tol) {
        return None;
    }
    let mut section = Section::default();
    let mut remaining_segments = MAX_PLANAR_SEGMENTS;
    let mut remaining_loops = MAX_PLANAR_SEGMENTS;
    for (fi, f) in tops {
        remaining_loops = remaining_loops.checked_sub(f.boundaries().len())?;
        let loops = index.get().loops(f, fi, plane, tol, &mut curve_work, &mut remaining_segments)?;
        let one = Section::from_loops(loops)?;
        section.regions.extend(one.regions);
        section.tags.extend(one.tags);
    }
    section.drop_crossed(&Stations::new(index.get(), walls, d, plane, tol), lo, hi, tol);
    Some(Prism { lo, hi, section })
}

/// The loop with the largest area is the outer one.
fn split_outer<T>(mut loops: Vec<(Loop2, T)>) -> Option<((Loop2, T), Vec<(Loop2, T)>)> {
    let i = (0..loops.len()).max_by(|a, b| loops[*a].0.signed_area().abs().total_cmp(&loops[*b].0.signed_area().abs()))?;
    let outer = loops.remove(i);
    Some((outer, loops))
}

fn heights_of(b: &Body, plane: &Plane) -> (f64, f64) {
    b.solid.vertex_iter().map(|v| plane.height(from_p3(v.point()))).fold((f64::INFINITY, f64::NEG_INFINITY), |m, h| (m.0.min(h), m.1.max(h)))
}

/// Does `s` run along `t` from end to end? Two lines, `s`'s ends on `t`; two arcs of one
/// circle, `s`'s ends and middle on `t`.
fn runs_along(s: &Seg2, t: &Seg2, tol: f64) -> bool {
    let on = |p: Vec2| t.point_at(t.closest_param(p)).dist(p) < tol;
    match (*s, *t) {
        (Seg2::Line { .. }, Seg2::Line { .. }) => on(s.start()) && on(s.end()),
        (Seg2::Arc { center: c, radius: r, .. }, Seg2::Arc { center: d, radius: q, .. }) => {
            c.dist(d) < tol && (r - q).abs() < tol && on(s.start()) && on(s.end()) && on(s.point_at(0.5))
        }
        _ => false,
    }
}

/// Points where two segments cross (parameters on each), lines and arcs only.
///
/// Curves that touch within the tolerance (a line tangent to an arc, two circles tangent inside
/// or outside) meet at one point, taken from the centres: the foot of the centre on the line,
/// or the point on the line of centres. The intersection formulas' two roots there are rounding
/// noise spread along the curves by its square root (a 1e-12 mm error in a distance puts them
/// ±2e-6 mm from a 2 mm arc's tangent point), close enough to both curves to pass for two
/// crossings; when the noise falls the other way there is no root and the contact is missed.
fn crossings(s: &Seg2, t: &Seg2, tol: f64) -> Vec<(f64, f64)> {
    let mut pts: Vec<Vec2> = Vec::new();
    match (*s, *t) {
        (Seg2::Line { a, b }, Seg2::Line { a: c, b: d }) => {
            let (r, q) = (b - a, d - c);
            let den = r.cross(q);
            // Lines lying on one another within the tolerance do not cross: their ends split
            // them. A hair from parallel they still meet at a point, anywhere along the overlap,
            // and a split there leaves a piece shorter than the tolerance or a vertex that
            // neither outline has.
            let off = |p: Vec2, o: Vec2, dir: Vec2| (p - o).cross(dir).abs() / dir.len().max(1e-300);
            let collinear = off(c, a, r) < tol && off(d, a, r) < tol || off(a, c, q) < tol && off(b, c, q) < tol;
            if den.abs() > 1e-300 && !collinear {
                let u = (c - a).cross(q) / den;
                pts.push(a + r * u);
            }
        }
        (Seg2::Line { a, b }, Seg2::Arc { center, radius, .. }) | (Seg2::Arc { center, radius, .. }, Seg2::Line { a, b }) => {
            let dir = b - a;
            let f = a - center;
            let (qa, qb, qc) = (dir.dot(dir), 2.0 * f.dot(dir), f.dot(f) - radius * radius);
            let disc = qb * qb - 4.0 * qa * qc;
            if qa > 1e-300 {
                let foot = a + dir * (-qb / (2.0 * qa));
                if (foot.dist(center) - radius).abs() < tol {
                    pts.push(foot);
                } else if disc >= -tol * tol * qa {
                    let sd = disc.max(0.0).sqrt();
                    for u in [(-qb - sd) / (2.0 * qa), (-qb + sd) / (2.0 * qa)] {
                        pts.push(a + dir * u);
                    }
                }
            }
        }
        (Seg2::Arc { center: c0, radius: r0, .. }, Seg2::Arc { center: c1, radius: r1, .. }) => {
            let dd = c0.dist(c1);
            if dd > 1e-12 && dd <= r0 + r1 + tol && dd >= (r0 - r1).abs() - tol {
                // Circles of nearly one radius about nearly one centre lie on each other for a
                // long way rather than touch at a point (centres 1000 tolerances apart: within
                // the tolerance of each other over 5° of arc): those keep the general formula.
                let inside = (dd - (r0 - r1).abs()).abs() < tol && dd > 1e3 * tol;
                if (dd - (r0 + r1)).abs() < tol || inside {
                    pts.push(c0 + (c1 - c0) * (if inside && r1 > r0 { -r0 } else { r0 } / dd));
                } else {
                    let a = (r0 * r0 - r1 * r1 + dd * dd) / (2.0 * dd);
                    let h = (r0 * r0 - a * a).max(0.0).sqrt();
                    let m = c0 + (c1 - c0) * (a / dd);
                    let perp = (c1 - c0).perp() * (1.0 / dd);
                    pts.push(m + perp * h);
                    pts.push(m - perp * h);
                }
            }
        }
        _ => {}
    }
    let mut out = Vec::new();
    for p in pts {
        let (u, v) = (s.closest_param(p), t.closest_param(p));
        if s.point_at(u).dist(p) < tol && t.point_at(v).dist(p) < tol {
            out.push((u, v));
        }
    }
    out
}

/// Ray parity against analytic lines/arcs. Fragment classification must not depend on a
/// display chord tolerance: a tiny tool can sit between a circular boundary and its chords.
/// Loops are closed only within the chaining tolerance (an arrangement's own output joins its
/// pieces within `10 · tol`), so a gap where one line ends and the next begins is closed by a
/// straight connector: a ray through the gap would otherwise slip out of the loop without a
/// crossing and flip the parity of points level with the corner. Arcs decide their own ends
/// within an angular tolerance (below), so joints with an arc keep that rule.
fn inside_loop(lp: &Loop2, p: Vec2) -> bool {
    // A ray through a vertex or grazing an arc's top or bottom is decided by the half-open
    // rule, which needs both curves meeting there to agree on that point exactly. Arcs read
    // back from a solid (a patterned hole's two halves) can disagree by a few ulps, so one half
    // counts the touch and the other misses it. Move the ray off such points: a shift far below
    // any modelling tolerance cannot change the answer for a point that is not on the loop.
    let scale = 1.0 + p.x.abs().max(p.y.abs());
    // A point on the loop keeps the half-open answer.
    if lp.segs.iter().any(|s| s.point_at(s.closest_param(p)).dist(p) < scale * 1e-9) {
        return inside_loop_ray(lp, p);
    }
    let grazes = |q: Vec2| {
        let near = |y: f64| (y - q.y).abs() < scale * 1e-9;
        lp.segs.iter().any(|s| {
            near(s.start().y)
                || match *s {
                    Seg2::Arc { center, radius, .. } => near(center.y + radius) || near(center.y - radius),
                    _ => false,
                }
        })
    };
    let ray = [0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0].into_iter().map(|k| Vec2::new(p.x, p.y + k * scale * 1e-7)).find(|q| !grazes(*q)).unwrap_or(p);
    inside_loop_ray(lp, ray)
}

/// [`inside_loop`] with the ray along +x from `p` taken as given.
fn inside_loop_ray(lp: &Loop2, p: Vec2) -> bool {
    let mut odd = false;
    let nexts = lp.segs.iter().skip(1).chain(lp.segs.first());
    for (seg, next) in lp.segs.iter().zip(nexts) {
        if let (Seg2::Line { b: a, .. }, Seg2::Line { a: b, .. }) = (*seg, *next)
            && (a.y > p.y) != (b.y > p.y)
            && a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) > p.x
        {
            odd = !odd;
        }
        match *seg {
            Seg2::Line { a, b } => {
                if (a.y > p.y) != (b.y > p.y) {
                    let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                    if x > p.x {
                        odd = !odd;
                    }
                }
            }
            Seg2::Arc { center, radius, start, sweep } => {
                let y = (p.y - center.y) / radius;
                if !(-1.0..=1.0).contains(&y) {
                    continue;
                }
                let a = y.asin();
                let root = (1.0 - y * y).max(0.0).sqrt();
                let full = (sweep.abs() - std::f64::consts::TAU).abs() < 1e-12;
                for (i, (angle, x)) in [(a, center.x + radius * root), (std::f64::consts::PI - a, center.x - radius * root)].into_iter().enumerate() {
                    // A tangent has one geometric root. Interior tangencies contribute zero
                    // parity; a minimum at an endpoint follows the same half-open rule as lines.
                    if root == 0.0 && i == 1 {
                        continue;
                    }
                    if x <= p.x {
                        continue;
                    }
                    let mut delta = if sweep > 0.0 { angle - start } else { start - angle };
                    delta = delta.rem_euclid(std::f64::consts::TAU);
                    if (delta - std::f64::consts::TAU).abs() < 1e-12 {
                        delta = 0.0;
                    }
                    if delta > sweep.abs() + 1e-12 {
                        continue;
                    }
                    let dy = angle.cos() * sweep;
                    let tangent = dy.abs() < sweep.abs() * 1e-12;
                    let at_start = delta < 1e-12;
                    let at_end = (delta - sweep.abs()).abs() < 1e-12;
                    let crosses = if full {
                        !tangent
                    } else if at_start {
                        dy > 0.0 && !tangent || tangent && y < 0.0
                    } else if at_end {
                        dy < 0.0 && !tangent || tangent && y < 0.0
                    } else {
                        !tangent
                    };
                    if crosses {
                        odd = !odd;
                    }
                }
            }
            _ => {
                // Private prism callers only supply lines/arcs. Keep a controlled fallback
                // for future internal callers; the public helper rejects other segment kinds.
                let points = seg.polyline(1e-7);
                for pair in points.windows(2) {
                    if let [a, b] = pair
                        && (a.y > p.y) != (b.y > p.y)
                        && a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) > p.x
                    {
                        odd = !odd;
                    }
                }
            }
        }
    }
    odd
}

fn inside(regions: &[Region2], p: Vec2) -> bool {
    regions.iter().any(|r| inside_loop(&r.outer, p) && !r.holes.iter().any(|h| inside_loop(h, p)))
}

fn dist_to(regions: &[Region2], p: Vec2) -> f64 {
    regions
        .iter()
        .flat_map(|r| std::iter::once(&r.outer).chain(&r.holes))
        .flat_map(|l| l.segs.iter())
        .map(|s| s.point_at(s.closest_param(p)).dist(p))
        .fold(f64::INFINITY, f64::min)
}

/// Direction of the boundary of `regions` through `p` (on it), as the boundary runs.
fn dir_at(regions: &[Region2], p: Vec2, tol: f64) -> Option<Vec2> {
    for r in regions {
        for l in std::iter::once(&r.outer).chain(&r.holes) {
            for s in &l.segs {
                let t = s.closest_param(p);
                if s.point_at(t).dist(p) < tol {
                    return s.derivative(t).normalized();
                }
            }
        }
    }
    None
}

/// 2D boolean of regions bounded by lines and arcs.
fn boolean2d(a: &[Region2], b: &[Region2], op: BoolOp, tol: f64) -> Option<Vec<Region2>> {
    boolean2d_tagged(&Section::untagged(a), &Section::untagged(b), op, tol).map(|s| s.regions)
}

/// [`boolean2d`] of tagged sections: each segment of the result tagged as the segment of `a` or
/// `b` it is (a piece of).
fn boolean2d_tagged(a: &Section, b: &Section, op: BoolOp, tol: f64) -> Option<Section> {
    let count =
        |rs: &[Region2]| rs.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).fold(0usize, |n, lp| n.saturating_add(lp.segs.len()));
    let (na, nb) = (count(&a.regions), count(&b.regions));
    if na.saturating_add(nb) > MAX_PLANAR_SEGMENTS || na.saturating_mul(nb) > MAX_PLANAR_PAIRS {
        return None;
    }
    let (a, b) = (a.oriented(), b.oriented());
    let ((sa, ta), (sb, tb)) = (a.segments(), b.segments());
    let (a, b) = (a.regions, b.regions);
    if sa.iter().chain(&sb).any(|s| !matches!(s, Seg2::Line { .. } | Seg2::Arc { .. })) {
        return None;
    }
    // Split every segment where it crosses the other side's boundary.
    let mut cut_a: Vec<Vec<f64>> = vec![Vec::new(); sa.len()];
    let mut cut_b: Vec<Vec<f64>> = vec![Vec::new(); sb.len()];
    let mut events = 0usize;
    for (i, s) in sa.iter().enumerate() {
        for (j, t) in sb.iter().enumerate() {
            for (u, v) in crossings(s, t, tol) {
                events += 2;
                if events > MAX_PLANAR_EVENTS {
                    return None;
                }
                cut_a.get_mut(i)?.push(u);
                cut_b.get_mut(j)?.push(v);
            }
        }
    }
    // Ends of the other side's segments lying on a segment split it too (touching corners).
    for (i, s) in sa.iter().enumerate() {
        for t in &sb {
            for p in [t.start(), t.end()] {
                let u = s.closest_param(p);
                if s.point_at(u).dist(p) < tol {
                    events += 1;
                    if events > MAX_PLANAR_EVENTS {
                        return None;
                    }
                    cut_a.get_mut(i)?.push(u);
                }
            }
        }
    }
    for (j, t) in sb.iter().enumerate() {
        for s in &sa {
            for p in [s.start(), s.end()] {
                let v = t.closest_param(p);
                if t.point_at(v).dist(p) < tol {
                    events += 1;
                    if events > MAX_PLANAR_EVENTS {
                        return None;
                    }
                    cut_b.get_mut(j)?.push(v);
                }
            }
        }
    }
    let pieces = |s: &Seg2, cuts: &mut Vec<f64>| -> Vec<Seg2> {
        cuts.sort_by(f64::total_cmp);
        let mut out = Vec::new();
        let mut rest = *s;
        let mut done = 0.0;
        for &c in cuts.iter() {
            if c <= done + 1e-9 || c >= 1.0 - 1e-9 {
                continue;
            }
            let local = (c - done) / (1.0 - done);
            let (x, y) = rest.split_at(local);
            if x.length() > tol {
                out.push(x);
                rest = y;
                done = c;
            }
        }
        out.push(rest);
        out
    };
    let mut kept: Vec<(Seg2, Tag)> = Vec::new();
    // A's pieces.
    for (i, s) in sa.iter().enumerate() {
        if cut_a.get(i)?.len() > MAX_PLANAR_SEGMENTS {
            return None;
        }
        let split = pieces(s, cut_a.get_mut(i)?);
        let (n, tag) = (split.len(), ta.get(i).copied().unwrap_or(BLANK));
        for (k, p) in split.into_iter().enumerate() {
            let m = p.point_at(0.5);
            let on = dist_to(&b, m) < tol;
            let keep = if on {
                let same = dir_at(&b, m, tol).is_some_and(|d| d.dot(p.derivative(0.5)) > 0.0);
                match op {
                    BoolOp::Union | BoolOp::Intersect => same,
                    BoolOp::Cut => !same,
                }
            } else {
                let ins = inside(&b, m);
                match op {
                    BoolOp::Union | BoolOp::Cut => !ins,
                    BoolOp::Intersect => ins,
                }
            };
            if keep {
                if kept.len() >= MAX_PLANAR_SEGMENTS {
                    return None;
                }
                kept.push((p, tag.piece(k, n)));
            }
        }
    }
    // B's pieces (coincident ones were decided with A's).
    for (j, t) in sb.iter().enumerate() {
        if cut_b.get(j)?.len() > MAX_PLANAR_SEGMENTS {
            return None;
        }
        let split = pieces(t, cut_b.get_mut(j)?);
        let (n, tag) = (split.len(), tb.get(j).copied().unwrap_or(BLANK));
        for (k, p) in split.into_iter().enumerate() {
            let m = p.point_at(0.5);
            if dist_to(&a, m) < tol {
                continue;
            }
            let ins = inside(&a, m);
            if kept.len() >= MAX_PLANAR_SEGMENTS {
                return None;
            }
            let tag = tag.piece(k, n);
            match op {
                BoolOp::Union if !ins => kept.push((p, tag)),
                BoolOp::Intersect if ins => kept.push((p, tag)),
                BoolOp::Cut if ins => kept.push((p.reversed(), tag.reversed())),
                _ => {}
            }
        }
    }
    // Chain into loops.
    let mut loops: Vec<(Loop2, Vec<Tag>)> = Vec::new();
    let mut used = vec![false; kept.len()];
    for start in 0..kept.len() {
        if used.get(start).copied().unwrap_or(true) {
            continue;
        }
        let (seg, tag) = *kept.get(start)?;
        let (mut segs, mut tags) = (vec![seg], vec![tag]);
        if let Some(u) = used.get_mut(start) {
            *u = true;
        }
        let first = seg.start();
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > kept.len() + 2 {
                return None;
            }
            let end = segs.last()?.end();
            let full_circle =
                segs.first().is_some_and(|s| matches!(s, Seg2::Arc { sweep, .. } if (sweep.abs() - std::f64::consts::TAU).abs() < 1e-9));
            if end.dist(first) < tol * 10.0 && (segs.len() > 1 || full_circle) {
                break;
            }
            let next = (0..kept.len())
                .filter(|k| !used.get(*k).copied().unwrap_or(true))
                .min_by(|x, y| kept[*x].0.start().dist(end).total_cmp(&kept[*y].0.start().dist(end)))?;
            let (seg, tag) = *kept.get(next)?;
            if seg.start().dist(end) > tol * 10.0 {
                if end.dist(first) < tol * 10.0 {
                    break;
                }
                return None;
            }
            if let Some(u) = used.get_mut(next) {
                *u = true;
            }
            segs.push(seg);
            tags.push(tag);
        }
        let l = Loop2 { segs };
        if l.signed_area().abs() > tol * tol {
            loops.push((l, tags));
        }
    }
    // Outers (counter-clockwise) take the holes they contain, the smallest first.
    let (mut outers, holes): (Vec<(Loop2, Vec<Tag>)>, Vec<(Loop2, Vec<Tag>)>) = loops.into_iter().partition(|(l, _)| l.signed_area() > 0.0);
    outers.sort_by(|x, y| x.0.signed_area().abs().total_cmp(&y.0.signed_area().abs()));
    let mut out = Section::default();
    for (o, tags) in outers {
        out.regions.push(Region2 { outer: o, holes: Vec::new() });
        out.tags.push(vec![tags]);
    }
    for (h, tags) in holes {
        let p = h.segs.first()?.point_at(0.5);
        let r = out.regions.iter().position(|r| inside_loop(&r.outer, p) && r.outer.signed_area().abs() > h.signed_area().abs())?;
        out.regions.get_mut(r)?.holes.push(h);
        out.tags.get_mut(r)?.push(tags);
    }
    Some(out)
}

/// The boolean done on prism sections, or `None` when it doesn't apply.
/// The pieces of a segment between the given points (those lying on it, inside it).
fn split_seg(seg: &Seg2, pts: &[Vec2], tol: f64) -> Option<Vec<Seg2>> {
    if !matches!(seg, Seg2::Line { .. } | Seg2::Arc { .. }) {
        return None;
    }
    let mut ts: Vec<f64> = pts
        .iter()
        .filter_map(|p| {
            // A point within `tol` of an arc is within `tol` of its circle: the others need no
            // (sampled) projection.
            if let Seg2::Arc { center, radius, .. } = seg
                && (p.dist(*center) - radius).abs() >= tol
            {
                return None;
            }
            let t = seg.closest_param(*p);
            (t > 1e-9 && t < 1.0 - 1e-9 && seg.point_at(t).dist(*p) < tol).then_some(t)
        })
        .collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let mut out = Vec::new();
    let mut t0 = 0.0;
    for t1 in ts.into_iter().chain(std::iter::once(1.0)) {
        out.push(match *seg {
            Seg2::Line { .. } => Seg2::Line { a: seg.point_at(t0), b: seg.point_at(t1) },
            Seg2::Arc { center, radius, start, sweep } => Seg2::Arc { center, radius, start: start + sweep * t0, sweep: sweep * (t1 - t0) },
            _ => return None,
        });
        t0 = t1;
    }
    Some(out)
}

/// The union of two prisms along `d` stacked end to end (or overlapping) with different
/// sections, sewn from its faces: each slab between the prisms' end levels has walls over its
/// section's outline; each level has a face up over the section below less the one above, and
/// a face down over the one above less the one below. Every outline is split where any other
/// meets it, so the faces share their edges. `None` when a slab is empty or a curve isn't a
/// line or an arc.
fn sew_stack(plane: &Plane, d: Vec3, pa: &Prism, pb: &Prism, tol: f64, history: &lineage::Context) -> Option<Result<Body>> {
    let mut levels = vec![pa.lo, pa.hi, pb.lo, pb.hi];
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|x, y| (*x - *y).abs() < tol);
    let covers = |p: &Prism, z0: f64, z1: f64| p.lo <= z0 + tol && p.hi >= z1 - tol;
    let mut slabs: Vec<Section> = Vec::new();
    for w in levels.windows(2) {
        let (z0, z1) = (*w.first()?, *w.get(1)?);
        let s = match (covers(pa, z0, z1), covers(pb, z0, z1)) {
            (true, true) => boolean2d_tagged(&pa.section, &pb.section, BoolOp::Union, tol)?,
            (true, false) => pa.section.clone(),
            (false, true) => pb.section.clone(),
            (false, false) => return None,
        };
        slabs.push(s.oriented());
    }
    sew_sections(plane, d, &levels, &slabs, tol, false, history)
}

/// Sew the exact sections between successive levels, including empty gaps. With `coalesce`,
/// walls over one segment through successive slabs are one face.
fn sew_sections(
    plane: &Plane,
    d: Vec3,
    levels: &[f64],
    slabs: &[Section],
    tol: f64,
    coalesce: bool,
    history: &lineage::Context,
) -> Option<Result<Body>> {
    if levels.len() != slabs.len() + 1
        || slabs.len() > 128
        || plane.normal().dot(d) < 1. - 1e-12
        || levels.iter().any(|z| !z.is_finite())
        || levels.windows(2).any(|z| z[0] >= z[1])
    {
        return None;
    }
    let mut blocks = Vec::new();
    let mut start = None;
    for (i, slab) in slabs.iter().enumerate() {
        if slab.regions.is_empty() {
            if let Some(first) = start.take() {
                blocks.push((first, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(first) = start {
        blocks.push((first, slabs.len()));
    }
    if blocks.is_empty() {
        return None;
    }
    Some((|| {
        let mut pieces = Vec::new();
        let mut remaining_constructed_faces = MAX_PLANAR_SEGMENTS * 2;
        let mut remaining_faces = MAX_PLANAR_SEGMENTS * 2;
        for (first, end) in blocks {
            let tagged = slabs.get(first).ok_or_else(|| KernelError::Failed("missing prism section".into()))?;
            let section = &tagged.regions;
            if slabs[first..end].iter().all(|s| s.regions == *section) {
                let edge_count = section.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).flat_map(|l| &l.segs).fold(0usize, |n, s| {
                    n.saturating_add(match s {
                        Seg2::Arc { sweep, .. } if sweep.abs() > 1.5 * std::f64::consts::PI => 2,
                        _ => 1,
                    })
                });
                remaining_constructed_faces = remaining_constructed_faces
                    .checked_sub(edge_count.saturating_add(section.len().saturating_mul(2)))
                    .ok_or_else(|| KernelError::Failed("prism construction face budget".into()))?;
                // This is a NEW exact extrusion of the current Boolean section,
                // not copied lineage from either operand's old footprint.
                for (r, piece) in crate::extrude(plane, section, levels[first], levels[end])?.into_iter().enumerate() {
                    remaining_faces =
                        remaining_faces.checked_sub(piece.face_count()).ok_or_else(|| KernelError::Failed("prism output face budget".into()))?;
                    pieces.push((piece, lineage::Built::Extruded { section: tagged.region(r), lo: levels[first], hi: levels[end] }));
                }
            } else {
                let (piece, built) = sew_connected_sections(plane, d, &levels[first..=end], &slabs[first..end], tol, coalesce, history)
                    .ok_or_else(|| KernelError::Failed("stepped prism construction failed".into()))??;
                remaining_faces =
                    remaining_faces.checked_sub(piece.face_count()).ok_or_else(|| KernelError::Failed("prism output face budget".into()))?;
                pieces.push((piece, built));
            }
        }
        let mut old = std::collections::HashMap::new();
        let mut shells = Vec::new();
        let mut remaining_map_faces = MAX_PLANAR_SEGMENTS * 2;
        for (piece, _) in &pieces {
            for (index, face) in piece.solid.face_iter().enumerate() {
                remaining_map_faces = remaining_map_faces.checked_sub(1).ok_or_else(|| KernelError::Failed("prism output face budget".into()))?;
                let source = piece.face_sources().and_then(|s| s.get(index)).and_then(Option::as_ref);
                if old.insert(face.id(), Some((face.orientation(), source))).is_some() {
                    old.insert(face.id(), None);
                }
            }
            shells.extend(piece.solid.boundaries().iter().cloned());
        }
        let body = Body::new(crate::body::Solid::try_new(shells).map_err(|e| KernelError::Failed(format!("prism compound: {e}")))?)?;
        let sources = body
            .solid
            .face_iter()
            .map(|face| match old.get(&face.id()) {
                Some(Some((orientation, source))) if *orientation == face.orientation() => source.cloned(),
                _ => None,
            })
            .collect();
        let body = body.with_face_sources(Some(std::sync::Arc::new(sources)));
        Ok(history.attach(body, &pieces))
    })())
}

/// Stable exact identity for the bounded analytic segment subset.
fn wall_key(s: &Seg2) -> Option<[u64; 6]> {
    let bits = |v: f64| v.is_finite().then(|| if v == 0. { 0 } else { v.to_bits() });
    match s {
        Seg2::Line { a, b } => Some([0, bits(a.x)?, bits(a.y)?, bits(b.x)?, bits(b.y)?, 0]),
        Seg2::Arc { center, radius, start, sweep } => Some([1, bits(center.x)?, bits(center.y)?, bits(*radius)?, bits(*start)?, bits(*sweep)?]),
        _ => None,
    }
}

/// A genuinely stepped connected axial block has no single extrusion footprint.
fn sew_connected_sections(
    plane: &Plane,
    d: Vec3,
    levels: &[f64],
    slabs: &[Section],
    tol: f64,
    coalesce: bool,
    history: &lineage::Context,
) -> Option<Result<(Body, lineage::Built)>> {
    use crate::sew::{EdgeSpec, FaceSpec, SurfSpec};
    // Faces at each level: up where the slab below sticks out, down where the one above does.
    let diff = |x: &Section, y: &Section| -> Option<Section> {
        match (x.regions.is_empty(), y.regions.is_empty()) {
            (true, _) => Some(Section::default()),
            (false, true) => Some(x.clone()),
            (false, false) => boolean2d_tagged(x, y, BoolOp::Cut, tol).map(|r| r.oriented()),
        }
    };
    let empty = Section::default();
    let mut caps: Vec<(f64, bool, Section)> = Vec::new();
    for (k, z) in levels.iter().enumerate() {
        let below: &Section = if k == 0 { &empty } else { slabs.get(k - 1)? };
        let above: &Section = slabs.get(k).unwrap_or(&empty);
        caps.push((*z, true, diff(below, above)?));
        caps.push((*z, false, diff(above, below)?));
    }
    // Every segment end, everywhere: outlines split there.
    let loops_of =
        |rs: &[Region2]| -> Vec<Loop2> { rs.iter().flat_map(|r| std::iter::once(r.outer.clone()).chain(r.holes.iter().cloned())).collect() };
    let mut pts: Vec<Vec2> = Vec::new();
    for lp in slabs.iter().flat_map(|s| loops_of(&s.regions)).chain(caps.iter().flat_map(|c| loops_of(&c.2.regions))) {
        for sg in &lp.segs {
            pts.push(sg.start());
            pts.push(sg.end());
        }
    }
    let at = |u: Vec2, z: f64| plane.to_world(u) + d * z;
    let edge = |sg: &Seg2, z: f64| -> EdgeSpec {
        match sg {
            Seg2::Arc { .. } => EdgeSpec::Arc { a: at(sg.start(), z), b: at(sg.end(), z), mid: at(sg.point_at(0.5), z) },
            _ => EdgeSpec::Line { a: at(sg.start(), z), b: at(sg.end(), z) },
        }
    };
    // Exact oriented generated segments alone define a run. No support fitting,
    // proximity matching, or old face lineage is used here.
    // Each run, with what its wall is over in each of its slabs (for the result's history).
    type Run = (usize, usize, Seg2, Vec<lineage::Slab>);
    let mut runs: Vec<Run> = Vec::new();
    let mut last = std::collections::BTreeMap::new();
    let mut remaining = MAX_PLANAR_SEGMENTS * 2;
    for (i, slab) in slabs.iter().enumerate() {
        for (r, reg) in slab.regions.iter().enumerate() {
            for (l, lp) in std::iter::once(&reg.outer).chain(&reg.holes).enumerate() {
                // The loop's walls in order: the run each joins, its tag and where it ends.
                let mut around: Vec<(usize, Tag, Vec2)> = Vec::new();
                for (si, sg0) in lp.segs.iter().enumerate() {
                    let pieces = split_seg(sg0, &pts, tol)?;
                    let (n, tag) = (pieces.len(), slab.tag(r, l, si));
                    for (k, sg) in pieces.into_iter().enumerate() {
                        remaining = remaining.checked_sub(1)?;
                        let key = wall_key(&sg)?;
                        let prior = if coalesce { last.get(&key).copied() } else { None };
                        let run = match prior.map(|index| runs.get_mut(index).map(|run: &mut Run| (index, run))) {
                            Some(None) => return None,
                            Some(Some((index, run))) if run.1 == i => {
                                run.1 = i + 1;
                                index
                            }
                            // Duplicate oriented boundaries in one slab are not
                            // a valid reason to reconstruct a larger wall.
                            Some(Some((_, run))) if run.1 == i + 1 => return None,
                            _ => {
                                if coalesce {
                                    last.insert(key, runs.len());
                                }
                                runs.push((i, i + 1, sg, Vec::new()));
                                runs.len() - 1
                            }
                        };
                        around.push((run, tag.piece(k, n), sg.end()));
                    }
                }
                // Each wall runs into the next one of its loop, the last into the first.
                for (k, (run, tag, at)) in around.iter().enumerate() {
                    let next = around.get((k + 1) % around.len())?.0;
                    runs.get_mut(*run)?.3.push(lineage::Slab { tag: *tag, next, at: *at });
                }
            }
        }
    }
    let mut faces: Vec<FaceSpec> = Vec::new();
    let mut sources = Vec::new();
    let mut specs: Vec<lineage::Spec> = Vec::with_capacity(runs.len());
    let frame_is_orthonormal = plane.origin.is_finite()
        && plane.x.is_finite()
        && plane.y.is_finite()
        && d.is_finite()
        && (plane.x.len2() - 1.).abs() <= 64. * f64::EPSILON
        && (plane.y.len2() - 1.).abs() <= 64. * f64::EPSILON
        && (d.len2() - 1.).abs() <= 64. * f64::EPSILON
        && plane.x.dot(plane.y).abs() <= 64. * f64::EPSILON
        && plane.x.dot(d).abs() <= 64. * f64::EPSILON
        && plane.y.dot(d).abs() <= 64. * f64::EPSILON;
    for (first, end, sg, over) in runs {
        let (z0, z1) = (levels[first], levels[end]);
        let (a, b) = (sg.start(), sg.end());
        let mut lp3 = Vec::with_capacity(2 + 2 * (end - first));
        lp3.push(edge(&sg, z0));
        // Keep station vertices at both ends: neighboring stepped walls/caps
        // must share complete edges rather than create T-junctions.
        for k in first..end {
            lp3.push(EdgeSpec::Line { a: at(b, levels[k]), b: at(b, levels[k + 1]) });
        }
        lp3.push(edge(&sg.reversed(), z1));
        for k in (first..end).rev() {
            lp3.push(EdgeSpec::Line { a: at(a, levels[k + 1]), b: at(a, levels[k]) });
        }
        let outward = plane.dir_to_world(sg.derivative(0.5)).cross(d).normalized()?;
        let surface = match sg {
            Seg2::Arc { .. } => SurfSpec::Extruded { a: at(a, z0), b: at(b, z0), mid: at(sg.point_at(0.5), z0), dir: d * (z1 - z0) },
            _ => SurfSpec::Plane,
        };
        faces.push(FaceSpec { loops: vec![lp3], surface, probe: Some((at(sg.point_at(0.5), (z0 + z1) / 2.0), outward)) });
        // Axis-aligned generated lines have a common oriented support frame
        // independent of axial stations or along-line endpoint interpolation.
        let generated_plane = if coalesce
            && frame_is_orthonormal
            && let Seg2::Line { a, b } = sg
        {
            let (origin, x) = if a.y == b.y && a.x != b.x {
                (at(Vec2::new(0., a.y), 0.), plane.x * (b.x - a.x).signum())
            } else if a.x == b.x && a.y != b.y {
                (at(Vec2::new(a.x, 0.), 0.), plane.y * (b.y - a.y).signum())
            } else {
                (Vec3::ZERO, Vec3::ZERO)
            };
            (origin.is_finite() && x.len2() > 0.).then_some(Plane { origin, x, y: d })
        } else {
            None
        };
        sources.push((generated_plane.is_some() || end > first + 1).then_some(crate::FaceSource::ExtrudeSide {
            segment: sg,
            plane: generated_plane,
            coalesced: end > first + 1,
        }));
        specs.push(lineage::Spec::Wall { levels: levels.get(first..=end)?.to_vec(), slabs: over });
    }
    for (z, up, rs) in &caps {
        for (ri, r) in rs.regions.iter().enumerate() {
            let mut loops = Vec::new();
            for lp in std::iter::once(&r.outer).chain(&r.holes) {
                let mut segs: Vec<Seg2> = Vec::new();
                for sg in &lp.segs {
                    segs.extend(split_seg(sg, &pts, tol)?);
                }
                let segs: Vec<Seg2> = if *up { segs } else { segs.iter().rev().map(Seg2::reversed).collect() };
                loops.push(segs.iter().map(|sg| edge(sg, *z)).collect::<Vec<_>>());
            }
            faces.push(FaceSpec { loops, surface: SurfSpec::Plane, probe: None });
            sources.push(None);
            specs.push(lineage::Spec::Cap { section: rs.region(ri), z: *z, up: *up });
        }
    }
    let sewn = crate::sew::sew_touching_with_sources(&faces, &sources).map(|(body, from)| (body, lineage::Built::Sewn { specs, faces: from }));
    // Generated walls joined on their common supports: the piece's history over the operands,
    // carried through the joining.
    Some(if coalesce {
        sewn.and_then(|(body, built)| {
            let body = history.attach(body.clone(), &[(&body, built)]).regularize_generated_planar_faces()?;
            Ok((body, lineage::Built::Traced))
        })
    } else {
        sewn
    })
}

#[cfg(test)]
fn simple_prism_boolean(a: &Body, b: &Body, op: BoolOp) -> Option<Result<Option<Body>>> {
    simple_prism_indexed(a, b, op, [&LazyIndex::new(a, 0), &LazyIndex::new(b, 1)])
}

/// [`prism_boolean`] of single prisms, the operands' indexes given.
fn simple_prism_indexed(a: &Body, b: &Body, op: BoolOp, [ia, ib]: [&LazyIndex; 2]) -> Option<Result<Option<Body>>> {
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9) * 10.0;
    // Candidate directions: a's planar face normals (each line once).
    let mut dirs: Vec<Vec3> = Vec::new();
    let mut remaining_direction_faces = MAX_PLANAR_SEGMENTS * 2;
    for f in a.solid.face_iter() {
        remaining_direction_faces = remaining_direction_faces.checked_sub(1)?;
        if let mt::Surface::Plane(p) = f.oriented_surface() {
            let n = p.normal();
            if let Some(n) = Vec3::new(n.x, n.y, n.z).normalized()
                && !dirs.iter().any(|m| m.dot(n).abs() > 1.0 - 1e-9)
            {
                dirs.push(if n.dot(Vec3::new(1e-3, 1e-6, 1.0)) < 0.0 { -n } else { n });
                if dirs.len() == 8 {
                    break;
                }
            }
        }
    }
    let (d, plane, pa, pb) = dirs.into_iter().take(8).find_map(|d| {
        let plane = Plane::from_normal(Vec3::ZERO, d)?;
        let pa = prism_tagged(a, d, &plane, tol, ia)?;
        let pb = prism_tagged(b, d, &plane, tol, ib)?;
        Some((d, plane, pa, pb))
    })?;
    let history = lineage::Context::new(op, [a, b], [ia.get(), ib.get()], plane, tol, Some([(pa.lo, pa.hi), (pb.lo, pb.hi)]));
    let (blo, bhi) = heights_of(b, &plane);
    let spans = blo <= pa.lo + tol && bhi >= pa.hi - tol;
    let same = (pb.lo - pa.lo).abs() < tol && (pb.hi - pa.hi).abs() < tol;
    let area =
        |rs: &[Region2]| rs.iter().map(|r| r.outer.signed_area().abs() - r.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>()).sum::<f64>();
    // Prisms of one section overlapping or touching end to end: one longer prism.
    let stacked = op == BoolOp::Union && !same && pb.lo <= pa.hi + tol && pa.lo <= pb.hi + tol;
    let ok = match op {
        BoolOp::Cut | BoolOp::Intersect => spans,
        BoolOp::Union => same || stacked,
    };
    if !ok {
        return None;
    }
    let section = boolean2d_tagged(&pa.section, &pb.section, op, tol)?;
    let regions = &section.regions;
    if regions.is_empty() {
        return Some(Ok(None));
    }
    let (lo, hi) = if stacked {
        // Different sections (a boss hanging over a plate's edge): sewn slab by slab.
        let (u, x, y) = (area(regions), area(&pa.section.regions), area(&pb.section.regions));
        if (u - x).abs() > tol * (1.0 + u) || (u - y).abs() > tol * (1.0 + u) {
            return sew_stack(&plane, d, &pa, &pb, tol, &history).map(|r| r.map(Some));
        }
        (pa.lo.min(pb.lo), pa.hi.max(pb.hi))
    } else {
        (pa.lo, pa.hi)
    };
    let base = plane.offset(lo);
    let bodies = match crate::extrude(&base, regions, 0.0, hi - lo) {
        Ok(b) => b,
        Err(e) => return Some(Err(e)),
    };
    // Each body over its region.
    let mut pieces: Vec<(Body, lineage::Built)> =
        bodies.into_iter().enumerate().map(|(r, b)| (b, lineage::Built::Extruded { section: section.region(r), lo, hi })).collect();
    let last = pieces.pop()?;
    if !pieces.is_empty() {
        // Several pieces: one body of several lumps (the last first).
        pieces.push(last);
        pieces.rotate_right(1);
        let shells: Vec<mt::Shell> = pieces.iter().flat_map(|(b, _)| b.solid.boundaries().iter().cloned()).collect();
        return Some(
            crate::body::Solid::try_new(shells)
                .map_err(|e| crate::KernelError::Failed(e.to_string()))
                .and_then(Body::new)
                .map(|body| Some(history.attach(body, &pieces))),
        );
    }
    Some(Ok(Some(history.attach(last.0.clone(), &[last]))))
}

/// A stepped extrusion (case floor plus walls), recovered from its cap events.
/// Downward caps add a section above their level; upward caps remove one.
#[cfg(test)]
fn stack_of(b: &Body, d: Vec3, plane: &Plane, tol: f64) -> Option<Vec<Prism>> {
    stack_tagged(b, d, plane, tol, &LazyIndex::new(b, 0))
}

/// [`stack_of`], its sections' segments tagged from `index` (the body's).
fn stack_tagged(b: &Body, d: Vec3, plane: &Plane, tol: f64, index: &LazyIndex) -> Option<Vec<Prism>> {
    if let Some(p) = prism_tagged(b, d, plane, tol, index) {
        return Some(vec![p]);
    }
    if b.face_count() > 2048 {
        return None;
    }
    let mut events = Vec::new();
    let mut walls = std::collections::HashMap::new();
    let mut wall_work = 262_144;
    let mut curve_work = 2_000_000;
    let mut segments = usize::MAX;
    for (fi, f) in b.solid.face_iter().enumerate() {
        let surf = f.oriented_surface();
        if let mt::Surface::Plane(p) = &surf {
            let n = p.normal();
            let n = Vec3::new(n.x, n.y, n.z).normalized()?;
            if n.dot(d).abs() > 1.0 - 1e-9 {
                let section = Section::from_loops(index.get().loops(f, fi, plane, tol, &mut curve_work, &mut segments)?)?;
                events.push((plane.height(from_p3(p.origin())), n.dot(d) > 0.0, section.oriented(), fi));
                continue;
            }
            if n.dot(d).abs() > 1e-9 {
                return None;
            }
        } else if !wall_along(&surf, d, tol, &mut wall_work) {
            return None;
        }
        walls.insert(fi, top_of(f, plane));
    }
    let mut levels: Vec<f64> = events.iter().map(|(z, _, _, _)| *z).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|x, y| (*x - *y).abs() < tol);
    if levels.len() < 2 || levels.len() > 64 || levels.iter().any(|z| !z.is_finite()) {
        return None;
    }
    let stations = Stations::new(index.get(), walls, d, plane, tol);
    let mut current = Section::default();
    let mut stack: Vec<Prism> = Vec::new();
    let mut budget = MAX_PLANAR_PAIRS;
    for (i, z) in levels.iter().enumerate() {
        // The caps here end the slab below: its walls meet them.
        if let Some(below) = stack.last_mut().filter(|p| (p.hi - *z).abs() < tol) {
            let (faces, caps): (Vec<usize>, Vec<&Section>) =
                events.iter().filter(|(h, _, _, _)| (*h - *z).abs() < tol).map(|(_, _, s, f)| (*f, s)).unzip();
            below.section.mark_far(&caps, |w| stations.top_edge(index.get(), w, &faces, *z, tol), tol, &mut budget);
        }
        // The segments carried here run on past it, beside the walls there.
        current.retag(|t| match t.wall {
            Some(w) => t.beside(stations.past(w, *z, tol)),
            None => t,
        });
        // Remove terminating material before adding any material starting here.
        for up in [true, false] {
            for (_, _, section, _) in events.iter().filter(|(h, u, _, _)| (*h - *z).abs() < tol && *u == up) {
                current = match (current.regions.is_empty(), up) {
                    (true, false) => section.clone(),
                    (true, true) => return None,
                    (false, _) => boolean2d_tagged(&current, section, if up { BoolOp::Cut } else { BoolOp::Union }, tol)?,
                };
            }
        }
        if let Some(next) = levels.get(i + 1)
            && !current.regions.is_empty()
        {
            current.drop_crossed(&stations, *z, *next, tol);
            stack.push(Prism { lo: *z, hi: *next, section: current.oriented() });
        }
    }
    (current.regions.is_empty() && !stack.is_empty()).then_some(stack)
}

#[cfg(test)]
fn stack_boolean(a: &Body, b: &Body, op: BoolOp) -> Option<Result<Option<Body>>> {
    stack_indexed(a, b, op, [&LazyIndex::new(a, 0), &LazyIndex::new(b, 1)])
}

/// [`prism_boolean`] of stepped prisms, the operands' indexes given.
fn stack_indexed(a: &Body, b: &Body, op: BoolOp, [ia, ib]: [&LazyIndex; 2]) -> Option<Result<Option<Body>>> {
    // Common-cap joins and overlapping partial-depth cuts of aligned stepped prisms can
    // retain their exact section boundaries instead of intersecting meshes.
    // Nonaligned bodies retain their existing B-rep path.
    if !matches!(op, BoolOp::Union | BoolOp::Cut) {
        return None;
    }
    if op == BoolOp::Cut && a.face_count().saturating_add(b.face_count()) > 2048 {
        return None;
    }
    fn section(ps: &[Prism], lo: f64, hi: f64, tol: f64) -> Section {
        ps.iter().find(|p| p.lo <= lo + tol && p.hi >= hi - tol).map(|p| p.section.clone()).unwrap_or_default()
    }
    let tol = (a.size().max(b.size()) * 1e-7).max(1e-9) * 10.0;
    let mut dirs: Vec<Vec3> = Vec::new();
    for f in a.solid.face_iter() {
        if let mt::Surface::Plane(p) = f.oriented_surface() {
            let n = p.normal();
            if let Some(n) = Vec3::new(n.x, n.y, n.z).normalized()
                && !dirs.iter().any(|m| m.dot(n).abs() > 1.0 - 1e-9)
            {
                dirs.push(n);
            }
        }
    }
    let ends = |ps: &[Prism]| ps.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.lo), hi.max(p.hi)));
    let (d, plane, pa, pb) = dirs.into_iter().take(8).find_map(|d| {
        let plane = Plane::from_normal(Vec3::ZERO, d)?;
        let pa = stack_tagged(a, d, &plane, tol, ia)?;
        let pb = stack_tagged(b, d, &plane, tol, ib)?;
        let (ea, eb) = (ends(&pa), ends(&pb));
        match op {
            BoolOp::Union if (ea.0 - eb.0).abs() >= tol && (ea.1 - eb.1).abs() >= tol => return None,
            // A cut tool may extend past a cap into empty space. Exact slab sections
            // discard that space rather than intersecting coincident cylinder walls.
            BoolOp::Cut if ea.1 <= eb.0 + tol || eb.1 <= ea.0 + tol => return None,
            _ => {}
        }
        Some((d, plane, pa, pb))
    })?;
    let mut levels: Vec<f64> = pa.iter().chain(&pb).flat_map(|p| [p.lo, p.hi]).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|x, y| (*x - *y).abs() < tol);
    let mut slabs = Vec::new();
    let mut work = 0usize;
    let mut output_edges = 0usize;
    let edges = |regions: &[Region2]| regions.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).map(|l| l.segs.len()).sum::<usize>();
    for w in levels.windows(2) {
        let (lo, hi) = (*w.first()?, *w.get(1)?);
        let (x, y) = (section(&pa, lo, hi, tol), section(&pb, lo, hi, tol));
        if op == BoolOp::Cut {
            let count = edges(&x.regions).saturating_add(edges(&y.regions));
            work = work.saturating_add(count.saturating_mul(count));
            if work > 32_000_000 {
                return None;
            }
        }
        let s = match (x.regions.is_empty(), y.regions.is_empty(), op) {
            (true, _, BoolOp::Cut | BoolOp::Intersect) | (_, true, BoolOp::Intersect) => Section::default(),
            (true, _, BoolOp::Union) => y,
            (_, true, _) => x,
            _ => boolean2d_tagged(&x, &y, op, tol)?,
        };
        if op == BoolOp::Cut {
            output_edges = output_edges.saturating_add(edges(&s.regions));
            if output_edges > 8192 {
                return None;
            }
        }
        slabs.push(s.oriented());
    }
    if slabs.iter().all(|s| s.regions.is_empty()) {
        return Some(Ok(None));
    }
    let history = lineage::Context::new(op, [a, b], [ia.get(), ib.get()], plane, tol, None);
    let coalesce = op == BoolOp::Cut
        && !a.regularization_protected
        && !b.regularization_protected
        && a.split_keep().is_empty()
        && b.split_keep().is_empty()
        && a.paint().is_none_or(|p| p.faces.is_empty())
        && b.paint().is_none_or(|p| p.faces.is_empty());
    sew_sections(&plane, d, &levels, &slabs, tol, coalesce, &history).map(|r| r.map(Some))
}

/// Boolean of prisms along a common direction, in 2D (see the module notes); `None` when it
/// doesn't apply. The result carries its history over `[a, b]` (see [`lineage`]).
pub(crate) fn prism_boolean(a: &Body, b: &Body, op: BoolOp) -> Option<Result<Option<Body>>> {
    // Each operand's index is built once, and only if it is read as a prism.
    let index = [&LazyIndex::new(a, 0), &LazyIndex::new(b, 1)];
    crate::guard("prism boolean", || Ok(simple_prism_indexed(a, b, op, index).or_else(|| stack_indexed(a, b, op, index)))).ok().flatten()
}

#[cfg(test)]
mod planar_tests {
    use super::*;
    fn rectangle(a: Vec2, b: Vec2) -> Region2 {
        Region2 { outer: Loop2::polygon(&[a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)]), holes: vec![] }
    }
    #[test]
    fn simple_prism_retains_large_analytic_section_budget() {
        let count = 2050;
        let radius = 1000.;
        let points: Vec<_> = (0..count)
            .map(|i| {
                let angle = std::f64::consts::TAU * i as f64 / count as f64;
                Vec2::new(radius * angle.cos(), radius * angle.sin())
            })
            .collect();
        let region = Region2 { outer: Loop2::polygon(&points), holes: vec![] };
        let stock = crate::extrude(&Plane::XY, &[region], 0., 2.).unwrap().pop().unwrap();
        assert_eq!(stock.face_count(), count + 2);
        assert!(stock.face_count() > 2048 && stock.validity().is_empty());
        let before: Vec<_> = stock.solid.vertex_iter().map(|v| (v.id(), v.point())).collect();
        let recovered = prism_of(&stock, Vec3::Z, &Plane::XY, 1e-5).expect("large simple line prism remains supported");
        assert_eq!(recovered.section.regions.len(), 1);
        assert_eq!(recovered.section.regions[0].outer.segs.len(), count);
        assert!(recovered.section.regions[0].outer.segs.iter().all(|s| matches!(s, Seg2::Line { .. })));
        let area = count as f64 * radius * radius * (std::f64::consts::TAU / count as f64).sin() / 2.;
        assert!((recovered.section.regions[0].area() - area).abs() < area * 1e-12);
        assert!((recovered.lo - 0.).abs() < 1e-8 && (recovered.hi - 2.).abs() < 1e-8);
        let cutter = crate::box_solid(Vec3::new(-1001., -1001., -1.), Vec3::new(1001., 1001., 3.)).unwrap();
        assert!(
            simple_prism_boolean(&stock, &cutter, BoolOp::Cut)
                .expect("large analytic section routes through the exact simple path")
                .unwrap()
                .is_none()
        );
        assert_eq!(stock.solid.vertex_iter().map(|v| (v.id(), v.point())).collect::<Vec<_>>(), before);
    }
    #[test]
    fn curved_stop_cut_constructs_fresh_current_cap_for_only_uniform_lump() {
        let cylinder = crate::cylinder(Vec3::new(25., 25., 0.), Vec3::Z, 2., 20.).unwrap();
        let stop = crate::box_solid(Vec3::new(19., 19., 1.), Vec3::new(31., 31., 2.)).unwrap();
        let roof = crate::boolean(&cylinder, &stop, BoolOp::Union).unwrap().unwrap();
        let original = rectangle(Vec2::new(20., 20.), Vec2::new(30., 30.));
        let tool = crate::extrude(&Plane::XY, &[original], 0., 25.).unwrap().pop().unwrap();
        let before_tool = crate::step_export(&[&tool], "tool").unwrap();
        let before_roof = crate::step_export(&[&roof], "roof").unwrap();
        let trimmed = crate::boolean(&tool, &roof, BoolOp::Cut).unwrap().unwrap();
        let lumps = trimmed.lumps().unwrap();
        assert_eq!(lumps.len(), 2);
        let mut lower = 0;
        let mut upper = 0;
        for lump in &lumps {
            assert!(lump.validity().is_empty());
            let mesh = lump.tessellate(1e-5).unwrap();
            if mesh.contains(Vec3::new(21., 21., 0.5)) {
                lower += 1;
                assert!(!mesh.contains(Vec3::new(25., 25., 0.5)));
                assert!(!mesh.contains(Vec3::new(21., 21., 1.5)));
                assert!((mesh.measure().volume - (100. - 4. * std::f64::consts::PI)).abs() < 0.002);
                let sources = lump.face_sources().expect("fresh actual extrusion has complete sources");
                assert_eq!(sources.len(), lump.face_count());
                assert!(sources.iter().all(Option::is_some));
                let mut caps = 0;
                for source in sources.iter().flatten() {
                    match source {
                        crate::FaceSource::ExtrudeCap { region, plane, .. } => {
                            assert_eq!(region.holes.len(), 1);
                            assert!((region.area() - (100. - 4. * std::f64::consts::PI)).abs() < 1e-8);
                            assert!(plane.normal().z.abs() > 1. - 1e-10);
                            assert!((plane.origin.z - if plane.normal().z > 0. { 1. } else { 0. }).abs() < 1e-8);
                            caps += 1;
                        }
                        crate::FaceSource::ExtrudeSide { .. } => {}
                        _ => panic!("old producer lineage must not survive a changed Boolean footprint"),
                    }
                }
                assert_eq!(caps, 2);
            } else {
                upper += 1;
                assert!(mesh.contains(Vec3::new(21., 21., 3.)));
                assert!(!mesh.contains(Vec3::new(25., 25., 3.)));
                assert!(mesh.contains(Vec3::new(25., 25., 22.)));
                assert!(
                    lump.face_sources().is_none_or(|s| s
                        .iter()
                        .all(|s| s.is_none() || matches!(s, Some(crate::FaceSource::ExtrudeSide { coalesced: true, plane: None, .. })))),
                    "genuinely stepped geometry has no durable certificate"
                );
                assert!((mesh.measure().volume - (18. * (100. - 4. * std::f64::consts::PI) + 500.)).abs() < 0.02);
            }
        }
        assert_eq!((lower, upper), (1, 1));
        assert_eq!(crate::step_export(&[&tool], "tool").unwrap(), before_tool);
        assert_eq!(crate::step_export(&[&roof], "roof").unwrap(), before_roof);
    }
    fn stepped_tab_stock() -> Body {
        let stock = crate::box_solid(Vec3::new(-20., -15., 0.), Vec3::new(20., 15., 24.)).unwrap();
        let tab = crate::box_solid(Vec3::new(-12., -25., 8.), Vec3::new(-4., -15., 16.)).unwrap();
        crate::boolean(&stock, &tab, BoolOp::Union).unwrap().unwrap()
    }

    fn coplanar_joins(body: &Body) -> usize {
        let faces: Vec<_> = body.solid.face_iter().collect();
        let mut adjacency = std::collections::HashMap::<_, Vec<usize>>::new();
        for (i, face) in faces.iter().enumerate() {
            for edge in face.edge_iter() {
                adjacency.entry(edge.id()).or_default().push(i);
            }
        }
        adjacency
            .values()
            .filter(|faces| {
                let [a, b] = faces.as_slice() else { return false };
                let (Some(pa), Some(pb)) = (body.planar_face(*a), body.planar_face(*b)) else { return false };
                let (na, nb) = (pa.normal(), pb.normal());
                na.dot(nb) > 1. - 1e-12 && (pb.origin - pa.origin).dot(na).abs() < 1e-10
            })
            .count()
    }

    #[test]
    fn shell_of_joined_stepped_tabs_has_no_artificial_planar_stations() {
        let stock = stepped_tab_stock();
        let snapshot = crate::step_export(&[&stock], "unchanged stock").unwrap();
        assert_eq!(coplanar_joins(&stock), 0);
        let shell = crate::shell(&stock, &[Vec3::new(0., 0., 24.)], 1.).unwrap();
        assert_eq!(coplanar_joins(&shell), 0, "Shell must not add internal edges across continuous planar walls");
        assert!(shell.validity().is_empty());
        let mesh = shell.tessellate(1e-5).unwrap();
        let expected = 40. * 30. * 24. + 8. * 10. * 8. - (38. * 28. * 23. + 6. * 10. * 6.);
        assert!((mesh.measure().volume - expected).abs() < 1e-6);
        for (point, material) in [
            (Vec3::new(0., 0., 0.5), true),
            (Vec3::new(0., 0., 10.), false),
            (Vec3::new(0., -14.5, 10.), true),
            (Vec3::new(19.5, 0., 10.), true),
            (Vec3::new(-8., -24.5, 12.), true),
            (Vec3::new(-8., -20., 12.), false),
            (Vec3::new(0., -18., 9.), false),
        ] {
            assert_eq!(mesh.contains(point), material, "wrong material at {point:?}");
        }
        let step = crate::step_export(&[&shell], "stepped tab shell").unwrap();
        let reopened = crate::step_import(&step).unwrap();
        let body = &reopened.bodies[0].body;
        assert_eq!(body.face_count(), shell.face_count());
        assert_eq!(coplanar_joins(body), 0);
        assert!((body.tessellate(1e-5).unwrap().measure().volume - expected).abs() < 1e-6);
        assert_eq!(crate::step_export(&[&stock], "unchanged stock").unwrap(), snapshot);
    }

    #[test]
    fn contiguous_analytic_walls_remove_faces_but_retain_vertical_stations() {
        for curved in [false, true] {
            let outer = if curved {
                Region2 { outer: Loop2::circle(Vec2::ZERO, 10.), holes: vec![] }
            } else {
                rectangle(Vec2::new(-10., -10.), Vec2::new(10., 10.))
            };
            let mut upper = outer.clone();
            upper.holes.push(if curved {
                Loop2::circle(Vec2::ZERO, 5.).reversed()
            } else {
                rectangle(Vec2::new(-5., -5.), Vec2::new(5., 5.)).outer.reversed()
            });
            let slabs = vec![Section::untagged(&[outer]), Section::untagged(&[upper])];
            // Sections nothing is known of: no history, the construction alone is under test.
            let operand = crate::extrude(&Plane::XY, &[rectangle(Vec2::new(-1., -1.), Vec2::new(1., 1.))], 0., 1.).unwrap().pop().unwrap();
            let index = LazyIndex::new(&operand, 0);
            let history = lineage::Context::new(BoolOp::Cut, [&operand, &operand], [index.get(), index.get()], Plane::XY, 1e-7, None);
            let old = sew_sections(&Plane::XY, Vec3::Z, &[0., 2., 20.], &slabs, 1e-7, false, &history).unwrap().unwrap();
            let merged = sew_sections(&Plane::XY, Vec3::Z, &[0., 2., 20.], &slabs, 1e-7, true, &history).unwrap().unwrap();
            let count = if curved { 2 } else { 4 };
            assert_eq!(old.face_count() - merged.face_count(), count);
            let marked: Vec<_> = merged.solid.face_iter().enumerate().filter(|(i, _)| merged.face_was_coalesced(*i)).collect();
            assert_eq!(marked.len(), count);
            for (_, face) in marked {
                assert_eq!(face.boundaries()[0].len(), 6, "all axial station vertices are retained");
                let heights: Vec<_> = face.vertex_iter().map(|v| v.point().z).collect();
                for z in [0., 2., 20.] {
                    assert!(heights.contains(&z));
                }
            }
            assert!(old.face_sources().is_none_or(|s| s.iter().all(Option::is_none)));
            let expected = if curved { std::f64::consts::PI * (100. * 20. - 25. * 18.) } else { 400. * 20. - 100. * 18. };
            let mesh = merged.tessellate(1e-5).unwrap();
            assert!((mesh.measure().volume - expected).abs() < 0.03);
            assert!(mesh.contains(Vec3::new(0., 0., 1.)));
            assert!(!mesh.contains(Vec3::new(0., 0., 10.)));
            assert!(merged.validity().is_empty());
            assert_ne!(wall_key(&slabs[0].regions[0].outer.segs[0]), wall_key(&slabs[0].regions[0].outer.segs[0].reversed()));
        }
    }

    #[test]
    fn cut_run_coalescing_respects_authored_splits_and_face_paint() {
        let region = Region2 { outer: Loop2::circle(Vec2::ZERO, 10.), holes: vec![] };
        let tool_region = Region2 { outer: Loop2::circle(Vec2::ZERO, 5.), holes: vec![] };
        let stock = crate::extrude(&Plane::XY, &[region], 0., 20.).unwrap().pop().unwrap();
        let cutter = crate::extrude(&Plane::XY, &[tool_region], 2., 22.).unwrap().pop().unwrap();
        let merged = stack_boolean(&stock, &cutter, BoolOp::Cut).unwrap().unwrap().unwrap();
        let protected = [
            stock.clone().with_split_lines(Some(std::sync::Arc::new(vec![vec![Vec3::new(10., 0., 0.), Vec3::new(10., 0., 20.)]]))),
            stock.with_paint(Some(crate::Paint { opacity: 1., faces: vec![crate::FacePaint { face: 0, color: [1., 0., 0.], opacity: 1. }] })),
        ];
        for input in protected {
            let output = stack_boolean(&input, &cutter, BoolOp::Cut).unwrap().unwrap().unwrap();
            assert_eq!(output.face_count(), merged.face_count() + 2);
            assert!((0..output.face_count()).all(|i| !output.face_was_coalesced(i)));
            // Independent circular chord-volume bound: 2*pi*(10*20+5*18)*1e-5 < .019.
            let expected = std::f64::consts::PI * (100. * 20. - 25. * 18.);
            for body in [&output, &merged] {
                let mesh = body.tessellate(1e-5).unwrap();
                assert!((mesh.measure().volume - expected).abs() < 0.03);
                assert!(mesh.contains(Vec3::new(0., 0., 1.)));
                assert!(!mesh.contains(Vec3::new(0., 0., 10.)));
                assert!(mesh.contains(Vec3::new(7., 0., 10.)));
            }
        }
    }

    #[test]
    fn annular_inward_tool_recovers_exact_stacks_before_cap_policy() {
        let plane = Plane::XY;
        let mut region = rectangle(Vec2::ZERO, Vec2::new(40.0, 30.0));
        region.holes.push(Loop2::circle(Vec2::new(20.0, 15.0), 5.0));
        let mut stock = crate::extrude(&plane, std::slice::from_ref(&region), 0.0, 20.0).unwrap().pop().unwrap();
        for (lo, hi) in [(20.0, 30.0), (30.0, 35.0)] {
            let positive = crate::extrude(&plane, std::slice::from_ref(&region), lo, hi).unwrap().pop().unwrap();
            stock = crate::boolean(&stock, &positive, BoolOp::Union).unwrap().unwrap();
        }
        let mut grown = rectangle(Vec2::new(-0.3, -0.3), Vec2::new(40.3, 30.3));
        grown.holes = region.holes;
        let tool = crate::extrude(&plane, &[grown], 30.0, 35.6).unwrap().pop().unwrap();
        let tol = stock.size().max(tool.size()) * 1e-6;
        let a = stack_of(&stock, Vec3::Z, &plane, tol).expect("annular stock qualifies");
        let b = stack_of(&tool, Vec3::Z, &plane, tol).expect("overrun tool qualifies");
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert!((a[0].lo - 0.0).abs() < tol && (a[0].hi - 35.0).abs() < tol);
        assert!((b[0].lo - 30.0).abs() < tol && (b[0].hi - 35.6).abs() < tol);
        assert_eq!(a[0].section.regions[0].holes.len(), 1);
        assert_eq!(b[0].section.regions[0].holes.len(), 1);
        assert!((a[0].section.regions[0].area() - (1200.0 - 25.0 * std::f64::consts::PI)).abs() < 1e-8);
        assert!(simple_prism_boolean(&stock, &tool, BoolOp::Cut).is_none());
        let result = stack_boolean(&stock, &tool, BoolOp::Cut).expect("qualified overlapping stacks").unwrap().unwrap();
        assert_eq!(result.shell_count(), 1);
        assert!(result.validity().is_empty());
        let mesh = result.tessellate(1e-5).unwrap();
        let expected = (1200.0 - 25.0 * std::f64::consts::PI) * 30.0;
        // An inscribed circular mesh at radial chord error 1e-5 has volume
        // deficit below 2*pi*R*height*error, conservatively 0.01 mm^3 here.
        assert!((mesh.measure().volume - expected).abs() < 0.02);
        for (point, present) in [
            (Vec3::new(5.0, 5.0, 29.0), true),
            (Vec3::new(5.0, 5.0, 31.0), false),
            (Vec3::new(20.0, 15.0, 29.0), false),
            (Vec3::new(24.8, 15.0, 29.0), false),
            (Vec3::new(25.2, 15.0, 29.0), true),
        ] {
            assert_eq!(mesh.contains(point), present, "{point:?}");
        }
    }
    #[test]
    fn annular_partial_cut_overruns_and_interior_lumps_preserve_material() {
        let area = 1200.0 - 25.0 * std::f64::consts::PI;
        for seam in [0.0, 0.37] {
            for plane in [Plane::XY, Plane::new(Vec3::new(30.0, 20.0, 10.0), Vec3::X, Vec3::Z).unwrap()] {
                let mut region = rectangle(Vec2::ZERO, Vec2::new(40.0, 30.0));
                region.holes.push(Loop2::circle_from(Vec2::new(20.0, 15.0), 5.0, seam));
                let stock = crate::extrude(&plane, std::slice::from_ref(&region), 0.0, 35.0).unwrap().pop().unwrap();
                let before = crate::step_export(&[&stock], "source").unwrap();
                for (lo, hi, retained, shells) in [(30.0, 35.6, 30.0, 1), (-0.6, 5.0, 30.0, 1), (10.0, 15.0, 30.0, 2), (30.0, 35.0, 30.0, 1)] {
                    let tool = crate::extrude(&plane, std::slice::from_ref(&region), lo, hi).unwrap().pop().unwrap();
                    let result = stack_boolean(&stock, &tool, BoolOp::Cut).expect("exact aligned cut path").unwrap().unwrap();
                    assert_eq!(result.shell_count(), shells);
                    assert!(result.validity().is_empty());
                    let mesh = result.tessellate(1e-5).unwrap();
                    assert!((mesh.measure().volume - retained * area).abs() < 0.02);
                    let at = |x: f64, y: f64, z: f64| plane.to_world(Vec2::new(x, y)) + plane.normal() * z;
                    for z in [2.5, 12.5, 32.5] {
                        let material = !(lo < z && z < hi);
                        assert_eq!(mesh.contains(at(5.0, 5.0, z)), material);
                        assert!(!mesh.contains(at(20.0, 15.0, z)));
                        assert_eq!(mesh.contains(at(25.2, 15.0, z)), material);
                    }
                    let public = crate::boolean(&stock, &tool, BoolOp::Cut).unwrap().unwrap();
                    assert!((public.tessellate(1e-5).unwrap().measure().volume - retained * area).abs() < 0.02);
                    if lo == 30.0 && hi > 35.0 {
                        let imported = crate::step_import(&crate::step_export(&[&result], "inward annular cut").unwrap()).unwrap();
                        assert!(imported.warnings.is_empty());
                        let saved = &imported.bodies.first().unwrap().body;
                        assert!((saved.tessellate(1e-5).unwrap().measure().volume - 30.0 * area).abs() < 0.02);
                        let next = crate::extrude(&plane, std::slice::from_ref(&region), 25.0, 31.0).unwrap().pop().unwrap();
                        let shortened = crate::boolean(saved, &next, BoolOp::Cut).unwrap().unwrap();
                        let next_volume = shortened.tessellate(1e-5).unwrap().measure().volume;
                        assert!(
                            (next_volume - 25.0 * area).abs() < 0.02,
                            "seam {seam}, plane {plane:?}, next cut volume {next_volume}, expected {}",
                            25.0 * area
                        );
                        assert!(shortened.contains(at(5.0, 5.0, 24.0)));
                        assert!(!shortened.contains(at(5.0, 5.0, 26.0)));
                        assert!(!shortened.contains(at(20.0, 15.0, 24.0)));
                    }
                }
                let clear = crate::extrude(&plane, std::slice::from_ref(&region), 36.0, 40.0).unwrap().pop().unwrap();
                assert!(stack_boolean(&stock, &clear, BoolOp::Cut).is_none());
                let unchanged = crate::boolean(&stock, &clear, BoolOp::Cut).unwrap().unwrap();
                assert!(std::sync::Arc::ptr_eq(&stock.solid, &unchanged.solid), "disjoint Cut must retain exact source topology");
                let unchanged_mesh = unchanged.tessellate(1e-5).unwrap();
                let source_mesh = stock.tessellate(1e-5).unwrap();
                assert_eq!(unchanged_mesh.measure().volume, source_mesh.measure().volume);
                // tessellate() floors tolerance at body.size()*1e-6. The arc
                // polygon's area error is at most perimeter*radial chord error.
                let bound = std::f64::consts::TAU * 5.0 * 35.0 * (stock.size() * 1e-6).max(1e-5) + 1e-5;
                assert!((unchanged_mesh.measure().volume - 35.0 * area).abs() < bound);
                assert!(!unchanged_mesh.contains(plane.to_world(Vec2::new(20.0, 15.0)) + plane.normal() * 12.5));
                let all = crate::extrude(&plane, std::slice::from_ref(&region), -1.0, 36.0).unwrap().pop().unwrap();
                assert!(stack_boolean(&stock, &all, BoolOp::Cut).unwrap().unwrap().is_none());
                assert_eq!(crate::step_export(&[&stock], "source").unwrap(), before);
            }
        }
    }
    #[test]
    fn annular_partial_cut_preserves_unequal_radii_and_conservative_refusals() {
        let mut region = rectangle(Vec2::ZERO, Vec2::new(40.0, 30.0));
        region.holes.push(Loop2::circle(Vec2::new(20.0, 15.0), 5.0));
        let stock = crate::extrude(&Plane::XY, std::slice::from_ref(&region), 0.0, 35.0).unwrap().pop().unwrap();
        let before = crate::step_export(&[&stock], "unchanged source").unwrap();
        let mut larger_hole = region.clone();
        larger_hole.holes = vec![Loop2::circle(Vec2::new(20.0, 15.0), 5.1)];
        let tool = crate::extrude(&Plane::XY, &[larger_hole], 30.0, 36.0).unwrap().pop().unwrap();
        let result = stack_boolean(&stock, &tool, BoolOp::Cut).unwrap().unwrap().unwrap();
        let expected = (1200.0 - 25.0 * std::f64::consts::PI) * 30.0 + std::f64::consts::PI * (5.1_f64.powi(2) - 25.0) * 5.0;
        let mesh = result.tessellate(1e-5).unwrap();
        assert!((mesh.measure().volume - expected).abs() < 0.02);
        assert!(mesh.contains(Vec3::new(25.05, 15.0, 32.0)));
        assert!(!mesh.contains(Vec3::new(25.2, 15.0, 32.0)));
        assert!(!mesh.contains(Vec3::new(20.0, 15.0, 32.0)));
        let interior = crate::extrude(&Plane::XY, std::slice::from_ref(&region), 10.0, 15.0).unwrap().pop().unwrap();
        for d in [Vec3::X, Vec3::Y] {
            let plane = Plane::from_normal(Vec3::ZERO, d).unwrap();
            let tol = stock.size() * 1e-6;
            assert!(stack_of(&stock, d, &plane, tol).is_none(), "transverse cylinder axis must not fill the hole");
            assert!(stack_of(&interior, d, &plane, tol).is_none());
        }
        assert!(stack_boolean(&stock, &interior, BoolOp::Union).is_none(), "Union retains its common-cap policy");
        let tapered = crate::extrude_tapered(&Plane::XY.offset(30.0), &rectangle(Vec2::ZERO, Vec2::new(40.0, 30.0)), 6.0, 1.0, 0.05).unwrap();
        assert!(stack_boolean(&stock, &tapered, BoolOp::Cut).is_none(), "slanted walls are not aligned prisms");
        let curved_cap = crate::sphere(Vec3::new(20.0, 15.0, 35.0), 5.0).unwrap();
        assert!(stack_boolean(&stock, &curved_cap, BoolOp::Cut).is_none());
        let pieces =
            (0..350).map(|i| crate::box_solid(Vec3::new(i as f64 * 3.0, 0.0, 0.0), Vec3::new(i as f64 * 3.0 + 1.0, 1.0, 1.0)).unwrap()).collect();
        let over_budget = crate::ops::join_pieces(pieces).unwrap().unwrap();
        assert!(over_budget.face_count() > 2048);
        assert!(stack_boolean(&over_budget, &interior, BoolOp::Cut).is_none());
        assert_eq!(crate::step_export(&[&stock], "unchanged source").unwrap(), before);
    }
    #[test]
    fn curved_prism_wall_requires_positive_separable_rational_weights() {
        let surface = |mixed: bool| {
            let weight = if mixed { 2.0 } else { 1.0 };
            let points = vec![
                vec![mt::Vector4::new(0.0, 0.0, 0.0, 1.0), mt::Vector4::new(0.0, 0.0, 10.0, 1.0)],
                vec![mt::Vector4::new(5.0, 0.0, 0.0, 1.0), mt::Vector4::new(5.0 * weight, 0.0, 10.0 * weight, weight)],
            ];
            mt::Surface::NurbsSurface(mt::NurbsSurface::new(mt::BSplineSurface::new(
                (mt::KnotVec::bezier_knot(1), mt::KnotVec::bezier_knot(1)),
                points,
            )))
        };
        assert!(wall_along(&surface(false), Vec3::Z, 1e-5, &mut 1000));
        assert!(!wall_along(&surface(true), Vec3::Z, 1e-5, &mut 1000), "parallel Euclidean controls alone do not certify a rational sweep");
        assert!(!wall_along(&surface(false), Vec3::Y, 1e-5, &mut 1000));
        assert!(!wall_along(&surface(false), Vec3::Z, 1e-5, &mut 0));
    }
    #[test]
    fn partial_depth_cuts_touching_a_through_hole_use_exact_stacked_sections() {
        let plane = Plane::named("XY").unwrap();
        let mut plate = rectangle(Vec2::ZERO, Vec2::new(100., 60.));
        plate.holes.push(rectangle(Vec2::new(25., 20.), Vec2::new(30., 30.)).outer);
        let stock = crate::extrude(&plane, &[plate], 0., 2.5).unwrap().pop().unwrap();
        let low = crate::extrude(&plane, &[rectangle(Vec2::new(20., 20.), Vec2::new(30., 30.))], 0., 1.5).unwrap().pop().unwrap();
        let high = crate::extrude(&plane, &[rectangle(Vec2::new(25., 20.), Vec2::new(35., 30.))], 1., 2.5).unwrap().pop().unwrap();
        for tools in [[&low, &high], [&high, &low]] {
            let mut result = stock.clone();
            for tool in tools {
                let target = result;
                result = stack_boolean(&target, tool, BoolOp::Cut).expect("exact stack path").unwrap().unwrap();
                // The second cut's target has walls in pieces across its slabs, followed level by
                // level: its history is exact.
                let exact = crate::boolean_record::tests::Reach { made: 0., moved: 0. };
                crate::boolean_record::tests::checked(&result, &[&target, tool], 1e-3, exact, &|_| false);
            }
            assert!(result.validity().is_empty());
            assert_eq!(result.shell_count(), 1);
            assert!((crate::measure(&result).unwrap().volume - 14_725.).abs() < 1e-6);
            let mesh = result.tessellate(1e-4).unwrap();
            for (point, material) in [
                (Vec3::new(22.5, 25., 0.5), false),
                (Vec3::new(22.5, 25., 2.), true),
                (Vec3::new(32.5, 25., 0.5), true),
                (Vec3::new(32.5, 25., 2.), false),
                (Vec3::new(27.5, 25., 1.25), false),
                (Vec3::new(10., 10., 1.25), true),
            ] {
                assert_eq!(mesh.contains(point), material, "{point:?}");
            }
            let imported = crate::step_import(&crate::step_export(&[&result], "layered cut").unwrap()).unwrap();
            assert!(imported.warnings.is_empty());
            assert!((crate::measure(&imported.bodies.first().unwrap().body).unwrap().volume - 14_725.).abs() < 1e-6);
            let next = crate::extrude(&plane, &[rectangle(Vec2::new(20., 20.), Vec2::new(25., 30.))], 1.5, 2.5).unwrap().pop().unwrap();
            let opened = crate::boolean(&imported.bodies.first().unwrap().body, &next, BoolOp::Cut).unwrap().unwrap();
            assert!((crate::measure(&opened).unwrap().volume - 14_675.).abs() < 1e-6);
            assert!(opened.validity().is_empty());
        }
    }
    #[test]
    fn far_edges_of_a_stepped_slab_come_from_the_operand_topology() {
        // A block with a notch in its back, read along -y: the slab from the notch floor to the
        // front has three segments beside the top face, read off the back and the notch floor.
        // The top face ends at the front in one edge: the far edge of all three, found with no
        // position compared (no pair budget).
        let notched = Loop2::polygon(&[
            Vec2::new(0., 0.),
            Vec2::new(40., 0.),
            Vec2::new(40., 30.),
            Vec2::new(25., 30.),
            Vec2::new(25., 25.),
            Vec2::new(15., 25.),
            Vec2::new(15., 30.),
            Vec2::new(0., 30.),
        ]);
        let block = crate::extrude(&Plane::named("XY").unwrap(), &[Region2 { outer: notched, holes: vec![] }], 0., 20.).unwrap().pop().unwrap();
        let (d, tol) = (-Vec3::Y, 1e-6);
        let plane = Plane::from_normal(Vec3::ZERO, d).unwrap();
        let normal = |f: &mt::Face| crate::blend::plane_normal(f).unwrap_or(Vec3::ZERO);
        let faces: Vec<(usize, &mt::Face)> = block.solid.face_iter().enumerate().collect();
        let find = |n: Vec3| faces.iter().find(|(_, f)| normal(f).dot(n) > 1. - 1e-9).map(|(i, _)| *i).unwrap();
        let (top, front) = (find(Vec3::Z), find(-Vec3::Y));
        let walls = faces.iter().filter(|(_, f)| normal(f).dot(d).abs() < 1e-9).map(|(i, f)| (*i, top_of(f, &plane))).collect();
        let index = Index::new(&block, 0);
        let stations = Stations::new(&index, walls, d, &plane, tol);
        let cap = Section::from_loops(index.loops(faces[front].1, front, &plane, tol, &mut 1000, &mut 1000).unwrap()).unwrap().oriented();
        let edge = match index.between.get(&(top.min(front), top.max(front))).map(Vec::as_slice) {
            Some([e]) => *e,
            other => panic!("one edge between the top and the front: {other:?}"),
        };
        let mut slab = stack_of(&block, d, &plane, tol).unwrap().pop().unwrap();
        assert!((slab.lo + 25.).abs() < tol && slab.hi.abs() < tol);
        let beside_top = |s: &Section| s.segments().1.into_iter().filter(|t| t.wall == Some(top)).map(|t| t.far).collect::<Vec<_>>();
        assert_eq!(beside_top(&slab.section), vec![Some(edge); 3], "read as a stack");
        slab.section.retag(|t| Tag { far: None, ..t });
        slab.section.mark_far(&[&cap], |w| stations.top_edge(&index, w, &[front], 0., tol), tol, &mut 0);
        assert_eq!(beside_top(&slab.section), vec![Some(edge); 3], "by topology alone");
    }
    #[test]
    fn compound_planar_extrusion_normalizes_overlap_and_retains_islands() {
        let plane = Plane::named("XY").unwrap();
        let a = rectangle(Vec2::ZERO, Vec2::new(10.0, 10.0));
        let b = rectangle(Vec2::new(5.0, 0.0), Vec2::new(15.0, 10.0));
        let body = extrude_planar_regions(&plane, &[a.clone(), b], 0.0, 2.0).unwrap();
        assert!((crate::measure(&body).unwrap().volume - 300.0).abs() < 1e-6);
        let inner = Region2 { outer: Loop2::circle(Vec2::new(5.0, 5.0), 2.0), holes: vec![] };
        let body = extrude_planar_regions(&plane, &[a.clone(), inner.clone()], 0.0, 2.0).unwrap();
        assert!((crate::measure(&body).unwrap().volume - 200.0).abs() < 1e-6);
        let annulus = Region2 { outer: a.outer, holes: vec![Loop2::circle(Vec2::new(5.0, 5.0), 3.0)] };
        let body = extrude_planar_regions(&plane, &[annulus, inner], 0.0, 2.0).unwrap();
        let expected = (100.0 - std::f64::consts::PI * 5.0) * 2.0;
        assert!((crate::measure(&body).unwrap().volume - expected).abs() < 0.1);
        assert_eq!(body.solid.boundaries().len(), 2);
        let a = rectangle(Vec2::ZERO, Vec2::new(10.0, 10.0));
        let b = rectangle(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));
        assert!(extrude_planar_regions(&plane, &[a, b], 0.0, 2.0).is_err());
        let circles = [Vec2::ZERO, Vec2::new(10.0, 0.0)].map(|center| Region2 { outer: Loop2::circle(center, 5.0), holes: vec![] });
        assert!(extrude_planar_regions(&plane, &circles, 0.0, 2.0).is_err());
    }

    #[test]
    fn analytic_circle_classification_respects_signed_sweeps_and_seams() {
        for seam in [0.0, 0.13, std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2] {
            for reversed in [false, true] {
                let lp = Loop2::circle_from(Vec2::ZERO, 10.0, seam);
                let lp = if reversed { lp.reversed() } else { lp };
                for p in [Vec2::ZERO, Vec2::new(9.9998, 0.0), Vec2::new(0.0, 9.9998), Vec2::new(0.0, -9.9998)] {
                    assert!(inside_loop(&lp, p), "{seam} {reversed} {p:?}");
                }
                for p in [Vec2::new(11.0, 0.0), Vec2::new(0.0, 10.0), Vec2::new(0.0, -10.0)] {
                    assert!(!inside_loop(&lp, p), "{seam} {reversed} {p:?}");
                }
            }
        }
        let half = Loop2 {
            segs: vec![
                Seg2::Arc { center: Vec2::ZERO, radius: 10.0, start: -std::f64::consts::FRAC_PI_2, sweep: std::f64::consts::PI },
                Seg2::Line { a: Vec2::new(0.0, 10.0), b: Vec2::new(0.0, -10.0) },
            ],
        };
        for lp in [half.clone(), half.reversed()] {
            assert!(inside_loop(&lp, Vec2::new(5.0, 0.0)));
            assert!(!inside_loop(&lp, Vec2::new(-5.0, 0.0)));
            assert!(!inside_loop(&lp, Vec2::new(-5.0, -10.0)));
        }
        // #307: a circle read back as two halves whose centres disagree by a few ulps, split
        // at its bottom. A ray grazing that bottom must not count one half's touch only.
        let halves = Loop2 {
            segs: vec![
                Seg2::Arc { center: Vec2::new(0.0, 25.0), radius: 5.0, start: -std::f64::consts::FRAC_PI_2, sweep: std::f64::consts::PI },
                Seg2::Arc {
                    center: Vec2::new(2.9e-15, 25.000000000000004),
                    radius: 4.99999999999999,
                    start: std::f64::consts::FRAC_PI_2,
                    sweep: std::f64::consts::PI,
                },
            ],
        };
        for lp in [halves.clone(), halves.reversed()] {
            for x in [-30.0, -5.0, -0.1] {
                assert!(!inside_loop(&lp, Vec2::new(x, 20.0)), "{x}");
                assert!(!inside_loop(&lp, Vec2::new(x, 30.0)), "{x}");
            }
            assert!(inside_loop(&lp, Vec2::new(0.0, 25.0)));
        }
    }

    /// Loops are closed only within the chaining tolerance. A ray level with one end of such a
    /// gap slipped through it without a crossing, so a point beside the loop counted as inside:
    /// a sheet shrunk by a parameter change (sketch solver residuals of 1e-13 mm) lost a bend
    /// zone that way and its unfold failed.
    #[test]
    fn ray_parity_closes_the_gaps_chaining_leaves() {
        let gap = 1e-13;
        let square = Loop2 {
            segs: vec![
                Seg2::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0) },
                Seg2::Line { a: Vec2::new(10.0, 0.0), b: Vec2::new(10.0, 10.0) },
                Seg2::Line { a: Vec2::new(10.0, 10.0), b: Vec2::new(0.0, 10.0) },
                Seg2::Line { a: Vec2::new(0.0, 10.0), b: Vec2::new(0.0, 5.0 + gap) },
                Seg2::Line { a: Vec2::new(0.0, 5.0), b: Vec2::new(0.0, 0.0) },
            ],
        };
        for lp in [square.clone(), square.reversed()] {
            assert!(!inside_loop(&lp, Vec2::new(-5.0, 5.0)));
            assert!(inside_loop(&lp, Vec2::new(5.0, 5.0)));
            assert!(!inside_loop(&lp, Vec2::new(15.0, 5.0)));
        }
        // A neighbour with an edge level with the gap joins the square instead of being
        // dropped as if it lay inside it (which left the outline open: no result at all).
        let a = Region2 { outer: square, holes: vec![] };
        let b = rectangle(Vec2::new(-4.0, 2.0), Vec2::new(0.0, 5.0));
        let union = planar_regions_boolean(&[a], &[b], BoolOp::Union, 1e-7).unwrap();
        assert_eq!(union.len(), 1);
        assert!((union[0].area().abs() - 112.0).abs() < 1e-9, "{}", union[0].area());
    }
    /// Two edges lying on one another within the tolerance, a hair from parallel, meet at a
    /// point somewhere along their overlap. Taken for a crossing, that point split the square's
    /// edge once more: 5e-8 mm before its end, which left an edge shorter than the tolerance in
    /// the result, or in the middle of a straight edge. The ends of an overlap are what split
    /// the edges.
    #[test]
    fn overlapping_lines_a_hair_from_parallel_do_not_cross() {
        let square = rectangle(Vec2::ZERO, Vec2::new(10.0, 10.0));
        // A quadrilateral from x = 4 to x = 14 with one edge along the square's bottom edge,
        // tilted by 1e-9 rad about (meet, 0), and the opposite edge at y = far.
        let quad = |meet: f64, far: f64| {
            let on = |x: f64| Vec2::new(x, (meet - x) * 1e-9);
            let mut corners = [on(4.0), on(14.0), Vec2::new(14.0, far), Vec2::new(4.0, far)];
            if far < 0.0 {
                corners.reverse();
            }
            Region2 { outer: Loop2::polygon(&corners), holes: vec![] }
        };
        let lengths = |r: &Region2| {
            let mut all: Vec<f64> = r.outer.segs.iter().map(Seg2::length).collect();
            all.sort_by(f64::total_cmp);
            all
        };
        let same = |got: &[f64], want: &[f64]| got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6);
        for meet in [10.0 - 5e-8, 7.0] {
            // Below the square, against its bottom edge: cut away, it leaves the square, the
            // bottom edge split only at x = 4, where the quadrilateral's corner lies on it.
            let below = quad(meet, -5.0);
            let cut = planar_regions_boolean(std::slice::from_ref(&square), std::slice::from_ref(&below), BoolOp::Cut, 1e-7).unwrap();
            assert_eq!(cut.len(), 1, "{meet}");
            assert!(same(&lengths(&cut[0]), &[4.0, 6.0, 10.0, 10.0, 10.0]), "{meet}: cut edges {:?}", lengths(&cut[0]));
            assert!((cut[0].area() - 100.0).abs() < 1e-6, "{meet}: {}", cut[0].area());
            // Over the square, flush with that edge and reaching past it to x = 14: the
            // union's outline runs along y = 0 once, in the pieces 0..4, 4..10 and 10..14.
            let flush = quad(meet, 5.0);
            let union = planar_regions_boolean(std::slice::from_ref(&square), std::slice::from_ref(&flush), BoolOp::Union, 1e-7)
                .unwrap_or_else(|e| panic!("{meet}: union {e}"));
            assert_eq!(union.len(), 1, "{meet}");
            assert!(same(&lengths(&union[0]), &[4.0, 4.0, 4.0, 5.0, 5.0, 6.0, 10.0, 10.0]), "{meet}: union edges {:?}", lengths(&union[0]));
            assert!((union[0].area() - 120.0).abs() < 1e-6, "{meet}: {}", union[0].area());
            // The two edges themselves, either way round.
            let (edge, tilted) = (square.outer.segs[0], flush.outer.segs[0]);
            assert!(crossings(&edge, &tilted, 1e-7).is_empty() && crossings(&tilted, &edge, 1e-7).is_empty(), "{meet}");
        }
        // Lines further apart than the tolerance at their ends still cross: tilted by 1e-3 rad
        // about (7, 0), the edge from (4, 3e-3) to (14, -7e-3) meets the bottom edge at x = 7.
        let (edge, steep) = (square.outer.segs[0], Seg2::Line { a: Vec2::new(4.0, 3e-3), b: Vec2::new(14.0, -7e-3) });
        let hits = crossings(&edge, &steep, 1e-7);
        assert!(hits.len() == 1 && (hits[0].0 - 0.7).abs() < 1e-12 && (hits[0].1 - 0.3).abs() < 1e-12, "{hits:?}");
    }

    #[test]
    fn tiny_tool_between_circle_and_display_chord_is_not_lost() {
        let circle = Region2 { outer: Loop2::circle(Vec2::ZERO, 10.0), holes: vec![] };
        let sampled = circle.outer.polyline(1e-3);
        let direction = (sampled[0] + sampled[1]).normalized().unwrap();
        let at = direction * 9.9998;
        let half = Vec2::new(0.000025, 0.000025);
        let square = rectangle(at - half, at + half);
        assert!(square.outer.segs.iter().all(|s| s.start().len() < 10.0));
        assert!(!circle.contains(at), "fixture must lie outside the display chord");
        let cut = planar_regions_boolean(std::slice::from_ref(&circle), std::slice::from_ref(&square), BoolOp::Cut, 1e-7).unwrap();
        assert_eq!(cut.len(), 1);
        assert_eq!(cut[0].holes.len(), 1);
        assert!((cut[0].holes[0].signed_area().abs() - 2.5e-9).abs() < 1e-13);
        let intersect = planar_regions_boolean(&[circle], &[square], BoolOp::Intersect, 1e-7).unwrap();
        assert_eq!(intersect.len(), 1);
        assert!((intersect[0].area() - 2.5e-9).abs() < 1e-13);
    }
    /// A 200 x 37 plate turned by `theta` about the origin, and the fillet region of one of its
    /// corners: an arc of radius `r` tangent to both edges there, closed by two lines that meet
    /// `reach` out past the corner.
    fn plate_and_corner_fillet(theta: f64, r: f64, reach: f64) -> (Region2, Region2) {
        let (sn, cs) = theta.sin_cos();
        let rot = |x: f64, y: f64| Vec2::new(x * cs - y * sn, x * sn + y * cs);
        let (w, lo, hi) = (200.0, -40.25, -3.25);
        let plate = Region2 { outer: Loop2::polygon(&[rot(0.0, hi), rot(0.0, lo), rot(w, lo), rot(w, hi)]), holes: vec![] };
        let arc = Seg2::Arc { center: rot(w - r, lo + r), radius: r, start: theta, sweep: -std::f64::consts::FRAC_PI_2 };
        let out = rot(w + reach, lo - reach);
        let cut =
            Region2 { outer: Loop2 { segs: vec![arc, Seg2::Line { a: arc.end(), b: out }, Seg2::Line { a: out, b: arc.start() }] }, holes: vec![] };
        (plate, cut)
    }

    /// A contact between a line and an arc that touch is one point. The quadratic's two roots
    /// there are rounding noise: a 1e-12 mm error in the centre's distance from the line moves
    /// them 2e-6 mm either way along it, and the cut then ran that far along the plate's edge
    /// past the arc's end before turning onto the arc: an edge a few millionths of a millimetre
    /// long in the result. Which turns of the plate do that is a matter of rounding, so every
    /// one is tried.
    #[test]
    fn an_arc_tangent_to_the_edges_it_ends_on_leaves_no_hairline_edges() {
        for k in 0..=360 {
            let theta = (k as f64 * 0.25).to_radians();
            for r in [2.0, 0.5, 25.0] {
                let (plate, cut) = plate_and_corner_fillet(theta, r, 0.4);
                let out = planar_regions_boolean(std::slice::from_ref(&plate), std::slice::from_ref(&cut), BoolOp::Cut, 1e-7)
                    .unwrap_or_else(|e| panic!("{k} r {r}: {e}"));
                assert_eq!(out.len(), 1, "{k} r {r}");
                let lp = &out[0].outer;
                let shortest = lp.segs.iter().map(Seg2::length).fold(f64::MAX, f64::min);
                let arcs = lp.segs.iter().filter(|s| matches!(s, Seg2::Arc { .. })).count();
                assert!(
                    out[0].holes.is_empty() && lp.segs.len() == 5 && arcs == 1 && shortest > 0.5,
                    "{k} r {r}: {} edges, shortest {shortest:e}",
                    lp.segs.len()
                );
                let want = 200.0 * 37.0 - r * r * (1.0 - std::f64::consts::FRAC_PI_4);
                assert!((out[0].area() - want).abs() < 1e-9, "{k} r {r}: {} vs {want}", out[0].area());
                // What the cut takes is the corner's fillet region, whole.
                let taken = planar_regions_boolean(std::slice::from_ref(&plate), std::slice::from_ref(&cut), BoolOp::Intersect, 1e-7).unwrap();
                assert!(taken.len() == 1 && taken[0].outer.segs.len() == 3, "{k} r {r}: {taken:?}");
                assert!((taken[0].area() - r * r * (1.0 - std::f64::consts::FRAC_PI_4)).abs() < 1e-9, "{k} r {r}");
            }
        }
    }

    /// Touching circles meet at one point on the line of their centres, inside and outside.
    #[test]
    fn touching_lines_and_arcs_meet_at_one_point() {
        let full = |c: Vec2, r: f64| Seg2::Arc { center: c, radius: r, start: 0.3, sweep: std::f64::consts::TAU - 1e-3 };
        for k in 0..=360 {
            let dir = Vec2::from_angle((k as f64 * 0.25).to_radians());
            let c = Vec2::new(151.3, -37.7);
            for (r0, r1, d) in [(2.0, 3.0, 5.0), (7.0, 3.0, 4.0), (3.0, 7.0, 4.0), (250.0, 2.0, 248.0)] {
                let hits = crossings(&full(c, r0), &full(c + dir * d, r1), 1e-7);
                let touch = c + dir * if r1 > r0 && d < r1 { -r0 } else { r0 };
                let pts: Vec<Vec2> = hits.iter().map(|(u, _)| full(c, r0).point_at(*u)).collect();
                assert!(!pts.is_empty() && pts.iter().all(|p| p.dist(touch) < 1e-9), "{k} {r0} {r1}: {pts:?} vs {touch:?}");
            }
            // A line touching a circle, the circle's side of it and the far side by rounding.
            for r in [2.0, 250.0] {
                let foot = c + dir * r;
                let line = Seg2::Line { a: foot - dir.perp() * 120.0, b: foot + dir.perp() * 78.0 };
                let hits = crossings(&line, &full(c, r), 1e-7);
                let pts: Vec<Vec2> = hits.iter().map(|(u, _)| line.point_at(*u)).collect();
                assert!(!pts.is_empty() && pts.iter().all(|p| p.dist(foot) < 1e-9), "{k} {r}: {pts:?} vs {foot:?}");
            }
        }
        // Real crossings stay two points: a chord 0.01 mm deep in a 2 mm circle.
        let line = Seg2::Line { a: Vec2::new(-5.0, 1.99), b: Vec2::new(5.0, 1.99) };
        let hits = crossings(&line, &full(Vec2::ZERO, 2.0), 1e-7);
        let half = (4.0f64 - 1.99 * 1.99).sqrt();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|(u, _)| (line.point_at(*u).x.abs() - half).abs() < 1e-12), "{hits:?}");
    }

    #[test]
    fn planar_region_api_rejects_hostile_values_and_budgets() {
        let plate = rectangle(Vec2::ZERO, Vec2::new(10.0, 10.0));
        for tolerance in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            assert!(planar_regions_boolean(std::slice::from_ref(&plate), &[], BoolOp::Union, tolerance).is_err());
        }
        let mut bad = plate.clone();
        bad.outer.segs[0] = Seg2::Line { a: Vec2::new(f64::NAN, 0.0), b: Vec2::X };
        assert!(planar_regions_boolean(&[bad], &[], BoolOp::Union, 1e-7).is_err());
        let many = vec![plate; MAX_PLANAR_SEGMENTS / 4 + 1];
        assert!(planar_regions_boolean(&many, &[], BoolOp::Union, 1e-7).is_err());
    }
}
