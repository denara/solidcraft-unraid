//! A failed feature is marked in the bottom timeline and the Feature History by a shape, not only
//! its colour (#972): the error mark (a disc with a cross) where a warning's mark goes.

use egui::{Pos2, Rect, Shape, vec2};
use serde_json::json;

use crate::SolveApp;

/// Every shape in `s`, the parts of a list of shapes too.
fn each(s: &Shape, f: &mut impl FnMut(&Shape)) {
    match s {
        Shape::Vec(v) => v.iter().for_each(|s| each(s, f)),
        s => f(s),
    }
}

/// The sample with Fillet1 failing (the plate made narrower than its fillets).
fn failing_sample() -> SolveApp {
    let mut s = solidcraft_engine::Session::default();
    s.run_script(&solidcraft_engine::sample::script()).unwrap();
    s.execute("parameters.change", &json!({"name": "width", "expression": "5 mm"})).unwrap();
    SolveApp::new(s, crate::Services::default())
}

/// Each feature's name and whether it failed.
fn features(app: &SolveApp) -> Vec<(String, bool)> {
    app.session.doc.features.iter().map(|f| (f.name.clone(), app.session.model.result(f.id).is_some_and(|r| r.error.is_some()))).collect()
}

/// The discs filled in the error colour (either theme's) that one frame of `draw` paints.
fn error_discs(app: &mut SolveApp, draw: fn(&mut SolveApp, &mut egui::Ui)) -> Vec<Pos2> {
    crate::scenario::clear_handles();
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0));
    let out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| draw(app, ui));
    let errors = [crate::theme::Tokens::light().error, crate::theme::Tokens::dark().error];
    let mut found = Vec::new();
    for clipped in &out.shapes {
        each(&clipped.shape, &mut |s| {
            if let Shape::Circle(c) = s
                && errors.contains(&c.fill)
            {
                found.push(c.center);
            }
        });
    }
    found
}

/// The features that have a disc where their mark goes (`at`: the mark's centre from the item's).
fn marked(app: &mut SolveApp, draw: fn(&mut SolveApp, &mut egui::Ui), at: impl Fn(Pos2, Pos2) -> bool) -> Vec<(String, bool)> {
    let discs = error_discs(app, draw);
    let handles = crate::scenario::handles("timeline:");
    features(app)
        .into_iter()
        .map(|(name, _)| {
            let item = handles.iter().find(|(h, _)| *h == format!("timeline:{name}")).map(|(_, p)| *p);
            let item = item.unwrap_or_else(|| panic!("{name} is not drawn"));
            let has = discs.iter().any(|d| at(item, *d));
            (name, has)
        })
        .collect()
}

#[test]
fn a_failed_feature_is_marked_in_the_bottom_timeline() {
    let mut app = failing_sample();
    let want = features(&app);
    assert!(want.iter().any(|(n, failed)| *failed && n == "Fillet1"), "the sample's Fillet1 fails: {want:?}");
    assert!(want.iter().filter(|(_, failed)| !failed).count() >= 3, "and others don't");
    // The mark sits in the item's top-right corner: 10 points square, inside the 28-point item.
    let got = marked(&mut app, crate::timeline::timeline, |item, d| (d.x - (item.x + 9.0)).abs() < 1.5 && (d.y - (item.y - 9.0)).abs() < 1.5);
    assert_eq!(got, want, "an error mark on each failed feature and on no other");
}

#[test]
fn a_failed_feature_is_marked_in_the_feature_history() {
    let mut app = failing_sample();
    let want = features(&app);
    // The mark sits at the row's right end, on its centre line.
    let got = marked(&mut app, crate::timeline_left::left_timeline, |item, d| (d.y - item.y).abs() < 1.5 && d.x > item.x);
    assert_eq!(got, want, "an error mark on each failed feature and on no other");
}

/// The mark is its own drawing: a disc with a cross in the caller's colours, not the fallback
/// square, and a different shape from the warning's triangle.
#[test]
fn the_error_mark_is_a_disc_with_a_cross() {
    let ctx = egui::Context::default();
    let (ink, fill) = (egui::Color32::from_rgb(1, 2, 3), egui::Color32::from_rgb(4, 5, 6));
    let draw = |name: &'static str| {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::icons::paint(ui.painter(), Rect::from_min_size(Pos2::ZERO, vec2(24.0, 24.0)), name, ink, fill, fill);
        });
        let mut shapes = Vec::new();
        for c in &out.shapes {
            each(&c.shape, &mut |s| shapes.push(s.clone()));
        }
        shapes
    };
    let error = draw("error");
    let discs = error.iter().filter(|s| matches!(s, Shape::Circle(c) if c.fill == fill && c.stroke.color == ink)).count();
    let strokes = error.iter().filter(|s| matches!(s, Shape::Path(p) if !p.closed && p.stroke.color == egui::epaint::ColorMode::Solid(ink))).count();
    assert_eq!((discs, strokes), (1, 2), "one disc and the two strokes of a cross: {error:?}");
    assert_ne!(format!("{error:?}"), format!("{:?}", draw("warning")));
    assert_ne!(format!("{error:?}"), format!("{:?}", draw("no such icon")), "drawn, not the fallback square");
}
