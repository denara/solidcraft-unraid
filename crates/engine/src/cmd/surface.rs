//! Surface commands: Patch, Stitch, Thicken and Trim (surface bodies and turning them into
//! solids), and the planned surface toolset (`planned!`: registered, not built yet).

use serde_json::Value;
use solidcraft_doc::{FaceAt, FeatureKind, PlaneRef, ProfileSel, expr::Kind};

use super::CommandSpec;
use super::features::{add_feature, axis_param, check_expr, extent_param, feature_sketch, operation, plane_param, profiles, sketch_id};
use crate::params::{bad, bool_, expr, str_, string_list, vec3};
use crate::{Result, Session};

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("surface.patch", "Patch", patch)
        .at("SURFACE", "CREATE")
        .icon("patch")
        .params("sketch?: id|name and profiles? (planar patches of sketch regions) | edges: [[x,y,z]] (points on a closed loop of a body's edges: one face on them, planar when the loop is, else a smooth B-spline surface) and body?; name?, body_name?"),
    CommandSpec::new("surface.stitch", "Stitch", stitch)
        .at("SURFACE", "MODIFY")
        .icon("stitch")
        .params("bodies: [names] (surfaces sewn into the first; a solid when they close); tolerance?: expr (default: tight)"),
    CommandSpec::new("surface.thicken", "Thicken", thicken)
        .at("SURFACE", "CREATE")
        .icon("thicken")
        .params("bodies: [surface names]; thickness: expr (negative: against the normal); symmetric?: bool; operation?: new|join|cut|intersect; targets?"),
    CommandSpec::new("surface.extend", "Extend", extend)
        .at("SURFACE", "MODIFY")
        .icon("extend")
        .params("body: planar surface name; edges: [[x,y,z]] (points on straight edges to push out); distance: expr"),
    CommandSpec::new("surface.trim", "Trim", trim)
        .at("SURFACE", "MODIFY")
        .icon("trim")
        .params("body: surface name; plane: XY|XZ|YZ|plane name|{origin, normal} or tool: {body, point} (a face, its surface extended); keep: [x,y,z] (a point on the side to keep)"),
    CommandSpec::new("surface.extrude", "Surface Extrude", surface_extrude)
        .at("SURFACE", "CREATE")
        .icon("surface_extrude")
        .params("sketch?: id|name; curves?: [curve ids] (open or closed chains of curves meeting end to end) and/or profiles? (their boundaries; the default when no curves are given); distance: expr; direction?: positive|negative|symmetric; distance2?: expr; start_offset?: expr; name?, body_name? (a surface body: walls only, no caps)"),
    CommandSpec::new("surface.revolve", "Surface Revolve", surface_revolve)
        .at("SURFACE", "CREATE")
        .icon("surface_revolve")
        .params("sketch?: id|name; curves?: [curve ids] and/or profiles? (as surface.extrude); axis: sketch line id|x|y|X|Y|Z|{origin, dir}; angle?: expr (default 360 deg); angle2?: expr (the other side); symmetric?: bool; name?, body_name? (a surface body, no caps; a solid where a full turn closes it)"),
    CommandSpec::new("surface.sweep", "Surface Sweep", surface_sweep)
        .at("SURFACE", "CREATE")
        .icon("surface_sweep")
        .params("sketch?: id|name; curves?: [curve ids] and/or profiles? (as surface.extrude); path_sketch: sketch; path: [curve ids in order] (a path with corners, or 3D curves, gives ruled walls mitred at the corners); name?, body_name? (a surface body, no caps)"),
    CommandSpec::new("surface.loft", "Surface Loft", surface_loft)
        .at("SURFACE", "CREATE")
        .icon("surface_loft")
        .params("sections: [{sketch, curves?: [curve ids], profiles?}] (2…50 in order; each one chain: a profile without holes, or curves meeting end to end; all open or all closed); name?, body_name? (a surface body: ruled between two sections, smooth through three or more; no caps)"),
    CommandSpec::new("surface.offset", "Offset Surface", offset)
        .at("SURFACE", "CREATE")
        .icon("surface_offset")
        .params("faces: [[x,y,z]] (points on the faces); body?; distance: expr (signed, along the faces' normals; 0 copies the faces); chain?: bool (each pick's whole smooth chain of faces); tolerance?: expr (how far a fitted face may stray, default 0.01 mm). Planes and cylinders move exactly; other faces are fitted. A new surface body (a solid where its faces close)"),
    planned!("surface.ruled", "Ruled Surface", "762")
        .at("SURFACE", "CREATE")
        .icon("ruled")
        .params("planned: body; edges: [[x,y,z]] (points on an edge chain); distance: expr; mode?: normal|tangent|angle|direction; angle?: expr; direction?: [x,y,z]; flip?: bool; corners?: mitre|round"),
    planned!("surface.boundary", "Boundary Surface", "763")
        .at("SURFACE", "CREATE")
        .icon("boundary")
        .params("planned: u: [curves], v?: [curves] (sketch curves, 3D sketch wires or {body, point} edges; 2–4 boundaries, more make a network); continuity?: [{curve, g: 0|1|2}] for edge boundaries; operation?: new|join"),
    planned!("surface.blend", "Blend Surface", "764")
        .at("SURFACE", "CREATE")
        .icon("surface_blend")
        .params("planned: from, to: {body, edges: [[x,y,z]]} (two edge chains apart); continuity?: [g0|g1|g2, g0|g1|g2] (each side); bulge?: [number, number]; align?: auto|[[x,y,z], [x,y,z]]; stitch?: bool"),
    planned!("surface.untrim", "Untrim", "761")
        .at("SURFACE", "MODIFY")
        .icon("untrim")
        .params("planned: face: {body, point}; holes?: all|[[x,y,z]] (points on the inner loops to remove); outer?: bool (restore the natural bounds); in_place?: bool (a surface body only)"),
    CommandSpec::new("surface.unstitch", "Unstitch", unstitch)
        .at("SURFACE", "MODIFY")
        .icon("unstitch")
        .params("body; faces?: [[x,y,z]] (points on the faces to take out, each its own surface body; the rest stay together under the body's name; default: every face its own surface body). A solid comes apart into surfaces"),
    CommandSpec::new("surface.reverse_normal", "Reverse Normal", reverse_normal)
        .at("SURFACE", "MODIFY")
        .icon("reverse_normal")
        .params("body (a surface); faces?: [[x,y,z]] (points on the faces to turn over; default: all of them)"),
    planned!("surface.fillet", "Surface Fillet", "769")
        .at("SURFACE", "MODIFY")
        .icon("surface_fillet")
        .params("planned: a, b: {body, point} (two faces of separate bodies that meet or cross; the points pick the side the ball rolls on); radius: expr; trim?: both|first|none"),
];

fn patch(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.patch";
    let edges: Vec<solidcraft_geom::Vec3> = p.get("edges").and_then(Value::as_array).map(|a| a.iter().filter_map(vec3).collect()).unwrap_or_default();
    if !edges.is_empty() {
        if edges.len() > 10_000 {
            return Err(bad(cmd, "too many edges"));
        }
        let body = str_(p, "body").map(str::to_string);
        return add_feature(s, p, FeatureKind::Patch { sketch: None, profiles: ProfileSel::All, body, edges });
    }
    let sketch = feature_sketch(s, p, cmd)?;
    let profiles = profiles(p, cmd)?;
    add_feature(s, p, FeatureKind::Patch { sketch: Some(sketch), profiles, body: None, edges: Vec::new() })
}

fn surface_extrude(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.extrude";
    let sketch = feature_sketch(s, p, cmd)?;
    let (profiles, curves) = sheet_inputs(p, cmd)?;
    let extent = extent_param(s, p, cmd)?;
    if extent.through_all || extent.taper.is_some() || extent.to.is_some() || extent.from.is_some() {
        return Err(bad(cmd, "not supported yet: through all, to or from an object, or a taper"));
    }
    add_feature(s, p, FeatureKind::SurfaceExtrude { sketch, profiles, curves, extent })
}

/// A body by name, and points on some of its faces (none: all of them).
fn body_faces(s: &Session, p: &Value, cmd: &str) -> Result<(String, Vec<solidcraft_geom::Vec3>)> {
    let body = str_(p, "body").ok_or_else(|| bad(cmd, "`body` is required"))?.to_string();
    if s.model.state().body(&body).is_none() {
        return Err(bad(cmd, format!("no body `{body}`")));
    }
    let faces: Vec<solidcraft_geom::Vec3> = p.get("faces").and_then(Value::as_array).map(|a| a.iter().filter_map(vec3).collect()).unwrap_or_default();
    if faces.len() > 10_000 {
        return Err(bad(cmd, "too many faces"));
    }
    Ok((body, faces))
}

fn unstitch(s: &mut Session, p: &Value) -> Result<Value> {
    let (body, faces) = body_faces(s, p, "surface.unstitch")?;
    add_feature(s, p, FeatureKind::Unstitch { body, faces })
}

fn reverse_normal(s: &mut Session, p: &Value) -> Result<Value> {
    let (body, faces) = body_faces(s, p, "surface.reverse_normal")?;
    add_feature(s, p, FeatureKind::ReverseNormal { body, faces })
}

/// The profiles and curves a surface builder takes: `curves`, and `profiles` (all of them when
/// neither is given).
fn sheet_inputs(p: &Value, cmd: &str) -> Result<(Option<solidcraft_doc::ProfileSel>, Vec<String>)> {
    let curves = string_list(p, "curves");
    if curves.len() > 1000 {
        return Err(bad(cmd, "at most 1000 curves"));
    }
    let profiles = if curves.is_empty() || p.get("profiles").is_some() { Some(profiles(p, cmd)?) } else { None };
    Ok((profiles, curves))
}

fn surface_revolve(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.revolve";
    let sketch = feature_sketch(s, p, cmd)?;
    let (profiles, curves) = sheet_inputs(p, cmd)?;
    let axis = axis_param(p, cmd)?;
    let angle = expr(p, "angle").unwrap_or_else(|| "360 deg".into());
    check_expr(s, &angle, Kind::Angle, cmd, "angle")?;
    let angle2 = expr(p, "angle2");
    if let Some(a) = &angle2 {
        check_expr(s, a, Kind::Angle, cmd, "angle2")?;
    }
    let symmetric = bool_(p, "symmetric").unwrap_or(false) || str_(p, "direction").is_some_and(|d| d.eq_ignore_ascii_case("symmetric"));
    add_feature(s, p, FeatureKind::SurfaceRevolve { sketch, profiles, curves, axis, angle, angle2, symmetric })
}

fn surface_sweep(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.sweep";
    let sketch = feature_sketch(s, p, cmd)?;
    let (profiles, curves) = sheet_inputs(p, cmd)?;
    let path_sketch = sketch_id(s, p.get("path_sketch"), cmd, "path_sketch")?;
    let path = string_list(p, "path");
    if path.is_empty() || path.len() > 1000 {
        return Err(bad(cmd, "`path` must list 1…1000 curve ids"));
    }
    add_feature(s, p, FeatureKind::SurfaceSweep { sketch, profiles, curves, path_sketch, path })
}

fn surface_loft(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.loft";
    let given = p.get("sections").and_then(Value::as_array).ok_or_else(|| bad(cmd, "`sections` must list the sections in order"))?;
    if !(2..=50).contains(&given.len()) {
        return Err(bad(cmd, "a loft needs 2…50 sections"));
    }
    let mut sections = Vec::with_capacity(given.len());
    for sec in given {
        let sketch = sketch_id(s, sec.get("sketch"), cmd, "sections.sketch")?;
        let curves = string_list(sec, "curves");
        if curves.len() > 1000 {
            return Err(bad(cmd, "at most 1000 curves a section"));
        }
        let profiles = if sec.get("profiles").is_some() { Some(profiles(sec, cmd)?) } else { None };
        sections.push(solidcraft_doc::SheetSection { sketch, profiles, curves });
    }
    add_feature(s, p, FeatureKind::SurfaceLoft { sections })
}

fn offset(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.offset";
    let faces: Vec<solidcraft_geom::Vec3> = p.get("faces").and_then(Value::as_array).map(|a| a.iter().filter_map(vec3).collect()).unwrap_or_default();
    if faces.is_empty() || faces.len() > 10_000 {
        return Err(bad(cmd, "`faces` must list points on the faces to offset"));
    }
    let distance = expr(p, "distance").ok_or_else(|| bad(cmd, "`distance` is required"))?;
    check_expr(s, &distance, Kind::Length, cmd, "distance")?;
    let tolerance = expr(p, "tolerance");
    if let Some(t) = &tolerance {
        check_expr(s, t, Kind::Length, cmd, "tolerance")?;
    }
    let kind = FeatureKind::SurfaceOffset {
        faces,
        body: str_(p, "body").map(str::to_string),
        distance,
        chain: bool_(p, "chain").unwrap_or(false),
        tolerance,
    };
    add_feature(s, p, kind)
}

fn bodies_param(s: &Session, p: &Value, cmd: &str) -> Result<Vec<String>> {
    let bodies = string_list(p, "bodies");
    if bodies.is_empty() {
        return Err(bad(cmd, "`bodies` must list bodies"));
    }
    let st = s.model.state();
    if let Some(b) = bodies.iter().find(|b| st.body(b).is_none()) {
        return Err(bad(cmd, format!("no body `{b}`")));
    }
    Ok(bodies)
}

fn stitch(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.stitch";
    let bodies = bodies_param(s, p, cmd)?;
    let tolerance = expr(p, "tolerance").unwrap_or_else(|| "0".into());
    check_expr(s, &tolerance, Kind::Length, cmd, "tolerance")?;
    add_feature(s, p, FeatureKind::Stitch { bodies, tolerance })
}

fn thicken(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.thicken";
    let bodies = bodies_param(s, p, cmd)?;
    let thickness = expr(p, "thickness").ok_or_else(|| bad(cmd, "`thickness` is required"))?;
    check_expr(s, &thickness, Kind::Length, cmd, "thickness")?;
    let kind = FeatureKind::Thicken {
        bodies,
        thickness,
        symmetric: bool_(p, "symmetric").unwrap_or(false),
        operation: operation(p, cmd)?,
        targets: string_list(p, "targets"),
    };
    add_feature(s, p, kind)
}

fn extend(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.extend";
    let body = str_(p, "body").ok_or_else(|| bad(cmd, "`body` is required"))?.to_string();
    if s.model.state().body(&body).is_none() {
        return Err(bad(cmd, format!("no body `{body}`")));
    }
    let edges: Vec<solidcraft_geom::Vec3> = p.get("edges").and_then(Value::as_array).map(|a| a.iter().filter_map(vec3).collect()).unwrap_or_default();
    if edges.is_empty() || edges.len() > 10_000 {
        return Err(bad(cmd, "`edges` must list points on the edges to extend"));
    }
    let distance = expr(p, "distance").ok_or_else(|| bad(cmd, "`distance` is required"))?;
    check_expr(s, &distance, Kind::Length, cmd, "distance")?;
    add_feature(s, p, FeatureKind::SurfaceExtend { body, edges, distance })
}

fn trim(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "surface.trim";
    let body = str_(p, "body").ok_or_else(|| bad(cmd, "`body` is required"))?.to_string();
    if s.model.state().body(&body).is_none() {
        return Err(bad(cmd, format!("no body `{body}`")));
    }
    let keep = p.get("keep").and_then(vec3).ok_or_else(|| bad(cmd, "`keep` must be [x, y, z] on the side to keep"))?;
    let (plane, tool) = match p.get("tool").filter(|t| !t.is_null()) {
        Some(t) => {
            let tb = t.get("body").and_then(Value::as_str).ok_or_else(|| bad(cmd, "`tool.body` must name a body"))?.to_string();
            let point = t.get("point").and_then(vec3).ok_or_else(|| bad(cmd, "`tool.point` must be [x, y, z] on the tool face"))?;
            (PlaneRef::Origin { name: "XY".into() }, Some(FaceAt { body: tb, point }))
        }
        None => (plane_param(s, p.get("plane"), cmd)?, None),
    };
    add_feature(s, p, FeatureKind::SurfaceTrim { body, plane, tool, keep })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;

    fn run(s: &mut Session, c: &str, p: Value) -> Value {
        s.execute(c, &p).unwrap_or_else(|e| panic!("{c}: {e}"))
    }
    fn volume(s: &Session, name: &str) -> f64 {
        let st = s.world_state();
        solidcraft_kernel::measure(&st.body(name).unwrap().body).unwrap().volume
    }

    fn area(s: &Session, name: &str) -> f64 {
        solidcraft_kernel::measure(&s.world_state().body(name).unwrap().body).unwrap().area
    }

    /// Surface Extrude of open curves: a line and an arc joined end to end (listed in either
    /// order) make one sheet of two walls; both sides of the sketch; undo, redo and reopening
    /// give the same sheet.
    #[test]
    fn surface_extrude_open_curves() {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]], "ids": ["l1"]}));
        let arc = run(&mut s, "sketch.arc.three_point", json!({"start": [10, 0], "end": [20, 0], "through": [15, 5]}))["created"]["curves"][0]
            .as_str()
            .unwrap()
            .to_string();
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.extrude", json!({"sketch": "S", "curves": [arc, "l1"], "distance": 4, "body_name": "Wall"}));
        let st = s.world_state();
        let wall = &st.body("Wall").unwrap().body;
        assert!(wall.is_surface() && wall.face_count() == 2);
        // The line's wall and the half circle's (radius 5): length × height.
        let want = (10.0 + std::f64::consts::PI * 5.0) * 4.0;
        assert!((area(&s, "Wall") - want).abs() < want * 1e-3, "{} vs {want}", area(&s, "Wall"));
        run(&mut s, "surface.extrude", json!({"sketch": "S", "curves": ["l1"], "distance": 2, "direction": "symmetric", "body_name": "Both"}));
        let bb = solidcraft_kernel::measure(&s.world_state().body("Both").unwrap().body).unwrap().bbox;
        assert!((bb.min.z + 2.0).abs() < 1e-9 && (bb.max.z - 2.0).abs() < 1e-9, "{bb:?}");
        run(&mut s, "edit.undo", json!({}));
        assert!(s.world_state().body("Both").is_none());
        run(&mut s, "edit.redo", json!({}));
        assert!(s.world_state().body("Both").is_some());
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((area(&cold, "Wall") - area(&s, "Wall")).abs() < 1e-9);
        // Refused: no such curve, a zero distance, and an extent a sheet does not take yet.
        assert!(s.execute("surface.extrude", &json!({"sketch": "S", "curves": ["nope"], "distance": 4})).is_err());
        assert!(s.execute("surface.extrude", &json!({"sketch": "S", "curves": ["l1"], "distance": 0})).is_err());
        assert!(s.execute("surface.extrude", &json!({"sketch": "S", "curves": ["l1"], "distance": 4, "through_all": true})).is_err());
    }

    /// Surface Extrude of a profile gives its walls alone; patching both ends and stitching
    /// makes the solid that solid.extrude makes of the same profile.
    #[test]
    fn surface_extrude_profile_walls_stitch_into_the_solid() {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "R"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [10, 6]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.extrude", json!({"sketch": "R", "distance": 3, "body_name": "Walls"}));
        let st = s.world_state();
        let walls = &st.body("Walls").unwrap().body;
        assert!(walls.is_surface() && walls.face_count() == 4);
        assert!((area(&s, "Walls") - 96.0).abs() < 1e-9);
        let edges = |z: f64| json!([[5, 0, z], [10, 3, z], [5, 6, z], [0, 3, z]]);
        run(&mut s, "surface.patch", json!({"edges": edges(0.0), "body": "Walls", "body_name": "Bottom"}));
        run(&mut s, "surface.patch", json!({"edges": edges(3.0), "body": "Walls", "body_name": "Top"}));
        run(&mut s, "surface.stitch", json!({"bodies": ["Walls", "Bottom", "Top"]}));
        let st = s.world_state();
        assert!(!st.body("Walls").unwrap().body.is_surface());
        run(&mut s, "solid.extrude", json!({"sketch": "R", "distance": 3, "body_name": "Solid"}));
        assert!((volume(&s, "Walls") - volume(&s, "Solid")).abs() < 1e-9);
        assert!((volume(&s, "Walls") - 180.0).abs() < 1e-9);
    }

    /// Unstitch a box: six surface bodies whose faces keep the names they had on the box,
    /// stitched back into a solid of the same volume; two faces taken out leave the other four
    /// under the box's name; undo, redo and reopening.
    #[test]
    fn unstitch_a_box_and_stitch_it_back() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Box"}));
        let names_before: Vec<String> = solidcraft_doc::naming::face_names(s.model.state().body("Box").unwrap()).to_vec();
        run(&mut s, "surface.unstitch", json!({"body": "Box"}));
        let st = s.world_state();
        assert!(st.body("Box").is_none());
        let pieces: Vec<String> = st.bodies.iter().map(|b| b.name.clone()).collect();
        assert_eq!(pieces.len(), 6);
        let mut names_after: Vec<String> = Vec::new();
        for n in &pieces {
            let b = st.body(n).unwrap();
            assert!(b.body.is_surface() && b.body.face_count() == 1);
            names_after.extend(solidcraft_doc::naming::face_names(s.model.state().body(n).unwrap()).iter().cloned());
        }
        let (mut a, mut b) = (names_before.clone(), names_after);
        a.sort();
        b.sort();
        assert_eq!(a, b, "the faces keep their names");
        run(&mut s, "surface.stitch", json!({"bodies": pieces}));
        let st = s.world_state();
        let back = st.bodies.first().unwrap();
        assert!(!back.body.is_surface());
        assert!((solidcraft_kernel::measure(&back.body).unwrap().volume - 1000.0).abs() < 1e-9);
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.undo", json!({}));
        assert!(s.world_state().body("Box").is_some());
        run(&mut s, "surface.unstitch", json!({"body": "Box", "faces": [[5, 5, 10], [5, 5, 0]]}));
        let st = s.world_state();
        assert_eq!(st.bodies.len(), 3);
        assert_eq!(st.body("Box").unwrap().body.face_count(), 4);
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert_eq!(cold.world_state().bodies.len(), 3);
        assert!(s.execute("surface.unstitch", &json!({"body": "Nope"})).is_err());
        assert!(s.execute("surface.unstitch", &json!({})).is_err());
    }

    /// A patch turned over thickens the other way; a solid is not turned over.
    #[test]
    fn reverse_normal_turns_the_thicken_side() {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [10, 10]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.patch", json!({"sketch": "S", "body_name": "Sheet"}));
        run(&mut s, "surface.thicken", json!({"bodies": ["Sheet"], "thickness": 2, "body_name": "Up"}));
        let up = solidcraft_kernel::measure(&s.world_state().body("Up").unwrap().body).unwrap().bbox;
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "surface.reverse_normal", json!({"body": "Sheet"}));
        run(&mut s, "surface.thicken", json!({"bodies": ["Sheet"], "thickness": 2, "body_name": "Down"}));
        let down = solidcraft_kernel::measure(&s.world_state().body("Down").unwrap().body).unwrap().bbox;
        assert!((up.max.z - down.max.z - 2.0).abs() < 1e-9 && (up.min.z - down.min.z - 2.0).abs() < 1e-9, "{up:?} vs {down:?}");
        run(&mut s, "solid.box", json!({"length": 1, "width": 1, "height": 1, "corner": [20, 0, 0], "body_name": "Cube"}));
        assert!(s.execute("surface.reverse_normal", &json!({"body": "Cube"})).is_err());
    }

    /// Delete Face without healing: a box's top taken off leaves a five-face surface; patching
    /// the opening and stitching gives back a box of the same volume. By default the gap still
    /// heals (here it can't: refused); deleting every face is refused.
    #[test]
    fn delete_face_without_healing_leaves_an_open_surface() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Box"}));
        run(&mut s, "face.delete", json!({"faces": [[5, 5, 10]], "heal": false}));
        let st = s.world_state();
        let open = &st.body("Box").unwrap().body;
        assert!(open.is_surface() && open.face_count() == 5);
        let rim = json!([[5, 0, 10], [10, 5, 10], [5, 10, 10], [0, 5, 10]]);
        run(&mut s, "surface.patch", json!({"edges": rim, "body": "Box", "body_name": "Lid"}));
        run(&mut s, "surface.stitch", json!({"bodies": ["Box", "Lid"]}));
        assert!(!s.world_state().body("Box").unwrap().body.is_surface());
        assert!((volume(&s, "Box") - 1000.0).abs() < 1e-9);
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((volume(&cold, "Box") - 1000.0).abs() < 1e-9);
        // By default the gap heals, and a box's sides can't close its top: refused, the box kept.
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [20, 0, 0], "body_name": "Healed"}));
        assert!(s.execute("face.delete", &json!({"faces": [[25, 5, 10]], "body": "Healed"})).is_err());
        assert!(!s.world_state().body("Healed").unwrap().body.is_surface());
        let all = json!([[25, 5, 10], [25, 5, 0], [25, 0, 5], [25, 10, 5], [20, 5, 5], [30, 5, 5]]);
        assert!(s.execute("face.delete", &json!({"faces": all, "body": "Healed", "heal": false})).is_err());
    }

    /// Surface Revolve of open curves about the sketch's y axis: a line beside the axis makes a
    /// cylinder (all of it, a quarter, a symmetric quarter), a line from the axis a cone, a half
    /// circle a solid ball; a profile turned all the way round is the solid solid.revolve makes.
    #[test]
    fn surface_revolve_curves_and_profiles() {
        use std::f64::consts::PI;
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.line", json!({"points": [[3, 0], [3, 4]], "ids": ["side"]}));
        run(&mut s, "sketch.line", json!({"points": [[0, 10], [3, 14]], "ids": ["slant"]}));
        run(&mut s, "sketch.finish", json!({}));
        let close = |a: f64, want: f64| assert!((a - want).abs() < want * 2e-3, "{a} vs {want}"); // (measured on meshes)
        run(&mut s, "surface.revolve", json!({"sketch": "S", "curves": ["side"], "axis": "y", "body_name": "Tube"}));
        close(area(&s, "Tube"), 2.0 * PI * 3.0 * 4.0);
        run(&mut s, "surface.revolve", json!({"sketch": "S", "curves": ["side"], "axis": "Y", "angle": "90 deg", "body_name": "Quarter"}));
        close(area(&s, "Quarter"), PI / 2.0 * 3.0 * 4.0);
        run(
            &mut s,
            "surface.revolve",
            json!({"sketch": "S", "curves": ["side"], "axis": "y", "angle": "90 deg", "symmetric": true, "body_name": "Both"}),
        );
        let bb = solidcraft_kernel::measure(&s.world_state().body("Both").unwrap().body).unwrap().bbox;
        let z = 3.0 * (PI / 4.0).sin();
        assert!((bb.min.z + z).abs() < 1e-3 && (bb.max.z - z).abs() < 1e-3, "{bb:?}");
        run(&mut s, "surface.revolve", json!({"sketch": "S", "curves": ["slant"], "axis": "y", "body_name": "Cone"}));
        close(area(&s, "Cone"), PI * 3.0 * 5.0);
        assert!(s.world_state().body("Cone").unwrap().body.is_surface());
        run(&mut s, "edit.undo", json!({}));
        assert!(s.world_state().body("Cone").is_none());
        run(&mut s, "edit.redo", json!({}));
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((area(&cold, "Cone") - area(&s, "Cone")).abs() < 1e-9);
        // A rectangle off the axis turned all the way round closes into the ring solid.revolve makes.
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "R"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [5, 0], "p1": [7, 3]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.revolve", json!({"sketch": "R", "axis": "y", "body_name": "Ring"}));
        run(&mut s, "solid.revolve", json!({"sketch": "R", "axis": "y", "body_name": "Solid"}));
        assert!(!s.world_state().body("Ring").unwrap().body.is_surface());
        close(volume(&s, "Ring"), volume(&s, "Solid"));
        // Refused: a curve across the axis, no axis, and a turn of more than 360°.
        assert!(s.execute("surface.revolve", &json!({"sketch": "S", "curves": ["side"], "axis": {"origin": [3, 2, 0], "dir": [1, 0, 0]}})).is_err());
        assert!(s.execute("surface.revolve", &json!({"sketch": "S", "curves": ["side"]})).is_err());
        assert!(s.execute("surface.revolve", &json!({"sketch": "S", "curves": ["side"], "axis": "y", "angle": "400 deg"})).is_err());
    }

    /// Surface Sweep: a line along y carried up a path in the XZ plane, straight then round a
    /// quarter turn (every point of it 5 from the arc's axis: 10 × (5 + 5π/2)), or round a
    /// corner (10 × both legs); a rectangle's walls up the straight leg; undo, redo, reopening.
    #[test]
    fn surface_sweep_curves_and_profiles() {
        use std::f64::consts::FRAC_PI_2;
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 10]], "ids": ["l"]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sketch.create", json!({"plane": "XZ", "name": "P"}));
        run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 5]], "ids": ["up"]}));
        let arc = run(
            &mut s,
            "sketch.arc.three_point",
            json!({"start": [0, 5], "end": [5, 10], "through": [5.0 - 5.0 / 2f64.sqrt(), 5.0 + 5.0 / 2f64.sqrt()]}),
        )["created"]["curves"][0]
            .as_str()
            .unwrap()
            .to_string();
        run(&mut s, "sketch.line", json!({"points": [[0, 5], [5, 5]], "ids": ["over"]}));
        run(&mut s, "sketch.finish", json!({}));
        let close = |a: f64, want: f64| assert!((a - want).abs() < want * 2e-3, "{a} vs {want}"); // (measured on meshes)
        run(&mut s, "surface.sweep", json!({"sketch": "S", "curves": ["l"], "path_sketch": "P", "path": ["up", arc], "body_name": "Bend"}));
        assert!(s.world_state().body("Bend").unwrap().body.is_surface());
        close(area(&s, "Bend"), 10.0 * (5.0 + 5.0 * FRAC_PI_2));
        run(&mut s, "surface.sweep", json!({"sketch": "S", "curves": ["l"], "path_sketch": "P", "path": ["up", "over"], "body_name": "Corner"}));
        close(area(&s, "Corner"), 100.0);
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "R"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [-1, -1], "p1": [1, 2]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.sweep", json!({"sketch": "R", "path_sketch": "P", "path": ["up"], "body_name": "Walls"}));
        let st = s.world_state();
        assert!(st.body("Walls").unwrap().body.is_surface() && st.body("Walls").unwrap().body.face_count() == 4);
        close(area(&s, "Walls"), 10.0 * 5.0);
        run(&mut s, "edit.undo", json!({}));
        assert!(s.world_state().body("Walls").is_none());
        run(&mut s, "edit.redo", json!({}));
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((area(&cold, "Bend") - area(&s, "Bend")).abs() < 1e-9);
        // Refused: no path, and a path curve that is not there.
        assert!(s.execute("surface.sweep", &json!({"sketch": "S", "curves": ["l"], "path_sketch": "P"})).is_err());
        assert!(s.execute("surface.sweep", &json!({"sketch": "S", "curves": ["l"], "path_sketch": "P", "path": ["nope"]})).is_err());
    }

    /// Surface Loft between sketches 5 apart: two lines give a 10 × 5 strip, two rectangles
    /// their four walls; undo, redo and reopening; a mix of open and closed sections, one
    /// section, and a section of two separate curves are refused.
    #[test]
    fn surface_loft_open_and_closed_sections() {
        let mut s = Session::default();
        run(&mut s, "construct.plane.offset", json!({"base": "XY", "offset": 5, "name": "Mid"}));
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "A"}));
        run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]], "ids": ["a"]}));
        run(&mut s, "sketch.line", json!({"points": [[0, 3], [10, 3]], "ids": ["apart"]}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [20, 0], "p1": [30, 6]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sketch.create", json!({"plane": "Mid", "name": "B"}));
        run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]], "ids": ["b"]}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [20, 0], "p1": [30, 6]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.loft", json!({"sections": [{"sketch": "A", "curves": ["a"]}, {"sketch": "B", "curves": ["b"]}], "body_name": "Strip"}));
        assert!(s.world_state().body("Strip").unwrap().body.is_surface());
        assert!((area(&s, "Strip") - 50.0).abs() < 1e-6, "{}", area(&s, "Strip"));
        run(&mut s, "surface.loft", json!({"sections": [{"sketch": "A", "profiles": [0]}, {"sketch": "B", "profiles": [0]}], "body_name": "Walls"}));
        let st = s.world_state();
        let walls = &st.body("Walls").unwrap().body;
        assert!(walls.is_surface() && walls.face_count() == 4);
        assert!((area(&s, "Walls") - 160.0).abs() < 1e-6, "{}", area(&s, "Walls"));
        run(&mut s, "edit.undo", json!({}));
        assert!(s.world_state().body("Walls").is_none());
        run(&mut s, "edit.redo", json!({}));
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((area(&cold, "Walls") - area(&s, "Walls")).abs() < 1e-9);
        assert!(s.execute("surface.loft", &json!({"sections": [{"sketch": "A", "curves": ["a"]}, {"sketch": "B", "profiles": [0]}]})).is_err());
        assert!(s.execute("surface.loft", &json!({"sections": [{"sketch": "A", "curves": ["a"]}]})).is_err());
        assert!(
            s.execute("surface.loft", &json!({"sections": [{"sketch": "A", "curves": ["a", "apart"]}, {"sketch": "B", "curves": ["b"]}]})).is_err()
        );
    }

    /// Each wall of a surface built from sketch curves is named after the curve it comes from,
    /// exactly (not from geometry): extrude, revolve, sweep and loft.
    #[test]
    fn surface_walls_are_named_after_their_curves() {
        let mut s = Session::default();
        run(&mut s, "construct.plane.offset", json!({"base": "XY", "offset": 5, "name": "Mid"}));
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.line", json!({"points": [[3, 0], [3, 4]], "ids": ["l1"]}));
        run(&mut s, "sketch.line", json!({"points": [[3, 4], [6, 4]], "ids": ["l2"]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sketch.create", json!({"plane": "XZ", "name": "P"}));
        run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 5]], "ids": ["up"]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sketch.create", json!({"plane": "Mid", "name": "T"}));
        run(&mut s, "sketch.line", json!({"points": [[3, 0], [3, 4]], "ids": ["m1"]}));
        run(&mut s, "sketch.line", json!({"points": [[3, 4], [6, 4]], "ids": ["m2"]}));
        run(&mut s, "sketch.finish", json!({}));
        let made = [
            ("surface.extrude", json!({"sketch": "S", "curves": ["l1", "l2"], "distance": 3, "body_name": "E"})),
            ("surface.revolve", json!({"sketch": "S", "curves": ["l1", "l2"], "axis": "y", "angle": "90 deg", "body_name": "R"})),
            ("surface.sweep", json!({"sketch": "S", "curves": ["l1", "l2"], "path_sketch": "P", "path": ["up"], "body_name": "W"})),
            (
                "surface.loft",
                json!({"sections": [{"sketch": "S", "curves": ["l1", "l2"]}, {"sketch": "T", "curves": ["m1", "m2"]}], "body_name": "L"}),
            ),
        ];
        for (cmd, p) in made {
            let body = p["body_name"].as_str().unwrap().to_string();
            run(&mut s, cmd, p);
            let fid = s.doc.features.last().unwrap().id;
            let st = s.model.state();
            let b = st.body(&body).unwrap();
            let names = solidcraft_doc::naming::face_names(b);
            for curve in ["l1", "l2"] {
                let want = format!("F{fid}:side:{curve}");
                let i = names.iter().position(|n| n.starts_with(&want)).unwrap_or_else(|| panic!("{cmd}: no face named {want}: {names:?}"));
                assert!(solidcraft_doc::naming::face_name_exact(b, i), "{cmd}: {want} is named from geometry");
            }
        }
    }

    /// Offset Surface: a cylinder's side picked once with its chain (both halves) moved out to
    /// radius 12; a box top moved up exactly; a zero distance copies; past the axis refused, the
    /// design left as it was; undo, redo and reopening.
    #[test]
    fn offset_surface_of_a_cylinder_and_a_box_top() {
        let mut s = Session::default();
        run(&mut s, "solid.cylinder", json!({"radius": 10, "height": 20, "body_name": "Can"}));
        run(&mut s, "surface.offset", json!({"faces": [[10, 0, 10]], "chain": true, "distance": 2, "body": "Can", "body_name": "Sleeve"}));
        let st = s.world_state();
        let sleeve = &st.body("Sleeve").unwrap().body;
        assert!(sleeve.is_surface() && sleeve.face_count() == 2);
        let want = 2.0 * std::f64::consts::PI * 12.0 * 20.0;
        assert!((area(&s, "Sleeve") - want).abs() < want * 1e-3, "{} vs {want}", area(&s, "Sleeve")); // (measured on meshes)
        assert!(st.body("Can").is_some(), "the source body stays");
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [30, 0, 0], "body_name": "Box"}));
        run(&mut s, "surface.offset", json!({"faces": [[35, 5, 10]], "distance": "5 mm", "body_name": "Lid"}));
        let bb = solidcraft_kernel::measure(&s.world_state().body("Lid").unwrap().body).unwrap().bbox;
        assert!((bb.min.z - 15.0).abs() < 1e-9 && (area(&s, "Lid") - 100.0).abs() < 1e-9);
        run(&mut s, "surface.offset", json!({"faces": [[35, 5, 10]], "distance": 0, "body_name": "Copy"}));
        assert!((area(&s, "Copy") - 100.0).abs() < 1e-9);
        let before = s.doc.features.len();
        assert!(s.execute("surface.offset", &json!({"faces": [[10, 0, 10]], "chain": true, "distance": -11, "body": "Can"})).is_err());
        assert_eq!(s.doc.features.len(), before);
        run(&mut s, "edit.undo", json!({}));
        assert!(s.world_state().body("Copy").is_none());
        run(&mut s, "edit.redo", json!({}));
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((area(&cold, "Sleeve") - area(&s, "Sleeve")).abs() < 1e-9);
        assert!(s.execute("surface.offset", &json!({"distance": 1})).is_err());
    }

    /// The planned surface commands are registered but refuse to run: disabled with the
    /// issue that tracks them, an error (never a panic) from every caller, and the design left
    /// as it was.
    #[test]
    fn planned_commands_refuse_and_leave_the_design_alone() {
        let planned = ["surface.ruled", "surface.boundary", "surface.blend", "surface.untrim", "surface.fillet", "inspect.continuity"];
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "B"}));
        let before = s.doc.features.len();
        for id in planned {
            let spec = crate::find_command(id).unwrap_or_else(|| panic!("{id} is not registered"));
            let info = spec.info(&s);
            assert!(!info.enabled, "{id} must stay disabled until it is built");
            let why = info.disabled_reason.unwrap_or_default();
            assert!(why.starts_with("not supported yet (issue #"), "{id}: {why}");
            assert!(!spec.icon.is_empty() && !spec.params.is_empty(), "{id}: icon and params doc");
            assert!(s.execute(id, &json!({"body": "B"})).is_err(), "{id} ran");
            let e = (spec.run)(&mut s, &json!({})).unwrap_err().to_string();
            assert!(e.starts_with("not supported yet: ") && e.contains("(issue #"), "{id}: {e}");
        }
        assert_eq!(s.doc.features.len(), before, "a planned command changed the design");
    }

    /// A trim keeps its construction plane when the plane is renamed.
    #[test]
    fn renaming_a_plane_keeps_a_surface_trim_on_it() {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [40, 20]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.patch", json!({"sketch": "S", "body_name": "Sheet"}));
        run(&mut s, "construct.plane.offset", json!({"base": "YZ", "offset": 25, "name": "Cut"}));
        run(&mut s, "surface.trim", json!({"body": "Sheet", "plane": "Cut", "keep": [10, 10, 0], "name": "Trim"}));
        let trimmed = area(&s, "Sheet");
        run(&mut s, "timeline.rename", json!({"feature": "Cut", "name": "Cutter"}));
        assert!(s.model.results.iter().all(|r| r.error.is_none()), "the trim still finds its plane");
        let def = run(&mut s, "timeline.inspect", json!({"feature": "Trim"}));
        assert_eq!(def["definition"]["plane"]["name"], "Cutter", "{def}");
        assert!((area(&s, "Sheet") - trimmed).abs() < 1e-6, "{} vs {trimmed}", area(&s, "Sheet"));
        assert!(trimmed < 40.0 * 20.0 * 0.9, "the sheet is trimmed: {trimmed}");
    }

    /// A rectangle patched, trimmed by a plane and thickened; a box's open top patched; a rim
    /// that is not planar patched.
    #[test]
    fn patch_trim_thicken_and_stitch() {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [10, 10]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "surface.patch", json!({"sketch": "S", "body_name": "Sheet"}));
        let st = s.world_state();
        assert!(st.body("Sheet").unwrap().body.is_surface());
        run(&mut s, "surface.trim", json!({"body": "Sheet", "plane": {"origin": [3, 0, 0], "normal": [1, 0, 0]}, "keep": [8, 5, 0]}));
        run(&mut s, "surface.extend", json!({"body": "Sheet", "edges": [[10, 5, 0]], "distance": 5}));
        run(&mut s, "surface.thicken", json!({"bodies": ["Sheet"], "thickness": "2 mm", "body_name": "Slab"}));
        let v = volume(&s, "Slab");
        assert!((v - 240.0).abs() < 1e-9, "{v}");
        // Stitch the patch of a box's open top back on.
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [20, 0, 0], "body_name": "Cup"}));
        run(&mut s, "surface.patch", json!({"edges": [[25, 0, 10], [30, 5, 10], [25, 10, 10], [20, 5, 10]], "body": "Cup", "body_name": "Lid"}));
        assert!(s.world_state().body("Lid").unwrap().body.is_surface());
        // A rim that is not planar: a box with its top cut away by a sideways cylinder.
        run(&mut s, "solid.box", json!({"length": 28, "width": 28, "height": 20, "corner": [-14, -14, 0], "body_name": "Saddle"}));
        run(
            &mut s,
            "solid.cylinder",
            json!({"radius": 40, "height": 120, "base": [0, -60, 50], "axis": "Y", "operation": "cut", "targets": ["Saddle"]}),
        );
        let z = |x: f64| 50.0 - (1600.0 - x * x).sqrt();
        let rim = json!([[14, 0, z(14.0)], [0, 14, z(0.0)], [-14, 0, z(-14.0)], [0, -14, z(0.0)]]);
        run(&mut s, "surface.patch", json!({"edges": rim, "body": "Saddle", "body_name": "Cover"}));
        let st = s.world_state();
        let cover = &st.body("Cover").unwrap().body;
        assert!(cover.is_surface() && cover.face_count() == 1);
        // The cover is a smooth surface on the rim, close to the cylinder that cut it (its area
        // within 1% of the cylinder's inside the rim).
        let area = solidcraft_kernel::measure(cover).unwrap().area;
        let want = 28.0 * 40.0 * 2.0 * (14.0f64 / 40.0).asin();
        assert!((area - want).abs() < want * 1e-2, "{area} vs {want}");
        assert!(s.execute("surface.thicken", &json!({"bodies": ["Nope"], "thickness": 1})).is_err());
        assert!(s.execute("surface.trim", &json!({"body": "Sheet"})).is_err());
    }
}
