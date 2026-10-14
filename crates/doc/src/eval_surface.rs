//! Evaluation of the surface features: Patch, Stitch, Thicken, Trim, Extend, Surface Extrude,
//! Revolve, Sweep and Loft, Offset, Unstitch and Reverse Normal (kernel `surfaces`,
//! `surface_offset`).

use super::*;
use solidcraft_geom::Seg2;

pub(super) fn eval(doc: &Document, vals: &BTreeMap<String, Value>, f: &Feature, st: &mut ModelState) -> Result<()> {
    match &f.kind {
        FeatureKind::Patch { sketch, profiles, body, edges } => {
            let mut made = Vec::new();
            if let Some(sk) = sketch {
                let ss = st.sketch(*sk).ok_or_else(|| DocError::Unknown(format!("sketch {sk} (it must come earlier in the timeline)")))?;
                let regions = solidcraft_sketch::merge_regions(&select_profiles(ss, profiles)?);
                for r in &regions {
                    made.push(kernel::patch_region(&ss.plane, r)?);
                }
            }
            if !edges.is_empty() {
                let i = body_at(st, body, edges)?;
                let b = st.bodies.get(i).ok_or_else(|| DocError::Invalid("body".into()))?;
                made.push(kernel::patch_edges(&b.body, edges)?);
            }
            if made.is_empty() {
                return Err(DocError::Invalid("a patch needs a sketch region or a loop of edges".into()));
            }
            for (k, b) in made.into_iter().enumerate() {
                let name = new_name(st, f, k);
                st.bodies.push(ModelBody::new(name, b, f.id));
            }
            Ok(())
        }
        FeatureKind::Stitch { bodies, tolerance } => {
            let tol = val(vals, tolerance, Kind::Length)?;
            let idx: Vec<usize> = bodies
                .iter()
                .map(|n| st.bodies.iter().position(|b| &b.name == n).ok_or_else(|| DocError::Unknown(format!("body `{n}`"))))
                .collect::<Result<_>>()?;
            let Some(&first) = idx.first() else { return Err(DocError::Invalid("pick the surfaces to stitch".into())) };
            let refs: Vec<&kernel::Body> = idx.iter().filter_map(|i| st.bodies.get(*i).map(|b| &b.body)).collect();
            let sewn = kernel::stitch(&refs, tol)?;
            let Some(mb) = st.bodies.get(first).cloned() else { return Err(DocError::Invalid("body".into())) };
            if let Some(slot) = st.bodies.get_mut(first) {
                *slot = ModelBody::new(mb.name, sewn, mb.feature);
            }
            // The others are now part of the first.
            let gone: Vec<String> = idx.iter().skip(1).filter_map(|i| st.bodies.get(*i).map(|b| b.name.clone())).collect();
            st.bodies.retain(|b| !gone.contains(&b.name));
            Ok(())
        }
        FeatureKind::Thicken { bodies, thickness, symmetric, operation, targets } => {
            let t = val(vals, thickness, Kind::Length)?;
            let mut tools = Vec::new();
            for n in bodies {
                let b = st.body(n).ok_or_else(|| DocError::Unknown(format!("body `{n}`")))?;
                tools.push(kernel::thicken(&b.body, t, *symmetric)?);
            }
            if tools.is_empty() {
                return Err(DocError::Invalid("pick the surfaces to thicken".into()));
            }
            apply_op(st, f, tools, *operation, targets)
        }
        FeatureKind::SurfaceTrim { body, plane, tool, keep } => {
            let i = st.bodies.iter().position(|b| &b.name == body).ok_or_else(|| DocError::Unknown(format!("body `{body}`")))?;
            let Some(mb) = st.bodies.get(i).cloned() else { return Err(DocError::Invalid("body".into())) };
            let trimmed = match tool {
                Some(t) => {
                    let tb = st.body(&t.body).ok_or_else(|| DocError::Unknown(format!("body `{}`", t.body)))?;
                    let face = crate::appearance::face_index_at(tb, t.point)
                        .ok_or_else(|| DocError::Invalid(format!("no face of `{}` at the tool point", t.body)))?;
                    kernel::trim(&mb.body, &kernel::SplitTool::Face { body: &tb.body, face }, *keep)?
                }
                None => kernel::trim(&mb.body, &kernel::SplitTool::Plane(doc.resolve_plane_in(vals, plane, 0, Some(st))?), *keep)?,
            };
            if let Some(slot) = st.bodies.get_mut(i) {
                *slot = ModelBody::new(mb.name, trimmed, mb.feature);
            }
            Ok(())
        }
        FeatureKind::SurfaceExtend { body, edges, distance } => {
            let d = val(vals, distance, Kind::Length)?;
            let i = st.bodies.iter().position(|b| &b.name == body).ok_or_else(|| DocError::Unknown(format!("body `{body}`")))?;
            let Some(mb) = st.bodies.get(i).cloned() else { return Err(DocError::Invalid("body".into())) };
            let out = kernel::extend(&mb.body, edges, d)?;
            if let Some(slot) = st.bodies.get_mut(i) {
                *slot = ModelBody::new(mb.name, out, mb.feature);
            }
            Ok(())
        }
        FeatureKind::SurfaceExtrude { sketch, profiles, curves, extent } => {
            let ss = sketch_in_feature_frame(
                doc,
                st.sketch(*sketch).ok_or_else(|| DocError::Unknown(format!("sketch {sketch} (it must come earlier in the timeline)")))?,
                f.component,
            )?;
            if extent.through_all || extent.taper.is_some() || extent.to.is_some() || extent.from.is_some() {
                return Err(DocError::Invalid("not supported yet: a surface extrude through all, to or from an object, or with a taper".into()));
            }
            let d = val(vals, &extent.distance, Kind::Length)?;
            let off = extent.start_offset.as_deref().map(|e| val(vals, e, Kind::Length)).transpose()?.unwrap_or(0.0);
            let (a, b) = extent_sides(vals, extent, d, extent.direction)?;
            let chains = sheet_chains(&ss, profiles.as_ref(), curves)?;
            let body = kernel::extrude_sheet(&ss.plane, &chains, a + off, b + off)?;
            push_sheet(st, f, crate::lineage::ChainRoles { sketch: &ss, chains: &chains }.tool(body));
            Ok(())
        }
        FeatureKind::SurfaceRevolve { sketch, profiles, curves, axis, angle, angle2, symmetric } => {
            let ss = sketch_in_feature_frame(
                doc,
                st.sketch(*sketch).ok_or_else(|| DocError::Unknown(format!("sketch {sketch} (it must come earlier in the timeline)")))?,
                f.component,
            )?;
            let (o, d) = revolve_axis(&ss, axis)?;
            let a = val(vals, angle, Kind::Angle)?;
            let (start, sweep) = match (angle2, symmetric) {
                (Some(a2), _) => {
                    let a2 = val(vals, a2, Kind::Angle)?;
                    (-a2, a + a2)
                }
                (None, true) => (-a / 2.0, a),
                (None, false) => (0.0, a),
            };
            let chains = sheet_chains(&ss, profiles.as_ref(), curves)?;
            let body = kernel::revolve_sheet(&ss.plane, &chains, o, d, start, sweep)?;
            push_sheet(st, f, crate::lineage::ChainRoles { sketch: &ss, chains: &chains }.tool(body));
            Ok(())
        }
        FeatureKind::Unstitch { body, faces } => {
            let i = st.bodies.iter().position(|b| &b.name == body).ok_or_else(|| DocError::Unknown(format!("body `{body}`")))?;
            let Some(mb) = st.bodies.get(i).cloned() else { return Err(DocError::Invalid("body".into())) };
            let (pieces, rest) = kernel::unstitch(&mb.body, faces)?;
            let source = mb.traced();
            // The faces left keep the body's name; each face taken out is a new surface body.
            match rest {
                Some(rest) => {
                    if let Some(slot) = st.bodies.get_mut(i) {
                        *slot = ModelBody::traced_new(mb.name.clone(), source.then(rest), mb.feature);
                    }
                }
                None => {
                    st.bodies.remove(i);
                }
            }
            for (k, piece) in pieces.into_iter().enumerate() {
                let name = new_name(st, f, k);
                st.bodies.push(ModelBody::traced_new(name, source.then(piece), f.id));
            }
            Ok(())
        }
        FeatureKind::ReverseNormal { body, faces } => {
            let i = st.bodies.iter().position(|b| &b.name == body).ok_or_else(|| DocError::Unknown(format!("body `{body}`")))?;
            let Some(mb) = st.bodies.get(i).cloned() else { return Err(DocError::Invalid("body".into())) };
            let turned = mb.traced().then(kernel::reverse_faces(&mb.body, faces)?);
            if let Some(slot) = st.bodies.get_mut(i) {
                *slot = ModelBody::traced_new(mb.name, turned, mb.feature);
            }
            Ok(())
        }
        FeatureKind::SurfaceSweep { sketch, profiles, curves, path_sketch, path } => {
            let ss = st.sketch(*sketch).ok_or_else(|| DocError::Unknown(format!("sketch {sketch} (it must come earlier in the timeline)")))?;
            let ps =
                st.sketch(*path_sketch).ok_or_else(|| DocError::Unknown(format!("sketch {path_sketch} (it must come earlier in the timeline)")))?;
            // As Sweep: a 3D path sketch is followed in the feature's frame, the curves with it.
            let profile;
            let ss = if !ps.sketch.wires.is_empty() {
                profile = profile_in_frame(doc, ss, f.component)?;
                &profile
            } else {
                ss
            };
            let mapped = path_in_frame(doc, ps, f.component)?;
            let ps = mapped.as_ref();
            let chains = sheet_chains(ss, profiles.as_ref(), curves)?;
            // The path runs on from the end nearest the middle of the curves.
            let pts: Vec<Vec2> = chains.iter().flatten().flat_map(|s| s.polyline(1e-2)).collect();
            let start = ss.plane.to_world(pts.iter().fold(Vec2::default(), |a, p| a + *p) * (1.0 / pts.len().max(1) as f64));
            let body = match path_points(ps, path, start)? {
                Some(pts) => kernel::sweep_sheet_points(&ss.plane, &chains, &pts)?,
                None => kernel::sweep_sheet(&ss.plane, &chains, &path_segments(ps, path, start)?)?,
            };
            let traced = crate::lineage::ChainRoles { sketch: ss, chains: &chains }.tool(body);
            push_sheet(st, f, traced);
            Ok(())
        }
        FeatureKind::SurfaceLoft { sections } => {
            let mut secs = Vec::with_capacity(sections.len());
            let mut sketches = Vec::with_capacity(sections.len());
            for sec in sections {
                let ss = sketch_in_feature_frame(
                    doc,
                    st.sketch(sec.sketch)
                        .ok_or_else(|| DocError::Unknown(format!("sketch {} (it must come earlier in the timeline)", sec.sketch)))?,
                    f.component,
                )?;
                let profiles = match (&sec.profiles, sec.curves.is_empty()) {
                    (None, true) => Some(&ProfileSel::All),
                    (p, _) => p.as_ref(),
                };
                let mut chains = sheet_chains(&ss, profiles, &sec.curves)?.into_iter();
                let (Some(chain), None) = (chains.next(), chains.next()) else {
                    return Err(DocError::Invalid(
                        "each loft section must be one chain: one profile without holes, or curves meeting end to end".into(),
                    ));
                };
                secs.push((ss.plane, chain));
                sketches.push(ss);
            }
            let body = kernel::loft_sheet(&secs)?;
            // Each wall named by the section curves it runs between, as a loft's sides are.
            let loops: Vec<solidcraft_geom::Loop2> = secs.iter().map(|(_, c)| solidcraft_geom::Loop2 { segs: c.clone() }).collect();
            let traced = crate::lineage::SectionRoles { sections: sketches.iter().zip(&loops).collect() }.tool(body);
            push_sheet(st, f, traced);
            Ok(())
        }
        FeatureKind::SurfaceOffset { faces, body, distance, chain, tolerance } => {
            let d = val(vals, distance, Kind::Length)?;
            let tol = match tolerance {
                Some(t) => val(vals, t, Kind::Length)?,
                None => 0.01,
            };
            let i = body_at(st, body, faces)?;
            let Some(mb) = st.bodies.get(i).cloned() else { return Err(DocError::Invalid("body".into())) };
            let picks = if *chain { smooth_chain(&mb, faces) } else { faces.clone() };
            let mut picked: Vec<usize> = Vec::new();
            for p in &picks {
                let fi =
                    crate::appearance::face_index_at(&mb, *p).ok_or_else(|| DocError::Invalid(format!("Offset: no face of {} at {p:?}", mb.name)))?;
                if !picked.contains(&fi) {
                    picked.push(fi);
                }
            }
            let made = Traced::step(kernel::offset_surface(&mb.body, &picked, d, tol)?, &[&mb.traced()]);
            let name = new_name(st, f, 0);
            st.bodies.push(ModelBody::traced_new(name, made, f.id));
            Ok(())
        }
        _ => Err(DocError::Invalid("not a surface feature".into())),
    }
}

/// A surface builder's body, added to the model with what its faces came from.
fn push_sheet(st: &mut ModelState, f: &Feature, traced: crate::lineage::Traced) {
    let name = new_name(st, f, 0);
    st.bodies.push(ModelBody::traced_new(name, traced, f.id));
}

/// What a surface builder sweeps: the boundary loops of the picked profiles, then the picked
/// curves chained end to end.
fn sheet_chains(ss: &SolvedSketch, profiles: Option<&ProfileSel>, curves: &[String]) -> Result<Vec<Vec<Seg2>>> {
    let mut chains: Vec<Vec<Seg2>> = Vec::new();
    if let Some(sel) = profiles {
        for r in solidcraft_sketch::merge_regions(&select_profiles(ss, sel)?) {
            chains.extend(std::iter::once(r.outer).chain(r.holes).map(|lp| lp.segs));
        }
    }
    chains.extend(curve_chains(ss, curves)?);
    if chains.is_empty() {
        return Err(DocError::Invalid("pick the profiles or curves".into()));
    }
    Ok(chains)
}

/// Most sketch curves one feature chains.
const MAX_CHAIN_CURVES: usize = 1000;

/// The sketch curves `ids` (free-form ones exactly where they can be) grouped into chains of segments that meet end to end, each oriented
/// to run one way. Curves are joined greedily at their free ends: where three meet, one of them
/// starts a chain of its own.
fn curve_chains(ss: &SolvedSketch, ids: &[String]) -> Result<Vec<Vec<Seg2>>> {
    if ids.len() > MAX_CHAIN_CURVES {
        return Err(DocError::Invalid(format!("at most {MAX_CHAIN_CURVES} curves")));
    }
    let mut pieces: std::collections::VecDeque<Vec<Seg2>> = ids
        .iter()
        .map(|id| {
            let ci = ss.sketch.curve_index(id).ok_or_else(|| DocError::Unknown(format!("curve `{id}`")))?;
            Ok(ss.sketch.exact_segs(ci).unwrap_or_else(|| ss.sketch.segs(ci)))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    let reversed = |p: Vec<Seg2>| -> Vec<Seg2> { p.iter().rev().map(Seg2::reversed).collect() };
    let near = |p: Vec2, q: Vec2| p.dist(q) <= 1e-6;
    let mut chains = Vec::new();
    while let Some(mut chain) = pieces.pop_front() {
        while let (Some(head), Some(tail)) = (chain.first().map(Seg2::start), chain.last().map(Seg2::end)) {
            if near(head, tail) {
                break;
            }
            let ends = |p: &Vec<Seg2>| p.first().map(Seg2::start).zip(p.last().map(Seg2::end));
            let Some(i) = pieces.iter().position(|p| ends(p).is_some_and(|(s, e)| near(s, tail) || near(e, tail) || near(s, head) || near(e, head)))
            else {
                break;
            };
            let Some(p) = pieces.remove(i) else { break };
            let Some((s, e)) = ends(&p) else { continue };
            if near(s, tail) {
                chain.extend(p);
            } else if near(e, tail) {
                chain.extend(reversed(p));
            } else if near(e, head) {
                chain = p.into_iter().chain(chain).collect();
            } else {
                chain = reversed(p).into_iter().chain(chain).collect();
            }
        }
        chains.push(chain);
    }
    Ok(chains)
}
