//! The timeline (bottom): features in order, the rollback marker, errors and warnings.

pub(crate) use crate::icons::glyphs::transport_glyph;
use egui::{Rect, Sense, Stroke, UiBuilder, pos2, vec2};
use serde_json::json;
use solidcraft_engine::doc::FeatureKind;

use crate::theme::Tokens;
use crate::{SolveApp, icons};

pub(crate) fn icon_of(k: &FeatureKind) -> &'static str {
    match k {
        FeatureKind::Sketch { .. } => "sketch",
        FeatureKind::Extrude { .. } => "extrude",
        FeatureKind::SurfaceExtrude { .. } => "surface_extrude",
        FeatureKind::Unstitch { .. } => "unstitch",
        FeatureKind::ReverseNormal { .. } => "reverse_normal",
        FeatureKind::SurfaceRevolve { .. } => "surface_revolve",
        FeatureKind::SurfaceSweep { .. } => "surface_sweep",
        FeatureKind::SurfaceLoft { .. } => "surface_loft",
        FeatureKind::SurfaceOffset { .. } => "surface_offset",
        FeatureKind::Revolve { .. } => "revolve",
        FeatureKind::Fillet { .. } => "fillet",
        FeatureKind::Chamfer { .. } => "chamfer",
        FeatureKind::Box { .. } => "box",
        FeatureKind::Cylinder { .. } => "cylinder",
        FeatureKind::Sphere { .. } => "sphere",
        FeatureKind::Torus { .. } => "torus",
        FeatureKind::Combine { .. } => "combine",
        FeatureKind::Pattern { pattern: solidcraft_engine::doc::PatternKind::Rectangular { .. }, .. } => "pattern_rect",
        FeatureKind::Pattern { .. } => "pattern_circ",
        FeatureKind::Mirror { .. } => "mirror",
        FeatureKind::ConstructionPlane { .. } => "plane",
        FeatureKind::ConstructionAxis { .. } => "axis",
        FeatureKind::ConstructionPoint { .. } => "point",
        FeatureKind::Hole { .. } => "hole",
        FeatureKind::Loft { .. } => "loft",
        FeatureKind::Sweep { .. } => "sweep",
        FeatureKind::Shell { .. } => "shell",
        FeatureKind::Draft { .. } => "draft",
        FeatureKind::Split { .. } => "split",
        FeatureKind::Move { .. } => "move",
        FeatureKind::Import { .. } | FeatureKind::MeshImport { .. } | FeatureKind::MeshConvert { .. } => "import",
        other => crate::workspace::feature_icon(other).unwrap_or("feature"),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    Marker,
    Feature(u64),
}

/// Slot index (0..=n) nearest to screen x, given the left edges of the feature buttons.
fn slot_at(xs: &[f32], end: f32, x: f32) -> usize {
    let mut best = (0usize, f32::INFINITY);
    for (i, sx) in xs.iter().chain(std::iter::once(&end)).enumerate() {
        let d = (sx - 2.0 - x).abs();
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// Ask before deleting a feature others depend on; delete right away otherwise.
pub(crate) fn delete_feature(app: &mut SolveApp, id: u64) {
    delete_features(app, vec![id]);
}

pub(crate) fn delete_features(app: &mut SolveApp, ids: Vec<u64>) {
    let mut with = std::collections::BTreeSet::new();
    let mut fail = std::collections::BTreeSet::new();
    for id in &ids {
        let deps = app.session.execute("timeline.dependents", &json!({ "feature": id })).unwrap_or_default();
        for (key, output) in [("deleted_with_it", &mut with), ("would_fail", &mut fail)] {
            output
                .extend(deps.get(key).and_then(|value| value.as_array()).into_iter().flatten().filter_map(|value| value.as_str().map(String::from)));
        }
    }
    if with.is_empty() && fail.is_empty() {
        let _ = app.run("timeline.delete", json!({ "features": ids }));
    } else {
        app.dialog = Some(crate::dialogs::Dialog::confirm_delete_many(ids, with.into_iter().collect(), fail.into_iter().collect()));
    }
}

pub fn timeline(app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    egui::Panel::bottom("sc_timeline").exact_size(40.0).frame(egui::Frame::NONE.fill(t.timeline)).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.border));
        let n = app.session.doc.features.len();
        let marker = app.session.doc.marker.unwrap_or(n);
        // Playback controls.
        let mut x = r.left() + 10.0;
        let controls = [("first", Some(0usize)), ("back", Some(marker.saturating_sub(1))), ("forward", Some((marker + 1).min(n))), ("last", None)];
        for (what, target) in controls {
            let br = Rect::from_center_size(pos2(x + 11.0, r.center().y), vec2(22.0, 22.0));
            let resp = ui.interact(br, ui.id().with(("tl", what)), Sense::click());
            if resp.hovered() {
                p.rect_filled(br, 3.0, t.hover);
            }
            transport_glyph(&p, br.center(), what, t.icon);
            if resp
                .on_hover_text(match what {
                    "first" => "Roll back to the start",
                    "back" => "Step back one feature",
                    "forward" => "Step forward one feature",
                    _ => "Play to the end",
                })
                .clicked()
            {
                let pos = match target {
                    Some(k) if k < n => json!({"position": k}),
                    _ => json!({}),
                };
                let _ = app.run("timeline.roll_to", pos);
            }
            x += 24.0;
        }
        x += 14.0;
        // The features scroll horizontally in the space right of the playback controls.
        let strip = Rect::from_min_max(pos2(x, r.top()), r.right_bottom());
        ui.scope_builder(UiBuilder::new().max_rect(strip), |ui| {
            egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                let p = ui.painter().clone();
                let mut x = ui.max_rect().left();
                let origin = x;
                let feats: Vec<_> = app
                    .session
                    .doc
                    .features
                    .iter()
                    .map(|f| (f.id, f.name.clone(), icon_of(&f.kind), f.suppressed, f.kind.type_name(), f.kind.clone()))
                    .collect();
                // The feature whose body is under the cursor in the viewport.
                let st = app.session.world_state();
                let from_view = match &app.viewport.hover {
                    Some(
                        crate::viewport::Hit::Face { body, .. } | crate::viewport::Hit::Edge { body, .. } | crate::viewport::Hit::Vertex { body, .. },
                    ) => st.body(body).map(|b| b.feature),
                    _ => None,
                };
                let drag_id = ui.id().with("tl_drag");
                let mut drag: Option<Drag> = ui.data(|d| d.get_temp(drag_id));
                let pointer = ui.input(|i| i.pointer.hover_pos());
                let mut xs: Vec<f32> = Vec::new();
                let mut hovered: Option<u64> = None;
                let mut edit: Option<u64> = None;
                let mut marker_rect: Option<Rect> = None;
                for (i, (id, name, icon, suppressed, ty, kind)) in feats.iter().enumerate() {
                    if i == marker {
                        marker_rect = Some(Rect::from_min_size(pos2(x, r.top() + 5.0), vec2(6.0, r.height() - 10.0)));
                        x += 10.0;
                    }
                    xs.push(x);
                    let br = Rect::from_min_size(pos2(x, r.top() + 6.0), vec2(28.0, 28.0));
                    crate::scenario::publish_handle(&format!("timeline:{name}"), br.center());
                    let resp = ui.interact(br, ui.id().with(("feat", *id)), Sense::click_and_drag());
                    let res = app.session.model.result(*id);
                    let err = res.and_then(|r| r.error.clone());
                    let warn = res.and_then(|r| r.warning.clone());
                    let rolled = i >= marker;
                    let is_it = |s: &solidcraft_engine::Sel| matches!(s, solidcraft_engine::Sel::Feature { id: f } if f == id);
                    let sel = app.session.selection.iter().any(is_it) || app.dialog.as_ref().is_some_and(|d| d.items().iter().any(is_it));
                    if resp.hovered() {
                        hovered = Some(*id);
                    }
                    let bg = if err.is_some() {
                        t.timeline_item_error
                    } else if sel {
                        t.accent_soft
                    } else if resp.hovered() || from_view == Some(*id) {
                        t.hover
                    } else {
                        t.timeline_item
                    };
                    let edge = if err.is_some() {
                        t.error
                    } else if from_view == Some(*id) {
                        t.accent
                    } else {
                        t.border
                    };
                    p.rect(br, 3.0, bg, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
                    let ink = if rolled || *suppressed { t.border } else { t.icon };
                    icons::paint(
                        &p,
                        br.shrink(4.0),
                        icon,
                        ink,
                        if rolled { t.panel_header } else { t.icon_fill },
                        if rolled { t.border } else { t.accent },
                    );
                    if *suppressed {
                        p.line_segment([br.left_bottom() + vec2(4.0, -4.0), br.right_top() + vec2(-4.0, 4.0)], Stroke::new(1.5, t.text_dim));
                    }
                    if warn.is_some() && err.is_none() {
                        icons::paint(
                            &p,
                            Rect::from_min_size(br.right_top() - vec2(10.0, 0.0), vec2(10.0, 10.0)),
                            "warning",
                            t.icon,
                            t.warning,
                            t.warning,
                        );
                    }
                    let mut tip = format!("{name}  ({ty})");
                    if let Some(e) = &err {
                        tip += &format!("\nError: {e}");
                    }
                    if let Some(w) = &warn {
                        tip += &format!("\nWarning: {w}");
                    }
                    if let Some(r) = res {
                        tip += &format!("\n{:.1} ms", r.ms);
                    }
                    tip += "\nDouble-click to edit · drag to reorder · right-click for more";
                    if resp.drag_started() {
                        drag = Some(Drag::Feature(*id));
                    }
                    let resp = resp.on_hover_text(tip);
                    if resp.clicked() {
                        // A dialog input that takes features (pattern objects) gets the click.
                        let takes = app.dialog.as_ref().and_then(|d| d.active_input()).is_some_and(|i| i.accept & crate::selection::FEATURES != 0);
                        if takes {
                            if let Some(mut d) = app.dialog.take() {
                                d.pick(&app.session, solidcraft_engine::Sel::Feature { id: *id });
                                app.dialog = Some(d);
                            }
                        } else {
                            let _ = app.run("select.set", json!({"items": [{"type": "feature", "id": id}]}));
                        }
                    }
                    if resp.double_clicked() {
                        edit = Some(*id);
                    }
                    let sketch_of = match kind {
                        FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } | FeatureKind::Sweep { sketch, .. } => {
                            Some(*sketch)
                        }
                        _ => None,
                    };
                    resp.context_menu(|ui| {
                        if ui.button(crate::i18n::tr_label("Edit Feature")).clicked() {
                            edit = Some(*id);
                            ui.close();
                        }
                        if let Some(sk) = sketch_of
                            && ui.button(crate::i18n::tr_label("Edit Profile Sketch")).clicked()
                        {
                            app.edit_sketch(sk);
                            ui.close();
                        }
                        if ui.button(crate::i18n::tr_label("Rename")).clicked() {
                            app.dialog = Some(crate::dialogs::Dialog::rename(*id, name));
                            ui.close();
                        }
                        if ui.button(if *suppressed { "Unsuppress Features" } else { "Suppress Features" }).clicked() {
                            let _ = app.run("timeline.suppress", json!({"feature": id}));
                            ui.close();
                        }
                        if ui.button(crate::i18n::tr_label("Roll History Marker Here")).clicked() {
                            let _ = app.run("timeline.roll_to", json!({ "feature": id }));
                            ui.close();
                        }
                        if ui.button(crate::i18n::tr_label("Find in Browser")).clicked() {
                            find_in_browser(app, *id);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Delete").clicked() {
                            delete_feature(app, *id);
                            ui.close();
                        }
                    });
                    x += 32.0;
                }
                let end = x;
                if marker >= feats.len() {
                    marker_rect = Some(Rect::from_min_size(pos2(x, r.top() + 5.0), vec2(6.0, r.height() - 10.0)));
                }
                // The history marker: drag it to roll the model back or forward.
                if let Some(mr) = marker_rect {
                    let resp = ui.interact(mr.expand2(vec2(3.0, 0.0)), ui.id().with("tl_marker"), Sense::drag());
                    if resp.drag_started() {
                        drag = Some(Drag::Marker);
                    }
                    let col = if resp.hovered() || drag == Some(Drag::Marker) { t.accent } else { t.timeline_marker };
                    p.rect_filled(mr, 1.0, col);
                    resp.on_hover_text("History marker: drag to roll the model back; new features go here");
                }
                let content_end = crate::dialogs_assembly::timeline_joints(app, ui, &p, end + 10.0, r);
                // The scroll range follows the drawn content (the buttons don't take layout space themselves).
                ui.allocate_exact_size(vec2((content_end - origin).max(0.0), r.height()), Sense::hover());
                // Drag feedback, and the drop.
                if let (Some(dg), Some(pp)) = (drag.as_ref(), pointer) {
                    let slot = slot_at(&xs, end, pp.x);
                    let sx = xs.get(slot).copied().unwrap_or(end) - 4.0;
                    p.line_segment(
                        [pos2(sx, r.top() + 3.0), pos2(sx, r.bottom() - 3.0)],
                        Stroke::new(2.0, if matches!(dg, Drag::Marker) { t.accent } else { t.sketch_accent }),
                    );
                }
                let released = ui.input(|i| i.pointer.any_released());
                // (Taking the drag ends it even when the pointer has left the window.)
                if released
                    && let Some(dg) = drag.take()
                    && let Some(pp) = pointer
                {
                    let slot = slot_at(&xs, end, pp.x);
                    match dg {
                        Drag::Marker => {
                            let _ = app.run("timeline.roll_to", if slot >= n { json!({}) } else { json!({ "position": slot }) });
                        }
                        Drag::Feature(id) => {
                            let from = app.session.doc.feature_index(id).unwrap_or(0);
                            // Dropping after itself means the slot shifts by one.
                            let to = if slot > from { slot - 1 } else { slot };
                            if to != from {
                                let _ = app.run("timeline.reorder", json!({ "feature": id, "position": to }));
                            }
                        }
                    }
                }
                ui.data_mut(|d| match drag {
                    Some(dg) => {
                        d.insert_temp(drag_id, dg.clone());
                    }
                    None => d.remove::<Drag>(drag_id),
                });
                app.viewport.hover_feature = hovered;
                if let Some(id) = edit {
                    app.edit_feature(id);
                }
            });
        });
    });
}

/// Select what a feature made, so the browser shows it.
fn find_in_browser(app: &mut SolveApp, id: u64) {
    let st = app.session.world_state();
    let bodies: Vec<serde_json::Value> = st.bodies.iter().filter(|b| b.feature == id).map(|b| json!({"type": "body", "name": b.name})).collect();
    let items = if bodies.is_empty() { vec![json!({"type": "feature", "id": id})] } else { bodies };
    let _ = app.run("select.set", json!({ "items": items }));
}
