//! Dialogs for the surface tools and face splitting (#19): Surface Extrude, Revolve, Sweep and
//! Loft, Patch, Stitch, Thicken, Extend, Trim and Split Face. Each has a live preview and Edit Feature.

use serde_json::{Map, Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::doc::expr::Kind as ValueKind;
use solidcraft_engine::doc::{AxisRef, Direction, FaceAt, FeatureKind, Mat, PlaneRef, ProfileSel};
use solidcraft_engine::geom::Vec3;

use crate::SolveApp;
use crate::dialogs::{Kind, OP_LABELS, OPS, combo, field, op_index, row_label};
use crate::selection::{AXES, BODIES, CURVES, EDGES, FACES, PLANES, PROFILES, SelInput};

/// A surface tool dialog's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Surf {
    /// `direction`: an index into [`EXTRUDE_SIDES`].
    Extrude {
        distance: String,
        direction: usize,
    },
    /// `axis`: a stored {origin, dir} axis, used while no axis is picked.
    Revolve {
        angle: String,
        symmetric: bool,
        axis: Option<Value>,
    },
    Sweep,
    Loft,
    Patch,
    Unstitch,
    ReverseNormal,
    Offset {
        distance: String,
        chain: bool,
    },
    Stitch {
        tolerance: String,
    },
    ConvertMesh {
        tolerance: String,
    },
    Thicken {
        thickness: String,
        symmetric: bool,
        operation: usize,
    },
    Extend {
        distance: String,
    },
    Trim,
    SplitFace,
}

impl Surf {
    pub fn title(&self) -> &'static str {
        match self {
            Surf::Extrude { .. } => "SURFACE EXTRUDE",
            Surf::Revolve { .. } => "SURFACE REVOLVE",
            Surf::Sweep => "SURFACE SWEEP",
            Surf::Loft => "SURFACE LOFT",
            Surf::Patch => "PATCH",
            Surf::Unstitch => "UNSTITCH",
            Surf::ReverseNormal => "REVERSE NORMAL",
            Surf::Offset { .. } => "OFFSET SURFACE",
            Surf::Stitch { .. } => "STITCH",
            Surf::ConvertMesh { .. } => "CONVERT MESH",
            Surf::Thicken { .. } => "THICKEN",
            Surf::Extend { .. } => "EXTEND",
            Surf::Trim => "TRIM",
            Surf::SplitFace => "SPLIT FACE",
        }
    }

    /// The value on the canvas.
    pub fn primary(&mut self) -> Option<(&'static str, ValueKind, &mut String)> {
        Some(match self {
            Surf::Thicken { thickness, .. } => ("Thickness", ValueKind::Length, thickness),
            Surf::Extend { distance } | Surf::Extrude { distance, .. } | Surf::Offset { distance, .. } => ("Distance", ValueKind::Length, distance),
            Surf::Revolve { angle, .. } => ("Angle", ValueKind::Angle, angle),
            _ => return None,
        })
    }
}

/// The sides a surface extrude goes to (labels, and the `direction` parameter).
const EXTRUDE_SIDES: [(&str, &str); 3] = [("One Side", "positive"), ("Symmetric", "symmetric"), ("Flipped", "negative")];

fn inputs(k: &Surf) -> Vec<SelInput> {
    match k {
        Surf::Extrude { .. } => vec![SelInput::new("Profiles or curves", PROFILES | CURVES, true)],
        Surf::Revolve { .. } => vec![SelInput::new("Profiles or curves", PROFILES | CURVES, true), SelInput::new("Axis", AXES, false)],
        Surf::Sweep => vec![SelInput::new("Profiles or curves", PROFILES | CURVES, true), SelInput::new("Path", CURVES, true)],
        Surf::Loft => vec![SelInput::new("Sections (one sketch each)", PROFILES | CURVES, true)],
        Surf::Patch => vec![SelInput::new("Boundary", PROFILES | EDGES, true)],
        Surf::Unstitch => vec![SelInput::new("Body, or faces to take out", BODIES | FACES, true)],
        Surf::ReverseNormal => vec![SelInput::new("Surface, or faces to turn over", BODIES | FACES, true)],
        Surf::Offset { .. } => vec![SelInput::new("Faces", FACES, true)],
        Surf::Stitch { .. } => vec![SelInput::new("Surfaces", BODIES, true)],
        Surf::ConvertMesh { .. } => vec![SelInput::new("Mesh body", BODIES, false)],
        Surf::Thicken { .. } => vec![SelInput::new("Surfaces", BODIES, true)],
        Surf::Extend { .. } => vec![SelInput::new("Edges", EDGES, true)],
        Surf::Trim => vec![
            SelInput::new("Surface", BODIES, false),
            SelInput::new("Trimming tool", PLANES | FACES, false),
            SelInput::new("Side to keep", FACES, false),
        ],
        Surf::SplitFace => vec![SelInput::new("Faces to split", FACES, true), SelInput::new("Splitting tool", PLANES | FACES, false)],
    }
}

/// The dialog for a surface command.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let k = match id {
        "surface.extrude" => Surf::Extrude { distance: "10 mm".into(), direction: 0 },
        "surface.revolve" => Surf::Revolve { angle: "360 deg".into(), symmetric: false, axis: None },
        "surface.sweep" => Surf::Sweep,
        "surface.loft" => Surf::Loft,
        "surface.patch" => Surf::Patch,
        "surface.unstitch" => Surf::Unstitch,
        "surface.reverse_normal" => Surf::ReverseNormal,
        "surface.offset" => Surf::Offset { distance: "2 mm".into(), chain: false },
        "surface.stitch" => Surf::Stitch { tolerance: String::new() },
        "mesh.convert" => Surf::ConvertMesh { tolerance: "0.05 mm".into() },
        "surface.thicken" => Surf::Thicken { thickness: "2 mm".into(), symmetric: false, operation: 0 },
        "surface.extend" => Surf::Extend { distance: "10 mm".into() },
        "surface.trim" => Surf::Trim,
        "solid.split_face" => Surf::SplitFace,
        _ => return None,
    };
    let i = inputs(&k);
    Some((Kind::Surface(k), i))
}

/// The rows after the inputs; true when Enter was pressed in a field.
pub fn rows(ui: &mut egui::Ui, k: &mut Surf) -> bool {
    let mut enter = false;
    match k {
        Surf::Stitch { tolerance } | Surf::ConvertMesh { tolerance } => {
            row_label(ui, "Tolerance");
            let r = ui.add(egui::TextEdit::singleline(tolerance).hint_text("tight"));
            crate::params_dialog::complete(ui, &r, tolerance);
            enter |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.end_row();
        }
        Surf::Thicken { thickness, symmetric, operation } => {
            row_label(ui, "Thickness");
            enter |= field(ui, thickness);
            ui.end_row();
            row_label(ui, "Direction");
            let mut dir = usize::from(*symmetric);
            combo(ui, "th_dir", &["One Side", "Symmetric"], &mut dir);
            *symmetric = dir == 1;
            ui.end_row();
            row_label(ui, "Operation");
            combo(ui, "th_op", &OP_LABELS, operation);
            ui.end_row();
        }
        Surf::Extend { distance } => {
            row_label(ui, "Distance");
            enter |= field(ui, distance);
            ui.end_row();
        }
        Surf::Revolve { angle, symmetric, .. } => {
            row_label(ui, "Angle");
            enter |= field(ui, angle);
            ui.end_row();
            row_label(ui, "Direction");
            let mut dir = usize::from(*symmetric);
            combo(ui, "sr_dir", &["One Side", "Symmetric"], &mut dir);
            *symmetric = dir == 1;
            ui.end_row();
        }
        Surf::Offset { distance, chain } => {
            row_label(ui, "Distance");
            enter |= field(ui, distance);
            ui.end_row();
            row_label(ui, "Tangent chain");
            ui.checkbox(chain, "");
            ui.end_row();
        }
        Surf::Extrude { distance, direction } => {
            row_label(ui, "Distance");
            enter |= field(ui, distance);
            ui.end_row();
            row_label(ui, "Direction");
            combo(ui, "sx_dir", &EXTRUDE_SIDES.map(|(label, _)| label), direction);
            ui.end_row();
        }
        Surf::Patch | Surf::Trim | Surf::SplitFace | Surf::Unstitch | Surf::ReverseNormal | Surf::Sweep | Surf::Loft => {}
    }
    enter
}

fn items(inputs: &[SelInput], i: usize) -> &[Sel] {
    inputs.get(i).map(|x| x.items.as_slice()).unwrap_or(&[])
}

fn pt(v: Vec3) -> Value {
    json!([v.x, v.y, v.z])
}

fn bodies(inputs: &[SelInput], i: usize) -> Vec<String> {
    items(inputs, i).iter().filter_map(|x| if let Sel::Body { name } = x { Some(name.clone()) } else { None }).collect()
}

/// One body and points on the picked faces of it (none: a whole body picked).
fn body_and_faces(picks: &[Sel]) -> Result<(String, Vec<Value>), String> {
    let body_of = |x: &Sel| match x {
        Sel::Body { name } | Sel::Face { body: name, .. } => Some(name.clone()),
        _ => None,
    };
    let body = picks.iter().find_map(body_of).ok_or("pick a body or some of its faces")?;
    if picks.iter().any(|x| body_of(x).as_ref() != Some(&body)) {
        return Err("pick faces of one body".into());
    }
    let faces = picks.iter().filter_map(|x| if let Sel::Face { point, .. } = x { Some(pt(*point)) } else { None }).collect();
    Ok((body, faces))
}

/// The sketch, profiles and curves of a surface builder's picks (all in one sketch).
fn sheet_picks(app: &SolveApp, picks: &[Sel]) -> Result<Value, String> {
    let profiles: Vec<(u64, usize)> =
        picks.iter().filter_map(|x| if let Sel::Profile { sketch, index } = x { Some((*sketch, *index)) } else { None }).collect();
    let curves: Vec<String> = picks.iter().filter_map(|x| if let Sel::SketchCurve { id } = x { Some(id.clone()) } else { None }).collect();
    let sketch = match profiles.first() {
        Some((sk, _)) => *sk,
        None => crate::dialogs::curves_sketch(app, &curves, None).ok_or("the curves must be in one sketch")?,
    };
    if profiles.iter().any(|(sk, _)| *sk != sketch) {
        return Err("the profiles and curves must be in one sketch".into());
    }
    let mut p = json!({ "sketch": sketch });
    if !curves.is_empty() {
        p["curves"] = json!(curves);
    }
    if !profiles.is_empty() {
        p["profiles"] = json!(profiles.iter().map(|(_, i)| *i).collect::<Vec<_>>());
    }
    Ok(p)
}

/// A surface builder's stored picks as selections again: its profiles, then its curves.
fn sheet_sels(app: &SolveApp, sketch: u64, profiles: Option<&ProfileSel>, curves: &[String]) -> Vec<Sel> {
    let st = app.session.world_state();
    let mut sels: Vec<Sel> = match (profiles, st.sketch(sketch)) {
        (Some(sel), Some(ss)) => crate::dialogs::profile_indices(ss, sel).into_iter().map(|index| Sel::Profile { sketch, index }).collect(),
        _ => Vec::new(),
    };
    sels.extend(curves.iter().map(|id| Sel::SketchCurve { id: id.clone() }));
    sels
}

/// A splitting or trimming tool: a plane by name, a planar face as its plane, or any other
/// face as {body, point} (its surface extended).
fn tool(app: &SolveApp, sel: Option<&Sel>) -> Result<(&'static str, Value), String> {
    match sel {
        Some(Sel::Plane { name }) => Ok(("plane", json!(name))),
        Some(Sel::Face { body, index, point }) => match crate::dialogs::planar_face(&app.session, body, *index) {
            Some((_, n)) => Ok(("plane", json!({"origin": pt(*point), "normal": pt(n)}))),
            None => Ok(("tool", json!({"body": body, "point": pt(*point)}))),
        },
        _ => Err("pick a plane or a face as the tool".into()),
    }
}

/// The command a surface dialog makes.
pub fn commands(app: &SolveApp, k: &Surf, inputs: &[SelInput], extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    let need = |i: usize, what: &str| if items(inputs, i).is_empty() { Err(format!("select {what} first")) } else { Ok(()) };
    let (cmd, mut p) = match k {
        Surf::Extrude { distance, direction } => {
            need(0, "profiles or curves to extrude")?;
            let mut p = sheet_picks(app, items(inputs, 0))?;
            p["distance"] = json!(distance);
            p["direction"] = json!(EXTRUDE_SIDES.get(*direction).map_or("positive", |(_, d)| *d));
            ("surface.extrude", p)
        }
        Surf::Revolve { angle, symmetric, axis } => {
            need(0, "profiles or curves to revolve")?;
            let mut p = sheet_picks(app, items(inputs, 0))?;
            p["axis"] = match items(inputs, 1).first() {
                Some(Sel::Axis { name }) => json!(name),
                Some(Sel::SketchCurve { id }) => json!(id),
                _ => axis.clone().ok_or("select an axis")?,
            };
            p["angle"] = json!(angle);
            if *symmetric {
                p["symmetric"] = json!(true);
            }
            ("surface.revolve", p)
        }
        Surf::Sweep => {
            need(0, "profiles or curves to sweep")?;
            need(1, "the path")?;
            let mut p = sheet_picks(app, items(inputs, 0))?;
            let path: Vec<String> =
                items(inputs, 1).iter().filter_map(|x| if let Sel::SketchCurve { id } = x { Some(id.clone()) } else { None }).collect();
            p["path_sketch"] =
                json!(crate::dialogs::curves_sketch(app, &path, p["sketch"].as_u64()).ok_or("the path curves must be in one other sketch")?);
            p["path"] = json!(path);
            ("surface.sweep", p)
        }
        Surf::Loft => {
            need(0, "the sections, one sketch each")?;
            // One section per sketch, in the order the sketches were first picked.
            let st = app.session.world_state();
            let sketch_of = |x: &Sel| match x {
                Sel::Profile { sketch, .. } => Some(*sketch),
                Sel::SketchCurve { id } => st.sketches.iter().rev().find(|ss| ss.sketch.curve_index(id).is_some()).map(|ss| ss.feature),
                _ => None,
            };
            let mut order: Vec<u64> = Vec::new();
            for x in items(inputs, 0) {
                let sk = sketch_of(x).ok_or("a picked curve is no longer in a sketch")?;
                if !order.contains(&sk) {
                    order.push(sk);
                }
            }
            if order.len() < 2 {
                return Err("pick sections in two or more sketches".into());
            }
            let sections: Vec<Value> = order
                .iter()
                .map(|sk| {
                    let picks: Vec<Sel> = items(inputs, 0).iter().filter(|x| sketch_of(x) == Some(*sk)).cloned().collect();
                    sheet_picks(app, &picks)
                })
                .collect::<Result<_, String>>()?;
            ("surface.loft", json!({ "sections": sections }))
        }
        Surf::Patch => {
            need(0, "a boundary (sketch profiles or a loop of edges)")?;
            let profiles: Vec<(u64, usize)> =
                items(inputs, 0).iter().filter_map(|x| if let Sel::Profile { sketch, index } = x { Some((*sketch, *index)) } else { None }).collect();
            if let Some((sk, _)) = profiles.first() {
                let idx: Vec<usize> = profiles.iter().filter(|(s, _)| s == sk).map(|(_, i)| *i).collect();
                ("surface.patch", json!({"sketch": sk, "profiles": idx}))
            } else {
                let edges: Vec<Value> =
                    items(inputs, 0).iter().filter_map(|x| if let Sel::Edge { point, .. } = x { Some(pt(*point)) } else { None }).collect();
                let body = items(inputs, 0).iter().find_map(|x| if let Sel::Edge { body, .. } = x { Some(body.clone()) } else { None });
                ("surface.patch", json!({"edges": edges, "body": body}))
            }
        }
        Surf::Offset { distance, chain } => {
            need(0, "the faces to offset")?;
            let (body, faces) = body_and_faces(items(inputs, 0))?;
            if faces.is_empty() {
                return Err("pick faces to offset".into());
            }
            let mut p = json!({"body": body, "faces": faces, "distance": distance});
            if *chain {
                p["chain"] = json!(true);
            }
            ("surface.offset", p)
        }
        Surf::Unstitch | Surf::ReverseNormal => {
            need(0, "a body or some of its faces")?;
            let (body, faces) = body_and_faces(items(inputs, 0))?;
            let cmd = if matches!(k, Surf::Unstitch) { "surface.unstitch" } else { "surface.reverse_normal" };
            (cmd, json!({"body": body, "faces": faces}))
        }
        Surf::Stitch { tolerance } => {
            need(0, "the surfaces to stitch")?;
            let mut p = json!({ "bodies": bodies(inputs, 0) });
            if !tolerance.trim().is_empty() {
                p["tolerance"] = json!(tolerance);
            }
            ("surface.stitch", p)
        }
        Surf::ConvertMesh { tolerance } => {
            need(0, "a closed mesh body")?;
            let body = bodies(inputs, 0).into_iter().next().ok_or("select a mesh body")?;
            ("mesh.convert", json!({"body":body,"tolerance":tolerance}))
        }
        Surf::Thicken { thickness, symmetric, operation } => {
            need(0, "the surfaces to thicken")?;
            (
                "surface.thicken",
                json!({"bodies": bodies(inputs, 0), "thickness": thickness, "symmetric": symmetric, "operation": OPS.get(*operation).copied().unwrap_or("new")}),
            )
        }
        Surf::Extend { distance } => {
            need(0, "the edges to extend")?;
            let body =
                items(inputs, 0).iter().find_map(|x| if let Sel::Edge { body, .. } = x { Some(body.clone()) } else { None }).unwrap_or_default();
            let edges: Vec<Value> =
                items(inputs, 0).iter().filter_map(|x| if let Sel::Edge { point, .. } = x { Some(pt(*point)) } else { None }).collect();
            ("surface.extend", json!({"body": body, "edges": edges, "distance": distance}))
        }
        Surf::Trim => {
            need(0, "the surface to trim")?;
            need(1, "the trimming tool")?;
            need(2, "the side to keep (a point on it)")?;
            let body = bodies(inputs, 0).into_iter().next().unwrap_or_default();
            let (key, t) = tool(app, items(inputs, 1).first())?;
            let keep = items(inputs, 2)
                .first()
                .and_then(|x| if let Sel::Face { point, .. } = x { Some(pt(*point)) } else { None })
                .ok_or("pick the side to keep")?;
            let mut p = json!({"body": body, "keep": keep});
            p[key] = t;
            ("surface.trim", p)
        }
        Surf::SplitFace => {
            need(0, "the faces to split")?;
            need(1, "the splitting tool")?;
            let faces: Vec<Value> =
                items(inputs, 0).iter().filter_map(|x| if let Sel::Face { point, .. } = x { Some(pt(*point)) } else { None }).collect();
            let body = items(inputs, 0).iter().find_map(|x| if let Sel::Face { body, .. } = x { Some(body.clone()) } else { None });
            let (key, t) = tool(app, items(inputs, 1).first())?;
            let mut p = json!({"faces": faces, "body": body});
            p[key] = t;
            ("solid.split_face", p)
        }
    };
    if let Value::Object(m) = &mut p {
        for (k, v) in extra {
            m.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    Ok(vec![(cmd.to_string(), p)])
}

/// A tool as a selection again: a plane, else the face at the stored point.
fn tool_sel(app: &SolveApp, plane: &PlaneRef, tool: Option<&FaceAt>) -> Option<Sel> {
    match tool {
        Some(t) => crate::dialogs::face_sel(&app.session, t.point),
        None => crate::dialogs::plane_sel(&app.session, plane),
    }
}

/// The dialog for editing a surface feature.
/// `frame` maps the feature's component frame to the world, for an axis stored as a line.
pub fn for_feature(app: &SolveApp, kind: &FeatureKind, frame: &Mat) -> Option<(Kind, Vec<SelInput>, Map<String, Value>)> {
    let s = &app.session;
    let edge = |p: &Vec3| crate::dialogs::edge_sel(s, *p);
    let (k, fill): (Surf, Vec<Vec<Sel>>) = match kind {
        FeatureKind::SurfaceRevolve { sketch, profiles, curves, axis, angle, angle2, symmetric } => {
            let (pick, stored) = match axis {
                AxisRef::World { axis } => (Some(Sel::Axis { name: axis.to_ascii_uppercase() }), None),
                AxisRef::SketchAxis { axis } => (Some(Sel::Axis { name: axis.to_ascii_lowercase() }), None),
                AxisRef::SketchLine { curve } => (Some(Sel::SketchCurve { id: curve.clone() }), None),
                AxisRef::Line { origin, dir } => (
                    None,
                    Some(json!({
                        "origin": pt(solidcraft_engine::doc::apply_point(frame, *origin)),
                        "dir": pt(solidcraft_engine::doc::apply_vector(frame, *dir))
                    })),
                ),
            };
            let mut extra = Map::new();
            if let Some(a2) = angle2 {
                extra.insert("angle2".into(), json!(a2));
            }
            let k = Surf::Revolve { angle: angle.clone(), symmetric: *symmetric, axis: stored };
            let mut ins = inputs(&k);
            for (inp, items) in ins.iter_mut().zip([sheet_sels(app, *sketch, profiles.as_ref(), curves), pick.into_iter().collect()]) {
                inp.items = items;
            }
            return Some((Kind::Surface(k), ins, extra));
        }
        FeatureKind::SurfaceSweep { sketch, profiles, curves, path, .. } => (
            Surf::Sweep,
            vec![sheet_sels(app, *sketch, profiles.as_ref(), curves), path.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect()],
        ),
        FeatureKind::SurfaceLoft { sections } => {
            (Surf::Loft, vec![sections.iter().flat_map(|sec| sheet_sels(app, sec.sketch, sec.profiles.as_ref(), &sec.curves)).collect()])
        }
        FeatureKind::SurfaceExtrude { sketch, profiles, curves, extent } => {
            let sels = sheet_sels(app, *sketch, profiles.as_ref(), curves);
            let direction = match extent.direction {
                Direction::Positive => 0,
                Direction::Symmetric => 1,
                Direction::Negative => 2,
            };
            (Surf::Extrude { distance: extent.distance.clone(), direction }, vec![sels])
        }
        FeatureKind::Patch { sketch, edges, .. } => {
            let mut sels: Vec<Sel> = edges.iter().filter_map(edge).collect();
            if let Some(sk) = sketch {
                let st = s.world_state();
                if let Some(ss) = st.sketch(*sk) {
                    sels.extend((0..ss.profiles.len()).map(|index| Sel::Profile { sketch: *sk, index }));
                }
            }
            (Surf::Patch, vec![sels])
        }
        FeatureKind::SurfaceOffset { faces, distance, chain, .. } => {
            (Surf::Offset { distance: distance.clone(), chain: *chain }, vec![faces.iter().filter_map(|p| crate::dialogs::face_sel(s, *p)).collect()])
        }
        FeatureKind::Unstitch { body, faces } | FeatureKind::ReverseNormal { body, faces } => {
            let sels: Vec<Sel> = if faces.is_empty() {
                vec![Sel::Body { name: body.clone() }]
            } else {
                faces.iter().filter_map(|p| crate::dialogs::face_sel(s, *p)).collect()
            };
            (if matches!(kind, FeatureKind::Unstitch { .. }) { Surf::Unstitch } else { Surf::ReverseNormal }, vec![sels])
        }
        FeatureKind::Stitch { bodies, tolerance } => (
            Surf::Stitch { tolerance: if tolerance.trim() == "0" { String::new() } else { tolerance.clone() } },
            vec![bodies.iter().map(|n| Sel::Body { name: n.clone() }).collect()],
        ),
        FeatureKind::MeshConvert { body, tolerance } => {
            (Surf::ConvertMesh { tolerance: tolerance.clone() }, vec![vec![Sel::Body { name: body.clone() }]])
        }
        FeatureKind::Thicken { bodies, thickness, symmetric, operation, .. } => (
            Surf::Thicken { thickness: thickness.clone(), symmetric: *symmetric, operation: op_index(operation) },
            vec![bodies.iter().map(|n| Sel::Body { name: n.clone() }).collect()],
        ),
        FeatureKind::SurfaceExtend { edges, distance, .. } => {
            (Surf::Extend { distance: distance.clone() }, vec![edges.iter().filter_map(edge).collect()])
        }
        FeatureKind::SurfaceTrim { body, plane, tool, keep } => (
            Surf::Trim,
            vec![
                vec![Sel::Body { name: body.clone() }],
                tool_sel(app, plane, tool.as_ref()).into_iter().collect(),
                crate::dialogs::face_sel(s, *keep).into_iter().collect(),
            ],
        ),
        FeatureKind::SplitFace { faces, plane, tool, .. } => (
            Surf::SplitFace,
            vec![faces.iter().filter_map(|p| crate::dialogs::face_sel(s, *p)).collect(), tool_sel(app, plane, tool.as_ref()).into_iter().collect()],
        ),
        _ => return None,
    };
    let mut ins = inputs(&k);
    for (inp, items) in ins.iter_mut().zip(fill) {
        inp.items = items;
    }
    Some((Kind::Surface(k), ins, Map::new()))
}
