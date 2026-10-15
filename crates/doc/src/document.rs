use serde::{Deserialize, Serialize};
use solidcraft_geom::{Plane, Vec2, Vec3};
use solidcraft_sketch::Sketch;

use crate::construct::{AxisDef, GeoRef, PointDef};
use crate::expr::{self, Kind};
use crate::{DocError, Result};

pub const MAX_FEATURES: usize = 10_000;
pub const MAX_PARAMS: usize = 10_000;
/// Largest feature id a replayed feature may keep (`Document::insert_feature`): exact in JSON
/// numbers, and far from overflowing the next id.
pub const MAX_FEATURE_ID: u64 = 1 << 52;

/// A named parameter. User parameters are created by the user; model parameters (`d1`, `d2`…)
/// are created by dimensions and features.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub expr: String,
    /// `mm`, `deg` or empty (unit-less).
    pub unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// Created by a dimension or feature (not by the user).
    #[serde(default)]
    pub model: bool,
}

impl Param {
    pub fn kind(&self) -> Kind {
        Kind::from_unit(&self.unit)
    }
}

/// Where a sketch lies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaneRef {
    /// `XY`, `XZ` or `YZ`.
    Origin { name: String },
    /// An explicit plane (e.g. picked from a planar face).
    Custom { plane: Plane },
    /// Another plane offset along its normal by an expression.
    Offset { base: Box<PlaneRef>, distance: String },
    /// Another plane rotated about a line (in world coordinates) by an angle expression.
    AtAngle { base: Box<PlaneRef>, axis_origin: Vec3, axis_dir: Vec3, angle: String },
    /// A construction plane feature (by name).
    Construction { name: String },
    /// Halfway between two planes (or planar faces); bisecting them when they meet at an angle.
    Midplane { a: Box<GeoRef>, b: Box<GeoRef> },
    /// Through two straight edges (lines) that lie in one plane.
    TwoEdges { a: Box<GeoRef>, b: Box<GeoRef> },
    /// Through three points.
    ThreePoints { a: Box<GeoRef>, b: Box<GeoRef>, c: Box<GeoRef> },
    /// Tangent to a face (cylinder, cone, sphere, torus) where the point `at` lies over it.
    Tangent { face: Box<GeoRef>, at: Vec3 },
    /// Square to an edge or sketch curve, a fraction `t` (expression, 0…1) of the way along it.
    AlongPath { path: Box<GeoRef>, t: String },
    /// Through a straight edge (or line, axis) and square to a plane (or planar face).
    Perpendicular { plane: Box<GeoRef>, line: Box<GeoRef> },
    /// A planar body face. Certified attachments resolve only their exact source and report
    /// an error when it is lost; `plane` then supplies a repair guide. Legacy references without
    /// an anchor try the persistent `name`, then the nearest parallel face at `at`, and retain
    /// the picked `plane` as their historical fallback.
    Face {
        plane: Plane,
        at: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Exact attachment identity for newly certified faces; absent on legacy designs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Box<crate::FaceAnchor>>,
    },
}

/// Complete source curves of an outer boundary and each of its holes. Unlike a
/// span multiset, this identity is independent of adaptive spline subdivision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WholeProfileBoundary {
    pub outer: Vec<String>,
    pub holes: Vec<Vec<String>>,
}

/// Which closed profiles of a sketch a feature uses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProfileSel {
    /// Every profile.
    All,
    /// Profiles by index in the solved sketch's profile list.
    Indices { indices: Vec<usize> },
    /// Profiles whose outer loop is made of exactly these curves, or pieces of them where other
    /// curves cut them (order and repeats don't matter; see `sketch::profile_by_curves`). Found
    /// again on every evaluation, so stable across parameter changes; no match or several is
    /// an error, never another profile.
    Curves { loops: Vec<Vec<String>> },
    /// Complete current boundaries of topology-aware analytic offset associations.
    /// The local association identity survives retirement and resurrection of arc roles.
    OffsetBoundaries { associations: Vec<String> },
    /// Exact whole source coverage, including holes; partial source curves do not match.
    WholeBoundaries { profiles: Vec<WholeProfileBoundary> },
    /// The profile containing each sketch point.
    Points { points: Vec<Vec2> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    #[default]
    NewBody,
    Join,
    Cut,
    Intersect,
}

impl Operation {
    pub fn parse(s: &str) -> Option<Operation> {
        Some(match s.to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
            "new" | "newbody" | "newcomponent" => Operation::NewBody,
            "join" | "union" | "add" => Operation::Join,
            "cut" | "subtract" | "difference" => Operation::Cut,
            "intersect" | "intersection" => Operation::Intersect,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Along the sketch normal.
    #[default]
    Positive,
    Negative,
    /// Both sides; the extent convention determines whether `distance` is total or per side.
    Symmetric,
}

/// The direction a ruled surface leaves its edges (`surface.ruled`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuledMode {
    /// Along the normal of the face beside each edge.
    #[default]
    Normal,
    /// Continuing the face beside each edge.
    Tangent,
    /// At an angle from a reference direction (the face's normal unless one is given).
    Angle,
    /// Along a fixed direction.
    Direction,
}

impl RuledMode {
    fn is_normal(&self) -> bool {
        *self == RuledMode::Normal
    }
}

/// How a ruled surface's strips meet at sharp corners of its edge chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuledCorners {
    /// Extended or cut back until they meet.
    #[default]
    Mitre,
    /// Joined by a sector (inside corners are cut back).
    Round,
}

impl RuledCorners {
    fn is_mitre(&self) -> bool {
        *self == RuledCorners::Mitre
    }
}

/// Angular starts in older designs used the raw axis sign. New revolves measure starts
/// along the same sketch-normal sweep direction as the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevolveConvention {
    #[default]
    LegacyAxisStart,
    SketchNormal,
}

/// Symmetric distance semantics. Missing markers preserve existing designs' thickness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeConvention {
    #[default]
    LegacyPerSide,
    TotalDistance,
}

impl ExtrudeConvention {
    /// Distance from the mid-plane to either cap for a distance-limited extent.
    pub fn symmetric_half_distance(self, distance: f64) -> f64 {
        match self {
            Self::LegacyPerSide => distance,
            Self::TotalDistance => distance * 0.5,
        }
    }
}

/// Extent of an extrude.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct Extent {
    #[walk(length = "Distance")]
    pub distance: String,
    #[serde(default)]
    pub convention: ExtrudeConvention,
    #[serde(default)]
    pub direction: Direction,
    /// Optional second side distance (two-sided extrude), along the negative normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(length = "Distance 2")]
    pub distance2: Option<String>,
    /// Start offset from the sketch plane (expression), along the sketch normal whatever the
    /// direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(length = "Start offset")]
    pub start_offset: Option<String>,
    /// Through all bodies in the direction(s) instead of `distance`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub through_all: bool,
    /// Taper angle (expression; positive grows the profile).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(angle = "Taper angle")]
    pub taper: Option<String>,
    /// To Object: end at the face (or vertex/point) at this point, instead of `distance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(pick)]
    pub to: Option<Vec3>,
    /// Offset past the To Object face (expression; negative stops short).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(length = "To offset")]
    pub to_offset: Option<String>,
    /// From Object: start at the face (or point) here instead of the sketch plane (plus
    /// `start_offset`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(pick)]
    pub from: Option<Vec3>,
}

/// Thin Extrude: a wall of constant thickness along sketch curves instead of the profiles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct ThinWall {
    /// The sketch curves the wall follows: open chains and closed loops, joined where they
    /// meet end to end.
    pub curves: Vec<String>,
    /// The wall thickness (expression); with `side: two_sides`, the thickness inside.
    #[walk(length = "Thickness")]
    pub thickness: String,
    #[serde(default, skip_serializing_if = "ThinSide::is_center")]
    pub side: ThinSide,
    /// With `side: two_sides`, the thickness outside (expression).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(length = "Thickness 2")]
    pub thickness2: Option<String>,
}

/// Where a thin wall lies against its curves. A closed loop's inside is the area it encloses;
/// an open chain's is the side it bends toward (a straight one's, the left of its first
/// curve as drawn).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinSide {
    /// Half the thickness either side of the curves.
    #[default]
    Center,
    Inside,
    Outside,
    /// `thickness` inside and `thickness2` outside.
    TwoSides,
}

impl ThinSide {
    pub fn is_center(&self) -> bool {
        *self == ThinSide::Center
    }
    pub fn parse(s: &str) -> Option<ThinSide> {
        match s.trim().to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
            "center" | "centre" | "centered" | "centred" | "symmetric" => Some(ThinSide::Center),
            "inside" | "side1" | "side_1" => Some(ThinSide::Inside),
            "outside" | "side2" | "side_2" => Some(ThinSide::Outside),
            "two_sides" | "both" => Some(ThinSide::TwoSides),
            _ => None,
        }
    }
}

/// Revolve axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisRef {
    /// A line of the profile's sketch.
    SketchLine { curve: String },
    /// Sketch x or y axis through the sketch origin (`x` / `y`).
    SketchAxis { axis: String },
    /// World X / Y / Z axis through the origin.
    World { axis: String },
    /// Any line in the sketch plane, in world coordinates.
    Line {
        #[walk(pos)]
        origin: Vec3,
        #[walk(dir)]
        dir: Vec3,
    },
}

/// Persistent cylindrical Emboss selection, authored in the feature component frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct EmbossFace {
    #[walk(body)]
    pub body: String,
    #[walk(pinned)]
    pub at: Vec3,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_path: Vec<u64>,
}

impl EmbossFace {
    /// The selected sketch placement, shared by evaluation and interactive guides.
    pub fn sketch_frame(&self, doc: &Document, author: u64, sketch_owner: u64, body: &crate::ModelBody) -> Result<crate::Mat> {
        let owner = doc.model_body_component(body);
        if sketch_owner == owner {
            return Self::component_frame(doc, owner, &self.path);
        }
        if sketch_owner == author {
            return Self::component_frame(doc, author, &self.author_path);
        }
        let path = doc.first_path(sketch_owner);
        let mut component = sketch_owner;
        for _ in 0..64 {
            if component == 0 {
                break;
            }
            let mut occurrences = doc.occurrences.iter().filter(|o| o.component == component);
            let occurrence = occurrences.next().ok_or_else(|| DocError::Invalid("sketch placement no longer exists".into()))?;
            if occurrences.next().is_some() {
                return Err(DocError::Invalid("sketch placement is ambiguous".into()));
            }
            component = occurrence.parent;
        }
        if component != 0 {
            return Err(DocError::Invalid("sketch placement exceeds depth limit".into()));
        }
        Self::component_frame(doc, sketch_owner, &path)
    }

    /// Resolve this exact placement without expanding every assembly instance.
    pub fn body_frame(&self, doc: &Document, body: &crate::ModelBody) -> Result<crate::Mat> {
        let owner = doc.model_body_component(body);
        let mut frame = Self::component_frame(doc, owner, &self.path)?;
        if let Some(offset) = doc.model_body_offset(body) {
            frame = crate::mat_mul(&frame, offset);
        }
        Ok(frame)
    }

    /// Resolve one saved component path without expanding other occurrences.
    pub fn component_frame(doc: &Document, owner: u64, path: &[u64]) -> Result<crate::Mat> {
        if path.len() > 64 || doc.occurrences.len() > 100_000 {
            return Err(DocError::Invalid("selected cylinder path exceeds work limits".into()));
        }
        let mut frame = crate::IDENTITY;
        let mut component = 0;
        let mut seen = std::collections::BTreeSet::new();
        for id in path {
            if *id == 0 || !seen.insert(*id) {
                return Err(DocError::Invalid("invalid selected cylinder path".into()));
            }
            let occurrence = doc
                .occurrences
                .iter()
                .find(|o| o.id == *id && o.parent == component)
                .ok_or_else(|| DocError::Invalid("selected cylinder placement no longer exists".into()))?;
            frame = crate::mat_mul(&frame, &occurrence.transform);
            component = occurrence.component;
        }
        if path.is_empty() {
            frame = doc.component_transform(owner);
        } else if component != owner {
            return Err(DocError::Invalid("selected cylinder path belongs to another component".into()));
        }
        Ok(frame)
    }
}

/// One edge of an edge flange feature: a point on the sheet's edge and, where given, this
/// edge's own angle and height (expressions) in place of the feature's. In a design file an
/// edge with neither is the point `[x, y, z]`, as every edge was before; one with its own
/// values is `{"point": [x, y, z], "angle": "…", "height": "…"}`.
#[derive(Clone, Debug, Default, PartialEq, solidcraft_derive::Walk)]
pub struct FlangeEdge {
    #[walk(pos)]
    pub point: Vec3,
    #[walk(angle = "Edge angle")]
    pub angle: Option<String>,
    #[walk(length = "Edge height")]
    pub height: Option<String>,
}

impl FlangeEdge {
    /// An edge that takes the feature's angle and height.
    pub fn at(point: Vec3) -> Self {
        Self { point, angle: None, height: None }
    }
}

impl Serialize for FlangeEdge {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        if self.angle.is_none() && self.height.is_none() {
            return self.point.serialize(s);
        }
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("point", &self.point)?;
        if let Some(a) = &self.angle {
            m.serialize_entry("angle", a)?;
        }
        if let Some(h) = &self.height {
            m.serialize_entry("height", h)?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for FlangeEdge {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
        /// The object form; a key this build doesn't know is refused, not dropped.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Own {
            point: Vec3,
            #[serde(default)]
            angle: Option<String>,
            #[serde(default)]
            height: Option<String>,
        }
        struct Edge;
        impl<'de> serde::de::Visitor<'de> for Edge {
            type Value = FlangeEdge;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a point [x, y, z] or an edge {point, angle?, height?}")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> std::result::Result<FlangeEdge, A::Error> {
                Vec3::deserialize(SeqAccessDeserializer::new(seq)).map(FlangeEdge::at)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> std::result::Result<FlangeEdge, A::Error> {
                let o = Own::deserialize(MapAccessDeserializer::new(map))?;
                Ok(FlangeEdge { point: o.point, angle: o.angle, height: o.height })
            }
        }
        d.deserialize_any(Edge)
    }
}

/// Feature definitions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeatureKind {
    /// A recorded timeline ownership boundary. Geometry is converted from its
    /// current local frame into the destination component frame exactly once.
    BodyTransfer {
        #[walk(body)]
        bodies: Vec<String>,
        from: u64,
        to: u64,
        transform: crate::Mat,
    },
    Sketch {
        plane: PlaneRef,
        sketch: Sketch,
    },
    Extrude {
        /// Feature id of the sketch.
        #[walk(sketch)]
        sketch: u64,
        // A thin extrude follows curves; its profiles are not used.
        #[walk(profiles_of = sketch, when = thin.is_none())]
        profiles: ProfileSel,
        extent: Extent,
        #[serde(default)]
        operation: Operation,
        /// Target body names for join/cut/intersect (empty = all bodies).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
        /// Thin: a wall along sketch curves instead of the profiles (which are then unused).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thin: Option<ThinWall>,
    },
    Revolve {
        #[walk(sketch)]
        sketch: u64,
        #[walk(profiles_of = sketch)]
        profiles: ProfileSel,
        axis: AxisRef,
        #[walk(angle = "Angle")]
        angle: String,
        #[serde(default)]
        operation: Operation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
        /// Two Sides: the angle on the other side.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Angle 2")]
        angle2: Option<String>,
        /// Symmetric: `angle` split evenly either side of the profile.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        symmetric: bool,
        /// Missing fields retain the angular placement of existing designs.
        #[serde(default)]
        convention: RevolveConvention,
        /// To Object: turn until the face, vertex or point here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(pick)]
        to: Option<Vec3>,
    },
    Fillet {
        /// Reference points on the edges to round (each re-finds the nearest edge).
        #[walk(pos)]
        edges: Vec<Vec3>,
        #[walk(length = style.radius_label())]
        radius: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        /// Constant radius (the default), chord length (`radius` is the width across), or a
        /// radius varying along each edge.
        #[serde(default, skip_serializing_if = "FilletStyle::is_constant")]
        style: FilletStyle,
        /// What to do when the radius is too large for the geometry around the edges.
        #[serde(default, skip_serializing_if = "FilletFit::is_exact")]
        fit: FilletFit,
    },
    Chamfer {
        #[walk(pos)]
        edges: Vec<Vec3>,
        #[walk(length = "Distance")]
        distance: String,
        /// Unequal chamfers: the distance along the second face…
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Distance 2")]
        distance2: Option<String>,
        /// …or the angle the chamfer makes with the first face.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Angle")]
        angle: Option<String>,
        /// Swap which face is the first (by default the one facing up most).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    Box {
        #[serde(with = "point_expr")]
        #[walk(placement)]
        corner: PointExpr,
        #[walk(length = "Length")]
        length: String,
        #[walk(length = "Width")]
        width: String,
        #[walk(length = "Height")]
        height: String,
        #[serde(default)]
        operation: Operation,
    },
    Cylinder {
        #[serde(with = "point_expr")]
        #[walk(placement)]
        base: PointExpr,
        #[serde(default = "z_axis")]
        #[walk(dir)]
        axis: Vec3,
        #[walk(length = "Radius")]
        radius: String,
        #[walk(length = "Height")]
        height: String,
        #[serde(default)]
        operation: Operation,
    },
    Sphere {
        #[serde(with = "point_expr")]
        #[walk(placement)]
        center: PointExpr,
        #[walk(length = "Radius")]
        radius: String,
        #[serde(default)]
        operation: Operation,
    },
    Torus {
        #[serde(with = "point_expr")]
        #[walk(placement)]
        center: PointExpr,
        #[walk(length = "Major radius")]
        major: String,
        #[walk(length = "Minor radius")]
        minor: String,
        #[serde(default)]
        operation: Operation,
    },
    Combine {
        #[walk(body)]
        target: String,
        #[walk(body)]
        tools: Vec<String>,
        operation: Operation,
        #[serde(default)]
        keep_tools: bool,
    },
    /// Copies of other features' results, or of bodies (rectangular, circular or along a path).
    Pattern {
        #[walk(feature)]
        features: Vec<String>,
        pattern: PatternKind,
        /// Copy these bodies (as new bodies) instead of replaying features.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        bodies: Vec<String>,
    },
    /// Mirror copies of other features' results in a plane.
    Mirror {
        #[walk(feature)]
        features: Vec<String>,
        plane: PlaneRef,
        /// Mirror these bodies instead of replaying features.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        bodies: Vec<String>,
        /// Join each mirrored body with its original.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        combine: bool,
    },
    /// A drilled hole (simple, counterbore or countersink) at a point, into the material.
    Hole {
        #[walk(pick_if = points.is_none())]
        position: Vec3,
        /// Drilling direction (into the material).
        #[walk(dir)]
        direction: Vec3,
        #[walk(length = "Diameter")]
        diameter: String,
        /// Depth to the shoulder; `None` = through all.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Depth")]
        depth: Option<String>,
        #[serde(default)]
        hole: HoleKind,
        /// Drill at these sketch points instead (one hole each, perpendicular to the sketch, into the material: against the sketch normal, or along it when only that side has material);
        /// the holes follow the points when the sketch changes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        points: Option<SketchPoints>,
        /// A cosmetic thread in the holes (e.g. "M6x1").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread: Option<String>,
        /// To Object: drill down to the face (or point) here instead of `depth`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(pick)]
        to: Option<Vec3>,
    },
    /// A thread on a cylindrical face. Cosmetic by default (no geometry change; drawn and
    /// exported as an annotation); `modeled` cuts the real helical ISO profile into the face.
    /// `designation` is an ISO metric size such as "M8" or "M8x1".
    Thread {
        #[walk(pick)]
        face: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        designation: Option<String>,
        /// Thread length from the face's end nearest `face`; `None` = full length.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Length")]
        length: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        modeled: bool,
    },
    /// Ruled loft through profiles of several sketches, in order.
    Loft {
        sections: Vec<LoftSection>,
        #[serde(default)]
        operation: Operation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
    },
    /// Sweep profiles along a chain of sketch curves.
    Sweep {
        #[walk(sketch)]
        sketch: u64,
        #[walk(profiles_of = sketch)]
        profiles: ProfileSel,
        #[walk(sketch)]
        path_sketch: u64,
        path: Vec<String>,
        #[serde(default)]
        operation: Operation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
    },
    /// Hollow a body leaving walls of `thickness`, removing the faces at the given points.
    Shell {
        #[walk(pick)]
        faces: Vec<Vec3>,
        #[walk(length = "Thickness")]
        thickness: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        /// inside (default) | outside | both (the wall straddles the faces).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        direction: String,
        /// Open the faces smoothly joined to the picked ones too.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        tangent_chain: bool,
    },
    /// Tilt faces (at the given points) about a neutral plane, leaning toward `pull`.
    Draft {
        #[walk(pick)]
        faces: Vec<Vec3>,
        #[walk(angle = "Angle")]
        angle: String,
        neutral: PlaneRef,
        #[walk(dir)]
        pull: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// A construction plane (sketch placement, split tool).
    ConstructionPlane {
        plane: PlaneRef,
    },
    /// Split a body into the parts on either side of a plane, or of a face of a body (the face
    /// through `point`, its surface extended) when `tool` is set.
    Split {
        #[walk(body)]
        body: String,
        plane: PlaneRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<FaceAt>,
    },
    /// A construction axis (Fusion's CONSTRUCT axes).
    ConstructionAxis {
        def: AxisDef,
    },
    /// A construction point (Fusion's CONSTRUCT points).
    ConstructionPoint {
        def: PointDef,
    },
    /// Split faces (at the given points) where a tool meets them, the solid unchanged: a plane,
    /// a face of a body (`tool`, its surface extended) or a sketch's curves (`sketch`, swept
    /// along its normal). The pieces are named after the face (`#k`).
    SplitFace {
        #[walk(pick)]
        faces: Vec<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        plane: PlaneRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<FaceAt>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(sketch)]
        sketch: Option<u64>,
    },
    /// Scale bodies about a point (uniform, or per axis).
    Scale {
        #[walk(body)]
        bodies: Vec<String>,
        #[serde(default)]
        #[walk(pos)]
        origin: Vec3,
        #[walk(unitless = "Scale factor")]
        factor: String,
        /// Per-axis factors instead of `factor`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(unitless = ["X scale", "Y scale", "Z scale"])]
        factors: Option<[String; 3]>,
    },
    /// Move planar faces (at the given points) along their normals; the body keeps its shape.
    OffsetFace {
        #[walk(pick)]
        faces: Vec<Vec3>,
        #[walk(length = "Distance")]
        distance: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Remove faces (at the given points) and heal the gap from the neighbouring planes, as when
    /// a fillet, chamfer, hole or boss is taken off. A picked face brings the rest of its surface
    /// (both halves of a hole's wall).
    DeleteFace {
        #[walk(pick)]
        faces: Vec<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        /// Close the gap by extending the neighbouring faces (the default); `false` leaves it
        /// open, the faces left a surface body.
        #[serde(default = "yes", skip_serializing_if = "is_yes")]
        heal: bool,
    },
    /// A box around bodies, grown by a margin.
    BoundingSolid {
        #[walk(body)]
        bodies: Vec<String>,
        #[serde(default = "zero_expr")]
        #[walk(length = "Margin")]
        margin: String,
    },
    /// A tube along a chain of sketch curves (solid, or hollow with a wall thickness).
    Pipe {
        #[walk(sketch)]
        path_sketch: u64,
        path: Vec<String>,
        #[walk(length = "Diameter")]
        diameter: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Wall")]
        wall: Option<String>,
        #[serde(default)]
        operation: Operation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
    },
    /// Raise or sink profiles on a planar face, or wrap them on a selected cylindrical wall.
    Emboss {
        #[walk(sketch)]
        sketch: u64,
        #[walk(profiles_of = sketch)]
        profiles: ProfileSel,
        #[walk(length = "Depth")]
        depth: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        deboss: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<EmbossFace>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
    },
    /// A thin wall from open sketch curves: `thickness` across the sketch plane, filling from
    /// the curves toward the body (to the next face, or `depth`). A web makes one per curve.
    Rib {
        #[walk(sketch)]
        sketch: u64,
        curves: Vec<String>,
        #[walk(length = "Thickness")]
        thickness: String,
        /// Depth from the curves; `None` = to the next face of the body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Depth")]
        depth: Option<String>,
        /// Fill toward the other side of the curves.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        /// Web: each curve is its own wall.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        web: bool,
    },
    /// Move planar faces (at the given points) onto a target plane parallel to them.
    ReplaceFace {
        #[walk(pick)]
        faces: Vec<Vec3>,
        target: PlaneRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Move bodies so that `from` lands on `to` (and, with normals, the faces meet face to face).
    Align {
        #[walk(body)]
        bodies: Vec<String>,
        #[walk(pos)]
        from: Vec3,
        #[walk(pos)]
        to: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(dir)]
        from_normal: Option<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(dir)]
        to_normal: Option<Vec3>,
        /// Normals the same way instead of facing each other.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
    },
    /// A helical coil: a circular or square section swept `turns` times about `axis`, rising
    /// `pitch` per turn, on a helix of `diameter` (the section's centre, or its inside or outside).
    Coil {
        #[serde(with = "point_expr")]
        #[walk(placement)]
        base: PointExpr,
        #[serde(default = "z_axis")]
        #[walk(dir)]
        axis: Vec3,
        #[walk(length = "Diameter")]
        diameter: String,
        #[walk(length = "Pitch")]
        pitch: String,
        #[walk(unitless = "Revolutions")]
        turns: String,
        #[walk(length = "Section size")]
        section_size: String,
        #[serde(default)]
        section: CoilSection,
        /// -1 inside, 0 on the centre, 1 outside of the diameter.
        #[serde(default, skip_serializing_if = "is_zero_i8")]
        position: i8,
        /// Start angle about the axis (expression; 0 = from the axis toward +x, or a
        /// perpendicular direction when the axis is x).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Start angle")]
        start_angle: Option<String>,
        /// Turn the other way (left-handed).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        clockwise: bool,
        #[serde(default)]
        operation: Operation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
    },
    /// Sheet metal base flange: a sheet from a closed profile (thickness from the rule).
    SheetBase {
        #[walk(sketch)]
        sketch: u64,
        #[walk(profiles_of = sketch)]
        profiles: ProfileSel,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
        /// Thickness grows against the sketch normal.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
    },
    /// Sheet metal contour flange: a sheet bent along an open chain of sketch lines.
    SheetContour {
        #[walk(sketch)]
        sketch: u64,
        curves: Vec<String>,
        #[walk(length = "Distance")]
        distance: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
        /// Material on the other side of the chain.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        /// Width the other way along the sketch normal.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reverse: bool,
    },
    /// Edge flanges on sheet edges (points on the top or bottom edges). `height` and `angle`
    /// hold for every edge that doesn't carry its own (see [`FlangeEdge`]).
    SheetFlange {
        #[walk(after = radius)]
        edges: Vec<FlangeEdge>,
        #[walk(length = "Height")]
        height: String,
        #[walk(angle = "Angle")]
        angle: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Bend radius")]
        radius: Option<String>,
        /// inside | outside | middle.
        #[serde(default = "inside_str")]
        position: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Flat hems on sheet edges.
    SheetHem {
        #[walk(pos)]
        edges: Vec<Vec3>,
        #[walk(length = "Length")]
        length: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Gap")]
        gap: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// A perforation field (#732): round holes of `diameter` on a staggered (60°) or square grid of `pitch` over the
    /// whole flat sheet, kept `margin` (default: the diameter) clear of its edges, cut-outs and bend zones. Stored like
    /// a cosmetic thread: the holes go to the flat pattern, the DXF and the measured volume, not into the B-rep, so a
    /// panel can carry tens of thousands of them.
    SheetPerforate {
        #[walk(length = "Diameter")]
        diameter: String,
        #[walk(length = "Pitch")]
        pitch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Margin")]
        margin: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Fold a sheet along a line on its base face: a sketch line (followed when the sketch
    /// changes), else the points `a`, `b`.
    SheetFold {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(sketch)]
        sketch: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        curve: Option<String>,
        #[walk(pos)]
        a: Vec3,
        #[walk(pos)]
        b: Vec3,
        #[walk(angle = "Angle")]
        angle: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Bend radius")]
        radius: Option<String>,
        /// centerline | start | end | mould
        position: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        /// A point on the side that stays put (default: the larger side).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(pos)]
        fixed: Option<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Unfold a sheet body flat (all bends), or refold it.
    SheetUnfold {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        refold: bool,
    },
    /// Convert a body to sheet metal from its fixed face at `face`: a plate of constant
    /// thickness or, with `bends`, a solid or shelled body whose faces reached from the fixed
    /// face across those edges become the sheet.
    SheetConvert {
        #[walk(body)]
        body: String,
        #[walk(pick_if = !bends.is_empty())]
        face: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
        /// Points on the sharp edges that become bends.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(pos)]
        bends: Vec<Vec3>,
    },
    /// Plastic boss: a round post on a face (draft, root fillet, hole, ribs).
    Boss {
        #[walk(pick)]
        position: Vec3,
        /// Out of the face (the boss grows along it).
        #[serde(default = "z_axis")]
        #[walk(dir)]
        direction: Vec3,
        #[walk(length = "Diameter")]
        diameter: String,
        #[walk(length = "Height")]
        height: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Hole diameter")]
        hole_diameter: Option<String>,
        /// Hole depth from the top (default: down to the face).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Hole depth")]
        hole_depth: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Draft angle")]
        draft: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Root fillet")]
        fillet: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(unitless = "Ribs")]
        ribs: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Rib thickness")]
        rib_thickness: Option<String>,
        /// Rib reach out from the boss wall.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Rib length")]
        rib_length: Option<String>,
        /// Rib top below the boss top.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Rib offset")]
        rib_offset: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Plastic lip (raised band) or groove (recess) along the rim face picked at `face`.
    Lip {
        #[walk(pos)]
        face: Vec3,
        #[walk(length = "Width")]
        width: String,
        #[walk(length = "Height")]
        height: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        groove: bool,
        /// Clearance added to a groove's width (and depth).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Gap")]
        gap: Option<String>,
        /// Along the rim's outer edge instead of its inner one.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        outside: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Plastic snap fit: a cantilever arm on a face with a hook (catch) at its tip.
    SnapFit {
        #[walk(pick)]
        position: Vec3,
        /// The arm grows along this (out of the face).
        #[serde(default = "z_axis")]
        #[walk(dir)]
        direction: Vec3,
        /// The catch sticks out this way.
        #[walk(dir)]
        hook: Vec3,
        #[walk(length = "Length")]
        length: String,
        #[walk(length = "Thickness")]
        thickness: String,
        #[walk(length = "Width")]
        width: String,
        #[walk(length = "Catch depth")]
        catch_depth: String,
        /// The catch's height along the arm.
        #[walk(length = "Catch length")]
        catch_length: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Plastic rest: a raised pad on a face (round, or a rectangle), solid or a hollow wall,
    /// with draft.
    Rest {
        #[walk(pick)]
        position: Vec3,
        /// Out of the face (the pad grows along it).
        #[serde(default = "z_axis")]
        #[walk(dir)]
        direction: Vec3,
        /// The rectangle's length runs along this (default: any direction in the face).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(dir)]
        along: Option<Vec3>,
        /// Diameter of a round rest, or the rectangle's width.
        #[walk(length = "Width")]
        width: String,
        /// The rectangle's length (none: round).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Length", after = height)]
        length: Option<String>,
        #[walk(length = "Height")]
        height: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Draft angle")]
        draft: Option<String>,
        /// Wall thickness of a hollow rest (none: solid).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Wall thickness")]
        thickness: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
    },
    /// Boundary Fill: solids from the cells the tool bodies enclose (a cell: inside some
    /// tools and outside the rest), each picked by a point inside it.
    BoundaryFill {
        #[walk(body)]
        tools: Vec<String>,
        #[walk(pos)]
        cells: Vec<Vec3>,
        #[serde(default)]
        operation: Operation,
        /// Remove the tool bodies afterwards.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        remove_tools: bool,
    },
    /// Remove bodies from the model (from here on in the timeline).
    Remove {
        #[walk(body)]
        bodies: Vec<String>,
    },
    /// Surface bodies: planar patches filling sketch regions (`profiles` of `sketch`), or a
    /// closed loop of a body's edges (`edges`: points on them, on `body`).
    Patch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(sketch)]
        sketch: Option<u64>,
        #[serde(default = "all_profiles")]
        #[walk(profiles_of = sketch)]
        profiles: ProfileSel,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(pos)]
        edges: Vec<Vec3>,
    },
    /// Surface bodies sewn along shared edges into the first one (a solid when they close).
    Stitch {
        #[walk(body)]
        bodies: Vec<String>,
        #[serde(default = "zero_expr")]
        #[walk(length = "Tolerance")]
        tolerance: String,
    },
    /// Surface bodies thickened into solids along their normals (by half each way when
    /// `symmetric`).
    Thicken {
        #[walk(body)]
        bodies: Vec<String>,
        #[walk(length = "Thickness")]
        thickness: String,
        #[serde(default)]
        symmetric: bool,
        operation: Operation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(body)]
        targets: Vec<String>,
    },
    /// A surface body swept straight along the sketch normal from open curve chains and the
    /// boundaries of profiles: walls only, no caps.
    SurfaceExtrude {
        #[walk(sketch)]
        sketch: u64,
        /// Profiles whose boundary loops are swept (none: only `curves`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(profiles_of = sketch)]
        profiles: Option<ProfileSel>,
        /// Sketch curve ids, grouped into chains of curves that meet end to end.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        curves: Vec<String>,
        extent: Extent,
    },
    /// A body taken apart: each face at `faces` (every face when empty) its own surface body,
    /// the faces left one surface body under the body's name. A solid comes apart into surfaces.
    Unstitch {
        #[walk(body)]
        body: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(pos)]
        faces: Vec<Vec3>,
    },
    /// A surface body with the faces at `faces` (every face when empty) turned over.
    ReverseNormal {
        #[walk(body)]
        body: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(pos)]
        faces: Vec<Vec3>,
    },
    /// The face at `face` restored towards its surface: the inner loops at the `holes` points
    /// taken away (every one when `None`; none when empty), and with `outer` its outer boundary
    /// back at the surface's natural bounds (an analytic surface's: the face's own span). A new
    /// surface body; with `in_place`, a surface body's face is untrimmed where it is.
    SurfaceUntrim {
        // The face is picked (and named); the holes are picked by points on their edges.
        #[walk(picks_faces)]
        face: FaceAt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(pos)]
        holes: Option<Vec<Vec3>>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        outer: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        in_place: bool,
    },
    /// A surface body turned about an axis from open curve chains and the boundaries of
    /// profiles: walls only, no caps (a solid where a full turn closes them).
    SurfaceRevolve {
        #[walk(sketch)]
        sketch: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(profiles_of = sketch)]
        profiles: Option<ProfileSel>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        curves: Vec<String>,
        axis: AxisRef,
        #[walk(angle = "Angle")]
        angle: String,
        /// Two Sides: the angle on the other side.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Angle 2")]
        angle2: Option<String>,
        /// Symmetric: `angle` split evenly either side of the sketch.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        symmetric: bool,
    },
    /// A surface body swept along a chain of sketch curves from open curve chains and the
    /// boundaries of profiles: walls only, no caps.
    SurfaceSweep {
        #[walk(sketch)]
        sketch: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(profiles_of = sketch)]
        profiles: Option<ProfileSel>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        curves: Vec<String>,
        #[walk(sketch)]
        path_sketch: u64,
        path: Vec<String>,
    },
    /// A surface body lofted through sections in order, each one open or closed chain of
    /// sketch curves: walls only, no caps.
    SurfaceLoft {
        sections: Vec<SheetSection>,
    },
    /// The faces at `faces` copied out at a signed `distance` along their normals as a new
    /// surface body (0 copies them); `chain` takes each pick's whole smooth chain of faces;
    /// fitted faces within `tolerance`.
    SurfaceOffset {
        #[walk(pick)]
        faces: Vec<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(body)]
        body: Option<String>,
        #[walk(length = "Distance")]
        distance: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        chain: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Tolerance")]
        tolerance: Option<String>,
    },
    /// A strip of ruled surface `distance` wide hanging off the chain of `body`'s edges through
    /// `edges`, as a new surface body: along `mode` (`angle` from `direction`, or from the
    /// face's normal; along `direction`), from the face at `face` where it borders the chain;
    /// `flip` puts it on the other side; sharp corners as `corners` says.
    SurfaceRuled {
        #[walk(body)]
        body: String,
        #[walk(pos)]
        edges: Vec<Vec3>,
        #[walk(length = "Distance")]
        distance: String,
        #[serde(default, skip_serializing_if = "RuledMode::is_normal")]
        mode: RuledMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Angle")]
        angle: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(dir)]
        direction: Option<Vec3>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        #[serde(default, skip_serializing_if = "RuledCorners::is_mitre")]
        corners: RuledCorners,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(pick)]
        face: Option<Vec3>,
    },
    /// A surface body's boundary edges through `edges` extended outward: by `distance`, or up
    /// to `up_to`; the surface continued (`mode` natural) or a run-off face along each edge
    /// (tangent, curvature). `neighbours`: the edges next to an extended edge are lengthened to
    /// meet it (else the extension's sides run square to the edge).
    SurfaceExtend {
        #[walk(body)]
        body: String,
        #[walk(pos)]
        edges: Vec<Vec3>,
        #[walk(length = "Distance")]
        distance: String,
        #[serde(default, skip_serializing_if = "ExtendMode::is_natural")]
        mode: ExtendMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        up_to: Option<ExtendTo>,
        #[serde(default = "yes", skip_serializing_if = "is_yes")]
        neighbours: bool,
    },
    /// A surface body trimmed by a plane, or by a face of a body (`tool`), keeping the side of
    /// `keep`.
    SurfaceTrim {
        #[walk(body)]
        body: String,
        plane: PlaneRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<FaceAt>,
        #[walk(pos)]
        keep: Vec3,
    },
    Move {
        #[walk(body)]
        bodies: Vec<String>,
        #[walk(length = ["X distance", "Y distance", "Z distance"])]
        translate: [String; 3],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(dir)]
        rotate_axis: Option<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(angle = "Angle")]
        angle: Option<String>,
        /// When set, the original bodies are kept and the transform is applied to a new copy
        /// of each instead of moving them in place.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        copy: bool,
    },
    /// A non-parametric base feature: bodies imported from a STEP file. The STEP text is kept
    /// in the design (it is the B-rep), so the design reopens without the original file.
    Import {
        /// Source file name (display only).
        file: String,
        /// The STEP (ISO 10303-21) text, shared by every copy of the design (undo states,
        /// previews, scratch sessions): a large file is held once, not once per edit.
        step: std::sync::Arc<str>,
        /// The file's product/assembly tree, kept for components.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        components: Vec<solidcraft_kernel::StepNode>,
    },
    /// Rebuild an upstream constant-section mesh with analytic planar/cylindrical faces.
    MeshConvert {
        #[walk(body)]
        body: String,
        #[walk(length = "Tolerance")]
        tolerance: String,
    },
    /// Mesh bodies imported from a 3MF or STL file, embedded in the design.
    MeshImport {
        /// Source file name (display only).
        file: String,
        meshes: Vec<MeshData>,
    },
}

/// How Extend continues a surface past its edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtendMode {
    /// The surface itself, continued.
    #[default]
    Natural,
    /// A ruled run-off in the surface's tangent plane (G1).
    Tangent,
    /// A run-off that keeps the surface's curvature across the edge (G2).
    Curvature,
}

impl ExtendMode {
    pub fn is_natural(&self) -> bool {
        *self == ExtendMode::Natural
    }
}

/// What Extend extends up to instead of a distance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
#[serde(rename_all = "snake_case")]
pub enum ExtendTo {
    /// A plane.
    Plane(PlaneRef),
    /// The surface of a body's face (extended).
    Face(FaceAt),
    /// The surface's own bounds (natural only): a trim undone.
    Bounds,
}

/// How a fillet's size is given.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilletStyle {
    #[default]
    Constant,
    /// `radius` is the chord: the width across the blend.
    Chord,
    /// G2 quintic blend of planar faces along a straight edge. `radius` sets the
    /// equivalent circular setback, not a constant curvature radius.
    Curvature,
    /// `radius` at the end of each edge nearest `start`, `radius2` at the other; `mid` radii at
    /// positions between (0 to 1 from the start end); straight between them, or `smooth`.
    Variable {
        #[walk(length = "End radius")]
        radius2: String,
        #[walk(pos)]
        start: Vec3,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[walk(length = "Mid radius")]
        mid: Vec<(f64, String)>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        smooth: bool,
    },
    /// The radius along each edge as a clamped B-spline of the position from the end nearest
    /// `start` (0) to the other (1): `radius` is its first control radius and `radii` the rest,
    /// its degree what `knots` leave (knots = control radii + degree + 1).
    Spline {
        #[walk(pos)]
        start: Vec3,
        knots: Vec<f64>,
        #[walk(length = |i: usize, n: usize| if i + 1 == n { "End radius" } else { "Control radius" })]
        radii: Vec<String>,
    },
}

/// A fillet radius too large for the geometry around its edges: refused (`Exact`, the default),
/// reduced to the largest that works for all of them (`Reduce`), or tapered along each edge it
/// doesn't fit to the room there, down to zero where a face closes to a point (`Taper`); a
/// warning says what was done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilletFit {
    #[default]
    Exact,
    Reduce,
    Taper,
}

impl FilletFit {
    pub fn is_exact(&self) -> bool {
        *self == FilletFit::Exact
    }
}

impl FilletStyle {
    pub fn is_constant(&self) -> bool {
        *self == FilletStyle::Constant
    }

    /// What the feature's `radius` is in this style (the label of its parameter).
    pub fn radius_label(&self) -> &'static str {
        match self {
            FilletStyle::Constant => "Radius",
            FilletStyle::Curvature => "Equivalent radius",
            FilletStyle::Chord => "Chord length",
            FilletStyle::Variable { .. } | FilletStyle::Spline { .. } => "Start radius",
        }
    }

    /// The spline of a [`FilletStyle::Spline`] with its control radii `values` (the first is
    /// the feature's `radius`).
    pub fn spline(knots: &[f64], values: Vec<f64>) -> solidcraft_kernel::RadiusSpline {
        let degree = knots.len().saturating_sub(values.len() + 1);
        solidcraft_kernel::RadiusSpline { degree, knots: knots.to_vec(), radii: values }
    }
}

/// A face picked by a point on it, on a named body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct FaceAt {
    #[walk(body)]
    pub body: String,
    #[walk(pos)]
    pub point: Vec3,
}

/// Triangles of one imported mesh body (millimetres).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshData {
    pub name: String,
    /// x, y, z per vertex.
    pub positions: Vec<f64>,
    /// Three vertex indices per triangle.
    pub triangles: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
}

/// Points of a sketch, by id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct SketchPoints {
    #[walk(sketch)]
    pub sketch: u64,
    pub ids: Vec<String>,
    /// Drill the other way: away from the side the holes would go into by themselves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
}

/// A component: a node of the design tree that owns sketches, features and bodies. The root
/// (id 0) is the design itself and is not stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub id: u64,
    pub name: String,
    /// Parent component (0 = the root).
    #[serde(default)]
    pub parent: u64,
}

/// One loft section: profiles of a sketch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct LoftSection {
    #[walk(sketch)]
    pub sketch: u64,
    #[walk(profiles_of = sketch, when = point.is_none())]
    pub profiles: ProfileSel,
    /// A sketch point instead of a profile (the loft closes to it, as a pyramid's apex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<String>,
}

/// A section of a surface loft: one chain from a sketch (one profile's boundary, or curves
/// meeting end to end).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
pub struct SheetSection {
    #[walk(sketch)]
    pub sketch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[walk(profiles_of = sketch)]
    pub profiles: Option<ProfileSel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<String>,
}

/// Coil cross-sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoilSection {
    #[default]
    Circular,
    Square,
}

fn is_zero_i8(v: &i8) -> bool {
    *v == 0
}

/// Hole shapes.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize, solidcraft_derive::Walk)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoleKind {
    /// Plain hole; blind holes get a drill point when `tip_angle` is set.
    #[default]
    Simple,
    Drilled {
        #[walk(angle = "Tip angle")]
        tip_angle: String,
    },
    Counterbore {
        #[walk(length = "Counterbore diameter")]
        cb_diameter: String,
        #[walk(length = "Counterbore depth")]
        cb_depth: String,
    },
    Countersink {
        #[walk(length = "Countersink diameter")]
        cs_diameter: String,
        #[walk(angle = "Countersink angle")]
        cs_angle: String,
    },
}

/// Pattern layouts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, solidcraft_derive::Walk)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternKind {
    Rectangular {
        #[walk(dir)]
        dir1: Vec3,
        #[walk(unitless = "Quantity")]
        count1: String,
        #[walk(length = "Spacing")]
        spacing1: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(dir)]
        dir2: Option<Vec3>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(unitless = "Quantity 2")]
        count2: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[walk(length = "Spacing 2")]
        spacing2: Option<String>,
    },
    Circular {
        #[walk(pos)]
        origin: Vec3,
        #[walk(dir)]
        axis: Vec3,
        #[walk(unitless = "Quantity")]
        count: String,
        /// Total angle (360 deg = evenly around).
        #[walk(angle = "Total angle")]
        angle: String,
    },
    /// Copies along a chain of sketch curves: instance k moves by path(k·spacing) − path(0),
    /// measured along the path from its start.
    Path {
        #[walk(sketch)]
        path_sketch: u64,
        path: Vec<String>,
        #[walk(unitless = "Quantity")]
        count: String,
        /// Distance along the path between instances, or the whole extent when `extent`.
        #[walk(length = "Distance")]
        spacing: String,
        /// `spacing` is the distance from the first to the last instance.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        extent: bool,
        /// Run from the path's other end.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        /// Turn copies with the path's direction (else they keep their orientation).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        orient: bool,
    },
}

fn z_axis() -> Vec3 {
    Vec3::Z
}

/// A point whose coordinates are length expressions (`["basin_width / 2", "10 mm", "0"]`), so
/// it follows parameter changes. Primitives are placed by one.
pub type PointExpr = [String; 3];

/// A point as expressions (plain numbers, mm).
pub fn point_expr(p: Vec3) -> PointExpr {
    [format!("{}", p.x), format!("{}", p.y), format!("{}", p.z)]
}

/// Stored as `[x, y, z]`: plain numbers as numbers (as points were before they took
/// expressions, so such designs read and write the same), anything else as a string.
mod point_expr {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum Coord {
        Num(f64),
        Expr(String),
    }

    pub fn serialize<S: Serializer>(p: &super::PointExpr, s: S) -> Result<S::Ok, S::Error> {
        let c = |e: &String| match e.parse::<f64>() {
            Ok(v) if v.is_finite() && format!("{v}") == *e => Coord::Num(v),
            _ => Coord::Expr(e.clone()),
        };
        [c(&p[0]), c(&p[1]), c(&p[2])].serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<super::PointExpr, D::Error> {
        let c = |x: Coord| match x {
            Coord::Num(v) => format!("{v}"),
            Coord::Expr(e) => e,
        };
        let [x, y, z] = <[Coord; 3]>::deserialize(d)?;
        Ok([c(x), c(y), c(z)])
    }
}

impl FeatureKind {
    /// A sheet metal feature (its result depends on the sheet metal rules).
    pub fn is_plastic(&self) -> bool {
        matches!(self, FeatureKind::Boss { .. } | FeatureKind::Lip { .. } | FeatureKind::SnapFit { .. } | FeatureKind::Rest { .. })
    }

    pub fn is_sheet(&self) -> bool {
        matches!(
            self,
            FeatureKind::SheetBase { .. }
                | FeatureKind::SheetContour { .. }
                | FeatureKind::SheetFlange { .. }
                | FeatureKind::SheetHem { .. }
                | FeatureKind::SheetPerforate { .. }
                | FeatureKind::SheetFold { .. }
                | FeatureKind::SheetUnfold { .. }
                | FeatureKind::SheetConvert { .. }
        )
    }

    /// Fusion-like type name (timeline tooltip, `inspect`).
    pub fn type_name(&self) -> &'static str {
        match self {
            FeatureKind::BodyTransfer { .. } => "BodyTransferFeature",
            FeatureKind::Sketch { .. } => "Sketch",
            FeatureKind::Extrude { .. } => "ExtrudeFeature",
            FeatureKind::Revolve { .. } => "RevolveFeature",
            FeatureKind::Fillet { .. } => "FilletFeature",
            FeatureKind::Chamfer { .. } => "ChamferFeature",
            FeatureKind::Box { .. } => "BoxFeature",
            FeatureKind::Cylinder { .. } => "CylinderFeature",
            FeatureKind::Sphere { .. } => "SphereFeature",
            FeatureKind::Torus { .. } => "TorusFeature",
            FeatureKind::Combine { .. } => "CombineFeature",
            FeatureKind::Pattern { pattern: PatternKind::Rectangular { .. }, .. } => "RectangularPatternFeature",
            FeatureKind::Pattern { pattern: PatternKind::Path { .. }, .. } => "PathPatternFeature",
            FeatureKind::Pattern { .. } => "CircularPatternFeature",
            FeatureKind::Mirror { .. } => "MirrorFeature",
            FeatureKind::ConstructionPlane { .. } => "ConstructionPlane",
            FeatureKind::Hole { .. } => "HoleFeature",
            FeatureKind::Thread { .. } => "ThreadFeature",
            FeatureKind::Loft { .. } => "LoftFeature",
            FeatureKind::Sweep { .. } => "SweepFeature",
            FeatureKind::Shell { .. } => "ShellFeature",
            FeatureKind::Draft { .. } => "DraftFeature",
            FeatureKind::Split { .. } => "SplitBodyFeature",
            FeatureKind::SplitFace { .. } => "SplitFaceFeature",
            FeatureKind::ConstructionAxis { .. } => "ConstructionAxis",
            FeatureKind::ConstructionPoint { .. } => "ConstructionPoint",
            FeatureKind::Move { .. } => "MoveFeature",
            FeatureKind::Scale { .. } => "ScaleFeature",
            FeatureKind::OffsetFace { .. } => "OffsetFacesFeature",
            FeatureKind::DeleteFace { .. } => "DeleteFaceFeature",
            FeatureKind::SurfaceOffset { .. } => "OffsetSurfaceFeature",
            FeatureKind::BoundingSolid { .. } => "BoundingSolidFeature",
            FeatureKind::Pipe { .. } => "PipeFeature",
            FeatureKind::Emboss { .. } => "EmbossFeature",
            FeatureKind::Boss { .. } => "BossFeature",
            FeatureKind::Lip { groove: false, .. } => "LipFeature",
            FeatureKind::Lip { .. } => "GrooveFeature",
            FeatureKind::SnapFit { .. } => "SnapFitFeature",
            FeatureKind::Rest { .. } => "RestFeature",
            FeatureKind::BoundaryFill { .. } => "BoundaryFillFeature",
            FeatureKind::SheetBase { .. } => "BaseFlangeFeature",
            FeatureKind::SheetContour { .. } => "ContourFlangeFeature",
            FeatureKind::SheetFlange { .. } => "EdgeFlangeFeature",
            FeatureKind::SheetHem { .. } => "HemFeature",
            FeatureKind::SheetPerforate { .. } => "PerforateFeature",
            FeatureKind::SheetFold { .. } => "FoldFeature",
            FeatureKind::SheetUnfold { refold: false, .. } => "UnfoldFeature",
            FeatureKind::SheetUnfold { .. } => "RefoldFeature",
            FeatureKind::SheetConvert { .. } => "ConvertToSheetMetalFeature",
            FeatureKind::Coil { .. } => "CoilFeature",
            FeatureKind::Rib { web: false, .. } => "RibFeature",
            FeatureKind::Rib { .. } => "WebFeature",
            FeatureKind::ReplaceFace { .. } => "ReplaceFaceFeature",
            FeatureKind::Align { .. } => "AlignFeature",
            FeatureKind::Remove { .. } => "RemoveFeature",
            FeatureKind::Patch { .. } => "PatchFeature",
            FeatureKind::SurfaceExtrude { .. } => "SurfaceExtrudeFeature",
            FeatureKind::SurfaceRevolve { .. } => "SurfaceRevolveFeature",
            FeatureKind::SurfaceSweep { .. } => "SurfaceSweepFeature",
            FeatureKind::SurfaceLoft { .. } => "SurfaceLoftFeature",
            FeatureKind::Stitch { .. } => "StitchFeature",
            FeatureKind::Thicken { .. } => "ThickenFeature",
            FeatureKind::SurfaceTrim { .. } => "TrimFeature",
            FeatureKind::SurfaceExtend { .. } => "ExtendFeature",
            FeatureKind::SurfaceRuled { .. } => "RuledSurfaceFeature",
            FeatureKind::Unstitch { .. } => "UnstitchFeature",
            FeatureKind::ReverseNormal { .. } => "ReverseNormalFeature",
            FeatureKind::SurfaceUntrim { .. } => "UntrimFeature",
            FeatureKind::Import { .. } => "BaseFeature",
            FeatureKind::MeshImport { .. } => "MeshFeature",
            FeatureKind::MeshConvert { .. } => "MeshConvertFeature",
        }
    }
    /// Default name prefix (`Extrude` → `Extrude1`).
    pub fn base_name(&self) -> &'static str {
        match self {
            FeatureKind::BodyTransfer { .. } => "BodyTransfer",
            FeatureKind::Sketch { .. } => "Sketch",
            FeatureKind::Extrude { .. } => "Extrude",
            FeatureKind::Revolve { .. } => "Revolve",
            FeatureKind::Fillet { .. } => "Fillet",
            FeatureKind::Chamfer { .. } => "Chamfer",
            FeatureKind::Box { .. } => "Box",
            FeatureKind::Cylinder { .. } => "Cylinder",
            FeatureKind::Sphere { .. } => "Sphere",
            FeatureKind::Torus { .. } => "Torus",
            FeatureKind::Combine { .. } => "Combine",
            FeatureKind::Pattern { pattern: PatternKind::Rectangular { .. }, .. } => "RectangularPattern",
            FeatureKind::Pattern { pattern: PatternKind::Path { .. }, .. } => "PathPattern",
            FeatureKind::Pattern { .. } => "CircularPattern",
            FeatureKind::Mirror { .. } => "Mirror",
            FeatureKind::ConstructionPlane { .. } => "Plane",
            FeatureKind::Hole { .. } => "Hole",
            FeatureKind::Thread { .. } => "Thread",
            FeatureKind::Loft { .. } => "Loft",
            FeatureKind::Sweep { .. } => "Sweep",
            FeatureKind::Shell { .. } => "Shell",
            FeatureKind::Draft { .. } => "Draft",
            FeatureKind::Split { .. } => "Split",
            FeatureKind::SplitFace { .. } => "SplitFace",
            FeatureKind::ConstructionAxis { .. } => "Axis",
            FeatureKind::ConstructionPoint { .. } => "Point",
            FeatureKind::Move { .. } => "Move",
            FeatureKind::Scale { .. } => "Scale",
            FeatureKind::OffsetFace { .. } => "OffsetFace",
            FeatureKind::DeleteFace { .. } => "DeleteFace",
            FeatureKind::SurfaceOffset { .. } => "OffsetSurface",
            FeatureKind::BoundingSolid { .. } => "BoundingSolid",
            FeatureKind::Pipe { .. } => "Pipe",
            FeatureKind::Emboss { .. } => "Emboss",
            FeatureKind::Boss { .. } => "Boss",
            FeatureKind::Lip { groove: false, .. } => "Lip",
            FeatureKind::Lip { .. } => "Groove",
            FeatureKind::SnapFit { .. } => "SnapFit",
            FeatureKind::Rest { .. } => "Rest",
            FeatureKind::BoundaryFill { .. } => "BoundaryFill",
            FeatureKind::SheetBase { .. } => "BaseFlange",
            FeatureKind::SheetContour { .. } => "ContourFlange",
            FeatureKind::SheetFlange { .. } => "EdgeFlange",
            FeatureKind::SheetHem { .. } => "Hem",
            FeatureKind::SheetPerforate { .. } => "Perforate",
            FeatureKind::SheetFold { .. } => "Fold",
            FeatureKind::SheetUnfold { refold: false, .. } => "Unfold",
            FeatureKind::SheetUnfold { .. } => "Refold",
            FeatureKind::SheetConvert { .. } => "ConvertToSheetMetal",
            FeatureKind::Coil { .. } => "Coil",
            FeatureKind::Rib { web: false, .. } => "Rib",
            FeatureKind::Rib { .. } => "Web",
            FeatureKind::ReplaceFace { .. } => "ReplaceFace",
            FeatureKind::Align { .. } => "Align",
            FeatureKind::Remove { .. } => "Remove",
            FeatureKind::Patch { .. } => "Patch",
            FeatureKind::SurfaceExtrude { .. } => "SurfaceExtrude",
            FeatureKind::SurfaceRevolve { .. } => "SurfaceRevolve",
            FeatureKind::SurfaceSweep { .. } => "SurfaceSweep",
            FeatureKind::SurfaceLoft { .. } => "SurfaceLoft",
            FeatureKind::Stitch { .. } => "Stitch",
            FeatureKind::Thicken { .. } => "Thicken",
            FeatureKind::SurfaceTrim { .. } => "Trim",
            FeatureKind::SurfaceExtend { .. } => "Extend",
            FeatureKind::SurfaceRuled { .. } => "RuledSurface",
            FeatureKind::Unstitch { .. } => "Unstitch",
            FeatureKind::ReverseNormal { .. } => "ReverseNormal",
            FeatureKind::SurfaceUntrim { .. } => "Untrim",
            FeatureKind::Import { .. } => "Import",
            FeatureKind::MeshImport { .. } => "Mesh",
            FeatureKind::MeshConvert { .. } => "ConvertMesh",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub suppressed: bool,
    /// Names for the bodies this feature creates (in order); missing ones are numbered after the
    /// first (`Ribs (4)`), or get `BodyN` when there is none (or see `shell_piece_order`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_names: Vec<String>,
    /// Parameter names of the feature's inputs (see `FeatureKind::inputs`), e.g. `d3`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_names: Vec<String>,
    /// The component the feature (and what it makes) belongs to; 0 = the root.
    #[serde(default, skip_serializing_if = "is_root")]
    pub component: u64,
    /// For each picked edge (fillets, chamfers): where it sat in its body when picked, so it
    /// is found again when earlier edits move or resize the body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_refs: Vec<crate::EdgeRef>,
    /// Persistent names of the picked edges (resolved first; see `naming`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_names: Vec<String>,
    /// Persistent names of the faces at the feature's face points (`FeatureKind::face_points`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub face_names: Vec<String>,
    /// Bodies a join/cut/intersect may change (any component); empty = the feature's own
    /// component's bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<String>,
    /// The pieces this feature splits a body into take its body names in the kernel's shell order,
    /// as builds before format 14 named them, instead of after their faces (`lineage::order_lumps`).
    /// Set on the features of designs saved before then, whose stored body names mean that order.
    /// Such features also name the bodies past their `body_names` `BodyN`, as those builds did,
    /// where newer ones number them after the first name (`Ribs (4)`, #373).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shell_piece_order: bool,
    #[serde(flatten)]
    pub kind: FeatureKind,
}

fn inside_str() -> String {
    "inside".into()
}

fn all_profiles() -> ProfileSel {
    ProfileSel::All
}

fn yes() -> bool {
    true
}

fn is_yes(b: &bool) -> bool {
    *b
}

fn zero_expr() -> String {
    "0".into()
}

fn is_root(c: &u64) -> bool {
    *c == 0
}

/// A standard part (fastener, pin, bearing) inserted as a component.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StandardPart {
    pub family: String,
    pub size: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
}

/// A component whose contents come from another design file (Insert Design): where that file
/// is, and the Import feature in the component that holds its bodies (Update Links replaces
/// them; Break Link keeps them and forgets the file).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignLink {
    /// The design file: relative to this design's folder once this design is saved, else as
    /// given (`/` separates folders).
    pub path: String,
    /// The Import feature holding the linked design's bodies.
    pub feature: u64,
}

/// The document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Named, renderer-independent setups. Device/job state is never saved here.
    #[serde(default, skip_serializing_if = "crate::render_settings::RenderSettings::is_empty")]
    pub render: std::sync::Arc<crate::render_settings::RenderSettings>,
    #[serde(default = "default_format")]
    pub format: String,
    pub name: String,
    #[serde(default = "default_units")]
    pub units: String,
    /// Design properties (project, author…) and other tools' data, kept as they are (#1015).
    #[serde(default, skip_serializing_if = "crate::Metadata::is_empty")]
    pub metadata: crate::Metadata,
    pub params: Vec<Param>,
    pub features: Vec<Feature>,
    /// Timeline marker: features at or after this index are rolled back (not evaluated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<usize>,
    #[serde(default)]
    pub next_id: u64,
    /// Components below the root, as a tree (by parent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
    /// Placements of components in their parents (each non-root component has at least one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occurrences: Vec<crate::Occurrence>,
    /// Joints between occurrences, joint origins and motion links.
    #[serde(default, skip_serializing_if = "crate::joints::Assembly::is_empty")]
    pub assembly: crate::joints::Assembly,
    /// Sheet metal rules.
    #[serde(default, skip_serializing_if = "crate::sheet::SheetSettings::is_empty")]
    pub sheet: crate::sheet::SheetSettings,
    /// Plastic rules.
    #[serde(default, skip_serializing_if = "crate::plastic::PlasticSettings::is_empty")]
    pub plastic: crate::plastic::PlasticSettings,
    /// Components that are standard parts (by component id): what to rebuild them from.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub parts: std::collections::BTreeMap<u64, StandardPart>,
    /// Components linked to another design file (by component id).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub links: std::collections::BTreeMap<u64, DesignLink>,
    /// Configurations (rows of parameter values and suppressions).
    #[serde(default, skip_serializing_if = "crate::config::ConfigTable::is_empty")]
    pub configs: crate::config::ConfigTable,
    /// Favourite parameters (by name).
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub favorites: std::collections::BTreeSet<String>,
    /// Reference images on planes (Insert Canvas).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvases: Vec<crate::canvas::Canvas>,
    /// Comments of feature-input parameters (by name).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub param_comments: std::collections::BTreeMap<String, String>,
    /// Physical material per body (by body name); others use the default.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub materials: std::collections::BTreeMap<String, String>,
    /// Physical materials defined in this design (`material.create`), next to the built-in ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_materials: Vec<UserMaterial>,
    /// Appearance colour per body (by body name, sRGB); it overrides the material's look.
    #[serde(default, skip_serializing_if = "crate::appearance::Appearances::is_empty")]
    pub appearances: crate::appearance::Appearances,
    /// Bodies moved into another component than their feature's (by body name).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub body_components: std::collections::BTreeMap<String, u64>,
    /// Bodies moved into another component keep their place in the world: the transform from
    /// the new component's frame to the frame the body was made in (by body name).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub body_offsets: std::collections::BTreeMap<String, crate::Mat>,
    /// Browser groups (folders of bodies, sketches or construction planes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub browser_groups: Vec<BrowserGroup>,
    /// Named folders used to organise features in the timeline. They do not affect evaluation
    /// order; feature ids remain the source of truth when features are renamed or reordered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeline_groups: Vec<TimelineGroup>,
    /// Browser item order per component folder (`"<component>/<folder>"` → item keys).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub browser_order: std::collections::BTreeMap<String, Vec<String>>,
    /// Named views: saved cameras.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub named_views: Vec<NamedView>,
    /// What was hidden when the design was saved. A session takes it from here when the design
    /// opens and keeps the live state itself (`browser.visibility`: saved, not an undo step), so
    /// a document in a session leaves this empty; the session puts it back when it saves.
    #[serde(default, skip_serializing_if = "Visibility::is_empty")]
    pub visibility: Visibility,
}

/// Items hidden or shown one by one. Saved with the design, but not an undo step.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Visibility {
    /// Bodies by name.
    pub hidden_bodies: Vec<String>,
    /// Exact occurrence placements hidden with all descendants.
    pub hidden_occurrences: Vec<Vec<u64>>,
    /// Origin items (O, X, Y, Z, XY, XZ, YZ) and construction geometry, by name.
    pub hidden_origin: Vec<String>,
    /// Sketches by feature id.
    pub hidden_sketches: Vec<u64>,
    /// Finished sketches shown although a feature uses them.
    pub shown_sketches: Vec<u64>,
    /// Assembly joints by id: their glyphs are not drawn (they still constrain and move).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden_joints: Vec<u64>,
}

impl Visibility {
    /// Nothing hidden or shown one by one (the default view).
    pub fn is_empty(&self) -> bool {
        self.hidden_bodies.is_empty()
            && self.hidden_occurrences.is_empty()
            && self.hidden_origin.is_empty()
            && self.hidden_sketches.is_empty()
            && self.shown_sketches.is_empty()
            && self.hidden_joints.is_empty()
    }
}

/// A saved camera (orbit camera: target, yaw, pitch, distance; `fov` 0 for orthographic).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NamedView {
    #[serde(default)]
    pub name: String,
    pub target: solidcraft_geom::Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
    #[serde(default)]
    pub fov: f64,
}

/// A browser group: items of one folder (`bodies`, `sketches` or `construction`) of a
/// component, by key (body name, or the feature id of a sketch or construction plane).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BrowserGroup {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub component: u64,
    pub folder: String,
    #[serde(default)]
    pub items: Vec<String>,
}

/// A saved feature-history folder. Folders may nest, and a feature can belong to at most one folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TimelineGroup {
    pub id: u64,
    pub name: String,
    /// The containing feature folder. `None` places the folder at the timeline root.
    #[serde(default)]
    pub parent: Option<u64>,
    #[serde(default)]
    pub features: Vec<u64>,
}

/// The first feature-history folder whose own and nested features do not form one
/// uninterrupted run in the timeline.
pub fn non_contiguous_timeline_group(features: &[Feature], groups: &[TimelineGroup]) -> Option<u64> {
    let positions: std::collections::BTreeMap<u64, usize> = features.iter().enumerate().map(|(index, feature)| (feature.id, index)).collect();
    let mut stats = std::collections::BTreeMap::new();
    let mut children = std::collections::BTreeMap::new();
    for group in groups {
        let mut count = 0usize;
        let mut first = usize::MAX;
        let mut last = 0usize;
        for index in group.features.iter().filter_map(|feature| positions.get(feature)) {
            count = count.saturating_add(1);
            first = first.min(*index);
            last = last.max(*index);
        }
        stats.insert(group.id, (count, first, last));
        children.entry(group.id).or_insert(0usize);
        if let Some(parent) = group.parent {
            let child_count = children.entry(parent).or_insert(0);
            *child_count = child_count.saturating_add(1);
        }
    }
    let parents: std::collections::BTreeMap<u64, Option<u64>> = groups.iter().map(|group| (group.id, group.parent)).collect();
    let mut pending: Vec<u64> = groups.iter().filter(|group| children.get(&group.id) == Some(&0)).map(|group| group.id).collect();
    while let Some(id) = pending.pop() {
        let (count, first, last) = stats.get(&id).copied().unwrap_or_default();
        if count > 0 && last.saturating_sub(first).saturating_add(1) != count {
            return Some(id);
        }
        if let Some(parent) = parents.get(&id).copied().flatten() {
            if let Some((parent_count, parent_first, parent_last)) = stats.get_mut(&parent) {
                *parent_count = parent_count.saturating_add(count);
                if count > 0 {
                    *parent_first = (*parent_first).min(first);
                    *parent_last = (*parent_last).max(last);
                }
            }
            if let Some(remaining) = children.get_mut(&parent) {
                *remaining = remaining.saturating_sub(1);
                if *remaining == 0 {
                    pending.push(parent);
                }
            }
        }
    }
    None
}

fn default_format() -> String {
    crate::format::format_string(crate::format::FORMAT)
}
fn default_units() -> String {
    "mm".into()
}

impl Default for Document {
    fn default() -> Self {
        Document::new("Untitled")
    }
}

impl Document {
    pub fn new(name: &str) -> Self {
        Document {
            render: Default::default(),
            format: default_format(),
            name: name.into(),
            units: default_units(),
            metadata: Default::default(),
            params: Vec::new(),
            features: Vec::new(),
            marker: None,
            next_id: 1,
            components: Vec::new(),
            occurrences: Vec::new(),
            assembly: Default::default(),
            sheet: Default::default(),
            plastic: Default::default(),
            configs: Default::default(),
            parts: Default::default(),
            links: Default::default(),
            body_components: Default::default(),
            body_offsets: Default::default(),
            materials: Default::default(),
            user_materials: Vec::new(),
            appearances: Default::default(),
            favorites: Default::default(),
            canvases: Vec::new(),
            param_comments: Default::default(),
            browser_groups: Vec::new(),
            timeline_groups: Vec::new(),
            browser_order: Default::default(),
            named_views: Vec::new(),
            visibility: Visibility::default(),
        }
    }

    /// A complete structural proof: no non-render field is allowed to differ. Keeping this
    /// comparison based on the whole Document also includes fields added in future versions.
    /// The clone/comparison is only needed when render intent actually differs.
    pub fn differs_only_in_render(&self, previous: &Document) -> bool {
        if std::sync::Arc::ptr_eq(&self.render, &previous.render) || self.render == previous.render || self.render.validate().is_err() {
            return false;
        }
        let mut normalized = previous.clone();
        normalized.render = self.render.clone();
        *self == normalized
    }

    /// The colour a body is shown and exported in (sRGB as 0–1 fractions): its appearance,
    /// else its component's, else its material's look, else the colour it was imported with.
    /// (Faces may have their own: `face_looks`.)
    pub fn body_color(&self, b: &crate::ModelBody) -> Option<[f32; 3]> {
        self.model_body_look(b).map(|l| l.rgb()).or_else(|| b.body.color()).or_else(|| b.imported.as_ref().and_then(|i| i.color))
    }

    /// The model with each body carrying the colour, opacity and face colours it is shown with
    /// (for export).
    pub fn painted(&self, state: &crate::ModelState) -> crate::ModelState {
        let mut st = state.clone();
        for b in &mut st.bodies {
            let c = self.body_color(b);
            let faces: Vec<solidcraft_kernel::FacePaint> = self
                .face_colors(b)
                .into_iter()
                .map(|(face, l)| solidcraft_kernel::FacePaint { face, color: l.rgb(), opacity: l.opacity as f32 })
                .collect();
            let opacity = self.model_body_opacity(b) as f32;
            let paint = solidcraft_kernel::Paint { opacity, faces };
            if c != b.body.color() || b.body.paint() != Some(&paint) {
                b.body = b.body.clone().with_color(c).with_paint(Some(paint));
            }
        }
        st
    }

    /// Density of a body's material (g/cm³).
    pub fn density(&self, body: &str) -> f64 {
        self.materials.get(body).and_then(|m| self.material_density(m)).unwrap_or(DEFAULT_DENSITY)
    }

    /// A material defined in this design, by name (case-insensitive).
    pub fn user_material(&self, name: &str) -> Option<&UserMaterial> {
        let name = name.trim();
        self.user_materials.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// A physical material's name as the library or this design spells it (case-insensitive
    /// lookup): built-in materials first, then the design's own.
    pub fn material_name(&self, name: &str) -> Option<String> {
        MATERIALS
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))
            .map(|(n, _)| n.to_string())
            .or_else(|| self.user_material(name).map(|m| m.name.clone()))
    }

    /// Density of a physical material (g/cm³): built-in or defined in this design. A design
    /// material with an unusable density (a damaged file) has none.
    pub fn material_density(&self, name: &str) -> Option<f64> {
        material_density(name).or_else(|| self.user_material(name).map(|m| m.density).filter(|d| valid_density(*d)))
    }

    /// The look a physical material gives a body without an appearance of its own.
    pub fn material_look(&self, name: &str) -> Option<crate::appearance::Look> {
        crate::appearance::material_look(name).or_else(|| {
            let mut look = self.user_material(name)?.appearance.clone()?;
            // A damaged file's opacity is shown opaque rather than trusted.
            if !(look.opacity.is_finite() && (0.0..=1.0).contains(&look.opacity)) {
                look.opacity = 1.0;
            }
            Some(look)
        })
    }

    /// Every physical material's name: the built-in library, then the design's own.
    pub fn material_names(&self) -> Vec<String> {
        MATERIALS.iter().map(|(n, _)| n.to_string()).chain(self.user_materials.iter().map(|m| m.name.clone())).collect()
    }

    /// The component a body belongs to: where it was moved, else its feature's.
    pub fn body_component(&self, body: &str, feature: u64) -> u64 {
        self.features
            .iter()
            .take(self.marker.unwrap_or(usize::MAX))
            .rev()
            .find_map(|f| {
                if f.suppressed {
                    return None;
                }
                match &f.kind {
                    FeatureKind::BodyTransfer { bodies, to, .. } if bodies.iter().any(|b| b == body) => Some(*to),
                    _ => None,
                }
            })
            .unwrap_or_else(|| self.legacy_body_component(body, feature))
    }

    pub fn legacy_body_component(&self, body: &str, feature: u64) -> u64 {
        self.body_components.get(body).copied().unwrap_or_else(|| self.feature(feature).map(|f| f.component).unwrap_or(0))
    }

    /// Ownership in this evaluated timeline state, rather than a later transfer.
    pub fn model_body_component(&self, body: &crate::ModelBody) -> u64 {
        if body.transferred || body.source_name.is_some() { body.component } else { self.legacy_body_component(&body.name, body.feature) }
    }

    /// Recorded transfers already converted geometry into the new local frame.
    pub fn model_body_offset(&self, body: &crate::ModelBody) -> Option<&crate::Mat> {
        if body.transferred { None } else { self.body_offsets.get(&body.name) }
    }

    /// Add a component under `parent` and return its id.
    pub fn add_component(&mut self, name: Option<&str>, parent: u64) -> Result<u64> {
        if parent != 0 && !self.components.iter().any(|c| c.id == parent) {
            return Err(DocError::Unknown(format!("component {parent}")));
        }
        if self.components.len() >= 10_000 {
            return Err(DocError::Invalid("too many components".into()));
        }
        let id = self.next_id.max(1);
        self.next_id = id.checked_add(1).ok_or_else(|| DocError::Invalid("too many items in the design".into()))?;
        let name = match name.map(str::trim) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => {
                let mut k = self.components.len() + 1;
                while self.components.iter().any(|c| c.name == format!("Component{k}")) {
                    k += 1;
                }
                format!("Component{k}")
            }
        };
        self.components.push(Component { id, name, parent });
        self.add_occurrence(id, parent, crate::IDENTITY)?;
        Ok(id)
    }

    /// A component by id or name (0 / "root" is the design itself).
    pub fn find_component(&self, key: &str) -> Option<u64> {
        if key == "0" || key.eq_ignore_ascii_case("root") || key == self.name {
            return Some(0);
        }
        self.components.iter().find(|c| c.id.to_string() == key || c.name == key).map(|c| c.id)
    }

    /// Read a design file's JSON (any format this build knows; older ones are upgraded).
    pub fn from_json(s: &str) -> Result<Document> {
        let d = crate::format::read(s)?;
        if d.features.len() > MAX_FEATURES || d.params.len() > MAX_PARAMS {
            return Err(DocError::Invalid("document too large".into()));
        }
        Ok(d)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn feature(&self, id: u64) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }
    pub fn feature_mut(&mut self, id: u64) -> Option<&mut Feature> {
        self.features.iter_mut().find(|f| f.id == id)
    }
    pub fn feature_index(&self, id: u64) -> Option<usize> {
        self.features.iter().position(|f| f.id == id)
    }
    /// Find a feature by id (number) or name.
    pub fn find_feature(&self, r: &str) -> Option<&Feature> {
        if let Ok(id) = r.parse::<u64>()
            && let Some(f) = self.feature(id)
        {
            return Some(f);
        }
        self.features.iter().find(|f| f.name.eq_ignore_ascii_case(r))
    }

    fn unique_name(&self, base: &str) -> String {
        let mut n = 1;
        loop {
            let name = format!("{base}{n}");
            if !self.features.iter().any(|f| f.name == name) {
                return name;
            }
            n += 1;
        }
    }

    /// The deepest timeline folder spanning the insertion slot between `marker - 1` and
    /// `marker`. A slot at a folder's outer boundary is not inside that folder.
    fn timeline_group_at_insertion(&self, marker: usize) -> Option<u64> {
        let before = self.features.get(marker.checked_sub(1)?)?.id;
        let after = self.features.get(marker)?.id;
        let direct = |feature| self.timeline_groups.iter().find(|group| group.features.contains(&feature)).map(|group| group.id);
        let parents: std::collections::BTreeMap<u64, Option<u64>> = self.timeline_groups.iter().map(|group| (group.id, group.parent)).collect();

        let mut before_ancestors = std::collections::BTreeSet::new();
        let mut current = direct(before);
        let mut steps = 0usize;
        while let Some(id) = current {
            if !before_ancestors.insert(id) || steps >= self.timeline_groups.len() {
                return None;
            }
            current = parents.get(&id).copied().flatten();
            steps = steps.saturating_add(1);
        }

        current = direct(after);
        steps = 0;
        while let Some(id) = current {
            if before_ancestors.contains(&id) {
                return Some(id);
            }
            if steps >= self.timeline_groups.len() {
                return None;
            }
            current = parents.get(&id).copied().flatten();
            steps = steps.saturating_add(1);
        }
        None
    }

    /// Insert a feature at the marker (or the end) and return its id.
    pub fn add_feature(&mut self, kind: FeatureKind, name: Option<&str>) -> Result<u64> {
        if self.features.len() >= MAX_FEATURES {
            return Err(DocError::Invalid("too many features".into()));
        }
        let component = if let FeatureKind::BodyTransfer { bodies, from, to, transform } = &kind {
            crate::validate_body_transfer(self, bodies, *from, *to, transform)?;
            *to
        } else {
            0
        };
        let id = self.next_id.max(1);
        self.next_id = id.checked_add(1).ok_or_else(|| DocError::Invalid("too many items in the design".into()))?;
        let name = match name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => self.unique_name(kind.base_name()),
        };
        let f = Feature {
            id,
            name,
            suppressed: false,
            body_names: Vec::new(),
            param_names: Vec::new(),
            component,
            edge_refs: Vec::new(),
            edge_names: Vec::new(),
            face_names: Vec::new(),
            participants: Vec::new(),
            shell_piece_order: false,
            kind,
        };
        self.place_feature(f);
        Ok(id)
    }

    /// Insert a feature as given, keeping its id, name, parameter names and stored edge and
    /// face names, at the marker (or the end), with the model parameters its sketch dimensions
    /// use: a feature replayed from a script (`timeline.add`) is the feature the design had.
    /// An id of 0 or an empty name gets a new one. Refused, changing nothing, when the id, the
    /// name or a parameter name is already used, or the feature belongs to a component this
    /// design does not have.
    pub fn insert_feature(&mut self, mut f: Feature, params: Vec<Param>) -> Result<u64> {
        if self.features.len() >= MAX_FEATURES {
            return Err(DocError::Invalid("too many features".into()));
        }
        if f.id == 0 {
            f.id = self.next_id.max(1);
        } else if f.id > MAX_FEATURE_ID {
            return Err(DocError::Invalid(format!("feature id {} is out of range", f.id)));
        } else if self.feature(f.id).is_some() {
            return Err(DocError::Invalid(format!("feature id {} is already used", f.id)));
        }
        f.name = f.name.trim().to_string();
        if f.name.is_empty() {
            f.name = self.unique_name(f.kind.base_name());
        } else if self.features.iter().any(|x| x.name == f.name) {
            return Err(DocError::Invalid(format!("a feature named `{}` already exists", f.name)));
        }
        if f.component != 0 && !self.components.iter().any(|c| c.id == f.component) {
            return Err(DocError::Invalid(format!("feature `{}` belongs to an unknown component", f.name)));
        }
        if let FeatureKind::BodyTransfer { bodies, from, to, transform } = &f.kind {
            crate::validate_body_transfer(self, bodies, *from, *to, transform)?;
            if f.component != *to {
                return Err(DocError::Invalid("a body transfer belongs to its destination component".into()));
            }
        }
        let mut own = std::collections::BTreeSet::new();
        for n in f.param_names.iter().filter(|n| !n.is_empty()) {
            if let Some(why) = Self::param_name_problem(n) {
                return Err(DocError::Invalid(why));
            }
            if self.name_taken(n) || !own.insert(n) {
                return Err(DocError::Invalid(format!("parameter name `{n}` is already used")));
            }
        }
        if self.params.len().saturating_add(params.len()) > MAX_PARAMS {
            return Err(DocError::Invalid("too many parameters".into()));
        }
        for p in &params {
            if let Some(why) = Self::param_name_problem(&p.name) {
                return Err(DocError::Invalid(why));
            }
            if self.name_taken(&p.name) || !own.insert(&p.name) {
                return Err(DocError::Invalid(format!("parameter name `{}` is already used", p.name)));
            }
            if p.expr.len() > 4096 || p.comment.len() > 4096 {
                return Err(DocError::Expr(format!("parameter `{}`: expression or comment too long", p.name)));
            }
            if !p.unit.is_empty() && expr::unit_info(&p.unit).is_none() {
                return Err(DocError::Invalid(format!("parameter `{}`: unknown unit `{}`", p.name, p.unit)));
            }
        }
        let id = f.id;
        self.next_id = self.next_id.max(id.saturating_add(1));
        self.params.extend(params.into_iter().map(|p| Param { model: true, ..p }));
        self.place_feature(f);
        Ok(id)
    }

    /// Put a feature at the marker (or the end), in the timeline folder that spans that slot,
    /// and name its inputs.
    fn place_feature(&mut self, f: Feature) {
        let id = f.id;
        let insertion_group = self.marker.and_then(|marker| self.timeline_group_at_insertion(marker));
        match self.marker {
            Some(m) if m < self.features.len() => {
                self.features.insert(m, f);
                self.marker = Some(m + 1);
            }
            _ => self.features.push(f),
        }
        if let Some(group_id) = insertion_group {
            let positions: std::collections::BTreeMap<u64, usize> =
                self.features.iter().enumerate().map(|(index, feature)| (feature.id, index)).collect();
            if let Some(group) = self.timeline_groups.iter_mut().find(|group| group.id == group_id) {
                group.features.push(id);
                group.features.sort_by_key(|feature| positions.get(feature).copied().unwrap_or(usize::MAX));
            }
        }
        self.name_feature_inputs(id);
    }

    /// Delete a feature (and, for a sketch, the features that use it).
    pub fn delete_feature(&mut self, id: u64) -> Result<Vec<u64>> {
        let idx = self.feature_index(id).ok_or_else(|| DocError::Unknown(format!("feature {id}")))?;
        let mut gone = vec![id];
        gone.extend(self.features.iter().filter(|f| f.kind.input_sketches().contains(&id)).map(|f| f.id));
        let gone_params: Vec<String> = self.features.iter().filter(|f| gone.contains(&f.id)).flat_map(|f| f.param_names.clone()).collect();
        self.configs
            .rules
            .retain(|rule| !rule.references_features(&gone) && !rule.parameter_references().iter().any(|name| gone_params.contains(name)));
        self.features.retain(|f| !gone.contains(&f.id));
        for group in &mut self.timeline_groups {
            group.features.retain(|feature| !gone.contains(feature));
        }
        // Configuration columns of deleted features go with them.
        let keep: Vec<bool> = self
            .configs
            .columns
            .iter()
            .map(|c| match c {
                crate::config::Column::Suppress(id) => !gone.contains(id),
                crate::config::Column::Param(name) => !gone_params.contains(name),
                crate::config::Column::FaceAppearance(target) => !gone.contains(&target.origin_feature),
                _ => true,
            })
            .collect();
        self.appearances
            .faces
            .retain(|face| !face.strict_name || !face.name.as_ref().is_some_and(|name| gone.iter().any(|id| name.starts_with(&format!("F{id}:")))));
        if keep.contains(&false) {
            let mut k = keep.iter();
            self.configs.columns.retain(|_| k.next().copied().unwrap_or(true));
            for r in &mut self.configs.rows {
                let mut k = keep.iter();
                r.cells.retain(|_| k.next().copied().unwrap_or(true));
            }
        }
        if let Some(m) = self.marker
            && m > idx
        {
            self.marker = Some(m.saturating_sub(gone.len()).max(idx));
        }
        // Drop model parameters nobody references any more.
        self.prune_model_params();
        Ok(gone)
    }

    /// Remove model parameters not referenced by any dimension, feature or other parameter.
    pub fn prune_model_params(&mut self) {
        let used = self.referenced_names();
        self.params.retain(|p| !p.model || used.contains(&p.name));
    }

    fn referenced_names(&self) -> Vec<String> {
        let mut used: Vec<String> = Vec::new();
        for rule in &self.configs.rules {
            used.extend(rule.parameter_references());
        }
        for (index, column) in self.configs.columns.iter().enumerate() {
            if let crate::config::Column::Param(name) = column {
                used.push(name.clone());
                for row in &self.configs.rows {
                    if let Some(value) = row.cells.get(index) {
                        used.extend(expr::references(value));
                    }
                }
            }
        }
        for f in &self.features {
            for e in f.kind.expressions() {
                used.extend(expr::references(e));
            }
            if let FeatureKind::Sketch { sketch, .. } = &f.kind {
                used.extend(sketch.constraints.iter().filter_map(|c| c.param.clone()));
            }
        }
        for p in &self.params {
            used.extend(expr::references(&p.expr));
        }
        used
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Why a name cannot be a parameter name (None = fine).
    pub fn param_name_problem(name: &str) -> Option<String> {
        let mut cs = name.chars();
        if !matches!(cs.next(), Some(c) if c.is_alphabetic() || c == '_') || !cs.all(|c| c.is_alphanumeric() || c == '_') {
            return Some(format!("`{name}` is not a valid parameter name (letters, digits and _, starting with a letter)"));
        }
        if name.len() > 64 {
            return Some("parameter names are at most 64 characters".into());
        }
        if expr::is_reserved(name) {
            return Some(format!("`{name}` is a unit, constant or function name"));
        }
        None
    }

    /// Add or change a parameter. Fails if the result would not evaluate. A new parameter
    /// without a unit takes the unit of its value (mm, deg or none).
    pub fn set_param(&mut self, name: &str, expr_s: &str, unit: Option<&str>, comment: Option<&str>) -> Result<()> {
        if let Some(p) = Self::param_name_problem(name) {
            return Err(DocError::Invalid(p));
        }
        if expr_s.len() > 4096 {
            return Err(DocError::Expr("expression too long".into()));
        }
        if expr_s.trim().is_empty() {
            return Err(DocError::Expr("empty expression".into()));
        }
        if let Some(u) = unit
            && expr::unit_info(u).is_none()
        {
            return Err(DocError::Expr(format!("unknown unit `{u}` (mm, cm, m, in, ft, deg, rad or none)")));
        }
        if comment.is_some_and(|c| c.len() > 4096) {
            return Err(DocError::Invalid("comment too long".into()));
        }
        if self.features.iter().any(|f| f.param_names.iter().any(|n| n == name)) {
            return Err(DocError::Invalid(format!("`{name}` is a feature's parameter")));
        }
        let mut next = self.clone();
        let inferred = match unit {
            Some(u) => u.trim().to_string(),
            None => next.infer_unit(expr_s),
        };
        match next.params.iter_mut().find(|p| p.name == name) {
            Some(p) => {
                p.expr = expr_s.trim().to_string();
                if let Some(u) = unit {
                    p.unit = u.trim().to_string();
                }
                if let Some(c) = comment {
                    p.comment = c.to_string();
                }
            }
            None => {
                if next.params.len() >= MAX_PARAMS {
                    return Err(DocError::Invalid("too many parameters".into()));
                }
                next.params.push(Param {
                    name: name.into(),
                    expr: expr_s.trim().into(),
                    unit: inferred,
                    comment: comment.unwrap_or_default().into(),
                    model: false,
                })
            }
        }
        let (_, errors) = next.param_values();
        if let Some(e) = errors.get(name) {
            return Err(DocError::Expr(e.clone()));
        }
        *self = next;
        Ok(())
    }

    /// Create the next free model parameter `dN` with the given expression and unit.
    pub fn new_model_param(&mut self, expr_s: &str, unit: &str) -> String {
        let name = self.next_d_name();
        self.params.push(Param { name: name.clone(), expr: expr_s.into(), unit: unit.into(), comment: String::new(), model: true });
        name
    }

    /// Evaluate an expression against the document's parameters.
    pub fn eval(&self, e: &str, kind: Kind) -> Result<f64> {
        // The parameters are evaluated only when the expression names one: most expressions an
        // agent sends are plain values, and evaluating every parameter of a large design for
        // each of them made every new feature cost more than the last.
        let vals: std::cell::OnceCell<std::collections::BTreeMap<String, expr::Value>> = std::cell::OnceCell::new();
        expr::eval_with(e, &|n| {
            vals.get_or_init(|| self.param_values().0).get(n).copied().ok_or_else(|| DocError::Expr(format!("unknown parameter `{n}`")))
        })?
        .to_kind(kind)
    }

    pub fn eval_in(vals: &std::collections::BTreeMap<String, expr::Value>, e: &str, kind: Kind) -> Result<f64> {
        expr::eval_with(e, &|n| vals.get(n).copied().ok_or_else(|| DocError::Expr(format!("unknown parameter `{n}`"))))?.to_kind(kind)
    }

    /// Resolve a plane reference.
    pub fn resolve_plane(&self, vals: &std::collections::BTreeMap<String, expr::Value>, p: &PlaneRef, depth: usize) -> Result<Plane> {
        self.resolve_plane_in(vals, p, depth, None)
    }

    /// Resolve a plane; planes built from model geometry (edges, faces, construction axes and
    /// points) need the model state `st`, and construction planes are read from it when given.
    pub fn resolve_plane_in(
        &self,
        vals: &std::collections::BTreeMap<String, expr::Value>,
        p: &PlaneRef,
        depth: usize,
        st: Option<&crate::ModelState>,
    ) -> Result<Plane> {
        if depth > 32 {
            return Err(DocError::Invalid("plane offsets nested too deeply".into()));
        }
        match p {
            PlaneRef::Origin { name } => Plane::named(name).ok_or_else(|| DocError::Unknown(format!("plane `{name}`"))),
            PlaneRef::Custom { plane } | PlaneRef::Face { plane, .. } => {
                Plane::new(plane.origin, plane.x, plane.y).ok_or_else(|| DocError::Invalid("degenerate plane".into()))
            }
            PlaneRef::Offset { base, distance } => {
                let b = self.resolve_plane_in(vals, base, depth + 1, st)?;
                Ok(b.offset(Self::eval_in(vals, distance, Kind::Length)?))
            }
            PlaneRef::AtAngle { base, axis_origin, axis_dir, angle } => {
                let b = self.resolve_plane_in(vals, base, depth + 1, st)?;
                let a = Self::eval_in(vals, angle, Kind::Angle)?;
                let d = axis_dir.normalized().ok_or_else(|| DocError::Invalid("rotation axis".into()))?;
                let rot = |v: Vec3| v * a.cos() + d.cross(v) * a.sin() + d * (d.dot(v) * (1.0 - a.cos()));
                let o = *axis_origin + rot(b.origin - *axis_origin);
                Plane::new(o, rot(b.x), rot(b.y)).ok_or_else(|| DocError::Invalid("degenerate plane".into()))
            }
            PlaneRef::Construction { name } => {
                // Evaluated already: as it came out.
                if let Some(c) = st.and_then(|st| st.construct.iter().find(|c| &c.name == name))
                    && let crate::construct::ConstructGeom::Plane(pl) = &c.geom
                {
                    return Ok(*pl);
                }
                match self.find_feature(name).map(|f| &f.kind) {
                    Some(FeatureKind::ConstructionPlane { plane }) => self.resolve_plane_in(vals, plane, depth + 1, st),
                    _ => Err(DocError::Unknown(format!("construction plane `{name}`"))),
                }
            }
            PlaneRef::Midplane { .. }
            | PlaneRef::TwoEdges { .. }
            | PlaneRef::ThreePoints { .. }
            | PlaneRef::Tangent { .. }
            | PlaneRef::AlongPath { .. }
            | PlaneRef::Perpendicular { .. } => {
                let st = st.ok_or_else(|| DocError::Invalid("this plane is built from the model: evaluate first".into()))?;
                crate::construct::resolve_geo_plane(&crate::construct::Ctx { doc: self, vals, st, depth }, p)
            }
        }
    }

    /// Mutable access to a sketch feature's sketch.
    pub fn sketch_mut(&mut self, id: u64) -> Result<&mut Sketch> {
        match self.feature_mut(id).map(|f| &mut f.kind) {
            Some(FeatureKind::Sketch { sketch, .. }) => Ok(sketch),
            Some(_) => Err(DocError::Invalid(format!("feature {id} is not a sketch"))),
            None => Err(DocError::Unknown(format!("sketch {id}"))),
        }
    }
    pub fn sketch(&self, id: u64) -> Result<&Sketch> {
        match self.feature(id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch { sketch, .. }) => Ok(sketch),
            Some(_) => Err(DocError::Invalid(format!("feature {id} is not a sketch"))),
            None => Err(DocError::Unknown(format!("sketch {id}"))),
        }
    }

    /// Apply parameter values to every dimension in a sketch (in place).
    pub fn apply_dimension_values(&self, vals: &std::collections::BTreeMap<String, expr::Value>, sk: &mut Sketch) -> Result<()> {
        for c in &mut sk.constraints {
            if let Some(p) = &c.param {
                let kind = if c.kind.is_angle() { Kind::Angle } else { Kind::Length };
                let v = vals.get(p).copied().ok_or_else(|| DocError::Unknown(format!("parameter `{p}`")))?.to_kind(kind)?;
                c.kind.set_value(v);
            }
        }
        for c in &sk.constraints {
            sk.validate(&c.kind)?;
        }
        Ok(())
    }

    /// Apply parameter values to a sketch's dimensions and solve it. `stored` are the values
    /// its geometry is solved for (the stored sketch's): what the solve cannot reach in one
    /// jump it reaches by stepping the changed values from there, and an edit that would turn
    /// the sketch over is refused. Offsets whose sources moved are refreshed and solved again:
    /// from the stored state again after a refused or failed first solve (the geometry is still
    /// the stored one), from the solved one otherwise.
    ///
    /// `moved_from`: the stored sketch, when `sk` is that sketch with its links refreshed. A
    /// link's points are where their source is now; the sides and turns the solve keeps are
    /// those of the stored state ([`Sketch::positions_in`]), not the ones read beside a line
    /// that has moved past a point.
    pub fn solve_sketch(
        &self,
        vals: &std::collections::BTreeMap<String, expr::Value>,
        sk: &mut Sketch,
        stored: &solidcraft_sketch::DrivingValues,
        moved_from: Option<&Sketch>,
    ) -> Result<solidcraft_sketch::SolveReport> {
        self.apply_dimension_values(vals, sk)?;
        let was = |sk: &Sketch| moved_from.and_then(|from| sk.positions_in(from));
        let mut report = solidcraft_sketch::solve_after_edit(sk, stored, was(sk).as_deref());
        if !sk.offsets.is_empty() || !sk.freeform_offsets.is_empty() {
            // Source dimensions may have moved curves during the solve.
            sk.refresh_offsets()?;
            report = if report.ok() { solidcraft_sketch::solve(sk) } else { solidcraft_sketch::solve_after_edit(sk, stored, was(sk).as_deref()) };
        }
        Ok(report)
    }
}

/// Density of the default material (g/cm³), as Fusion's default.
#[path = "../../../assets/materials/library.rs"]
mod materials;
pub use materials::{DEFAULT_DENSITY, MATERIALS, material_color};

/// A physical material defined in a design (`material.create`): its name, its density and,
/// optionally, the look it gives bodies without an appearance of their own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserMaterial {
    pub name: String,
    /// Density (g/cm³).
    pub density: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<crate::appearance::Look>,
}

/// Largest density a material may have (g/cm³); the densest elements are about 22.6.
pub const MAX_DENSITY: f64 = 1000.0;

/// A density a material can have: finite, above zero and at most [`MAX_DENSITY`] g/cm³.
pub fn valid_density(d: f64) -> bool {
    d.is_finite() && d > 0.0 && d <= MAX_DENSITY
}

/// Density of a built-in physical material (g/cm³).
pub fn material_density(name: &str) -> Option<f64> {
    MATERIALS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim())).map(|(_, d)| *d)
}
