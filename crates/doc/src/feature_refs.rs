//! References from a feature to the rest of the design: the sketches it is built from (by id),
//! the features it names (patterned and mirrored features, the construction plane a plane is
//! built on) and the bodies it works on (by name). One walk, written once for `&` and `&mut`,
//! behind `input_sketches`, `remap_refs` and `remap_bodies`. Every variant is listed, so a new
//! one has to say what it refers to.

use std::collections::BTreeMap;

use crate::{FeatureKind, PatternKind, PlaneRef};

/// A reference a feature holds: `S` is `&u64` or `&mut u64` (a sketch id), `N` is `&String` or
/// `&mut String` (a feature or body name).
pub(crate) enum Ref<S, N> {
    Sketch(S),
    Feature(N),
    Body(N),
}

/// The construction plane a plane is built on (through offsets and angles), for `&` and `&mut`
/// (`$m` is `mut` or nothing). Nesting is capped like every walk over a plane.
macro_rules! plane_walk {
    ($name:ident $(, $m:tt)?) => {
        fn $name<'a>(mut p: &'a $($m)? PlaneRef, f: &mut impl FnMut(Ref<&'a $($m)? u64, &'a $($m)? String>)) {
            for _ in 0..64 {
                match p {
                    PlaneRef::Offset { base, .. } | PlaneRef::AtAngle { base, .. } => p = base,
                    PlaneRef::Construction { name } => return f(Ref::Feature(name)),
                    _ => return,
                }
            }
        }
    };
}

plane_walk!(plane_refs);
plane_walk!(plane_refs_mut, mut);

/// Every reference of a feature, written once for `&` and `&mut`: `$plane` is the matching plane
/// walk, `$m` is `mut` or nothing.
macro_rules! feature_walk {
    ($name:ident, $plane:ident $(, $m:tt)?) => {
        pub(crate) fn $name<'a>(&'a $($m)? self, f: &mut impl FnMut(Ref<&'a $($m)? u64, &'a $($m)? String>)) {
            use Ref::{Body as B, Feature as F, Sketch as S};
            match self {
                FeatureKind::Sketch { plane, .. } | FeatureKind::ConstructionPlane { plane } => $plane(plane, f),
                FeatureKind::Extrude { sketch, targets, .. } | FeatureKind::Revolve { sketch, targets, .. } => {
                    f(S(sketch));
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::Emboss { sketch, targets, face, .. } => {
                    f(S(sketch));
                    if let Some(face) = face {
                        f(B(& $($m)? face.body));
                    }
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::SurfaceExtrude { sketch, .. }
                | FeatureKind::SurfaceRevolve { sketch, .. }
                | FeatureKind::Rib { sketch, .. }
                | FeatureKind::SheetBase { sketch, .. }
                | FeatureKind::SheetContour { sketch, .. } => f(S(sketch)),
                FeatureKind::SurfaceSweep { sketch, path_sketch, .. } => {
                    f(S(sketch));
                    f(S(path_sketch));
                }
                FeatureKind::SurfaceLoft { sections } => {
                    for s in sections {
                        f(S(& $($m)? s.sketch));
                    }
                }
                FeatureKind::Sweep { sketch, path_sketch, targets, .. } => {
                    f(S(sketch));
                    f(S(path_sketch));
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::Pipe { path_sketch, targets, .. } => {
                    f(S(path_sketch));
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::Loft { sections, targets, .. } => {
                    for s in sections {
                        f(S(& $($m)? s.sketch));
                    }
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::Coil { targets, .. } => {
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::Hole { points, .. } => {
                    if let Some(p) = points {
                        f(S(& $($m)? p.sketch));
                    }
                }
                FeatureKind::Pattern { features, pattern, bodies } => {
                    for n in features {
                        f(F(n));
                    }
                    if let PatternKind::Path { path_sketch, .. } = pattern {
                        f(S(path_sketch));
                    }
                    for b in bodies {
                        f(B(b));
                    }
                }
                FeatureKind::Mirror { features, plane, bodies, .. } => {
                    for n in features {
                        f(F(n));
                    }
                    $plane(plane, f);
                    for b in bodies {
                        f(B(b));
                    }
                }
                FeatureKind::Split { body, plane, tool } | FeatureKind::SurfaceTrim { body, plane, tool, .. } => {
                    f(B(body));
                    $plane(plane, f);
                    if let Some(t) = tool {
                        f(B(& $($m)? t.body));
                    }
                }
                FeatureKind::SplitFace { body, plane, tool, sketch, .. } => {
                    $plane(plane, f);
                    if let Some(s) = sketch {
                        f(S(s));
                    }
                    if let Some(b) = body {
                        f(B(b));
                    }
                    if let Some(t) = tool {
                        f(B(& $($m)? t.body));
                    }
                }
                FeatureKind::Draft { neutral: plane, body, .. } | FeatureKind::ReplaceFace { target: plane, body, .. } => {
                    $plane(plane, f);
                    if let Some(b) = body {
                        f(B(b));
                    }
                }
                FeatureKind::SheetFold { sketch, body, .. } | FeatureKind::Patch { sketch, body, .. } => {
                    if let Some(s) = sketch {
                        f(S(s));
                    }
                    if let Some(b) = body {
                        f(B(b));
                    }
                }
                FeatureKind::Fillet { body, .. }
                | FeatureKind::Chamfer { body, .. }
                | FeatureKind::Shell { body, .. }
                | FeatureKind::OffsetFace { body, .. }
                | FeatureKind::DeleteFace { body, .. }
                | FeatureKind::SheetFlange { body, .. }
                | FeatureKind::SheetHem { body, .. }
                | FeatureKind::SheetUnfold { body, .. }
                | FeatureKind::Boss { body, .. }
                | FeatureKind::Lip { body, .. }
                | FeatureKind::SnapFit { body, .. }
                | FeatureKind::Rest { body, .. }
                | FeatureKind::SurfaceOffset { body, .. } => {
                    if let Some(b) = body {
                        f(B(b));
                    }
                }
                FeatureKind::SheetConvert { body, .. }
                | FeatureKind::SurfaceExtend { body, .. }
                | FeatureKind::Unstitch { body, .. }
                | FeatureKind::ReverseNormal { body, .. }
                | FeatureKind::MeshConvert { body, .. } => f(B(body)),
                FeatureKind::Combine { target, tools, .. } => {
                    f(B(target));
                    for b in tools {
                        f(B(b));
                    }
                }
                FeatureKind::Thicken { bodies, targets, .. } => {
                    for b in bodies {
                        f(B(b));
                    }
                    for b in targets {
                        f(B(b));
                    }
                }
                FeatureKind::BodyTransfer { bodies, .. }
                | FeatureKind::Scale { bodies, .. }
                | FeatureKind::BoundingSolid { bodies, .. }
                | FeatureKind::Align { bodies, .. }
                | FeatureKind::Remove { bodies }
                | FeatureKind::Stitch { bodies, .. }
                | FeatureKind::Move { bodies, .. } => {
                    for b in bodies {
                        f(B(b));
                    }
                }
                FeatureKind::BoundaryFill { tools, .. } => {
                    for b in tools {
                        f(B(b));
                    }
                }
                // Construction axes and points refer to geometry, which copies remap on their own
                // (`Feature::remap_clone`).
                FeatureKind::ConstructionAxis { .. }
                | FeatureKind::ConstructionPoint { .. }
                | FeatureKind::Box { .. }
                | FeatureKind::Cylinder { .. }
                | FeatureKind::Sphere { .. }
                | FeatureKind::Torus { .. }
                | FeatureKind::Thread { .. }
                | FeatureKind::Import { .. }
                | FeatureKind::MeshImport { .. } => {}
            }
        }
    };
}

impl FeatureKind {
    feature_walk!(walk_refs, plane_refs);
    feature_walk!(walk_refs_mut, plane_refs_mut, mut);

    /// Sketches the feature is built from (profiles, paths, points): deleting one of them
    /// deletes the feature.
    pub fn input_sketches(&self) -> Vec<u64> {
        let mut v = Vec::new();
        self.walk_refs(&mut |r| {
            if let Ref::Sketch(id) = r {
                v.push(*id);
            }
        });
        v
    }

    /// Point references to sketches (by id) and features (by name) at their copies, or at a
    /// feature's new name (`timeline.rename`).
    pub fn remap_refs(&mut self, sketches: &BTreeMap<u64, u64>, names: &BTreeMap<String, String>) {
        self.walk_refs_mut(&mut |r| match r {
            Ref::Sketch(id) => {
                if let Some(n) = sketches.get(id) {
                    *id = *n;
                }
            }
            Ref::Feature(name) => {
                if let Some(n) = names.get(name) {
                    *name = n.clone();
                }
            }
            Ref::Body(_) => {}
        });
    }

    /// Point references to bodies (by name) at their copies.
    pub fn remap_bodies(&mut self, bodies: &BTreeMap<String, String>) {
        self.walk_refs_mut(&mut |r| {
            if let Ref::Body(name) = r
                && let Some(n) = bodies.get(name)
            {
                *name = n.clone();
            }
        });
    }
}
