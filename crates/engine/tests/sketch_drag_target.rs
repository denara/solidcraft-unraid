//! Cursor distance is minimized within the design's freedom; lengths stay mandatory.
#[path = "support/four_bar.rs"]
mod four_bar;

use serde_json::json;
use solidcraft_geom::Vec2;

fn drag(s: &mut solidcraft_engine::Session, target: Vec2) {
    let from = four_bar::point(s, "crank.end").unwrap();
    s.execute("sketch.drag", &json!({"entity":"crank.end","from":[from.x,from.y],"to":[target.x,target.y],"live":true})).unwrap();
}

#[test]
fn four_bar_large_off_locus_drag_reaches_the_closest_feasible_point() {
    for target in [Vec2::new(-10.0, 8.0), Vec2::new(-10.0, -8.0), Vec2::new(20.0, 20.0), Vec2::new(20.0, 0.0), Vec2::new(1000.0, -1000.0)] {
        let mut s = four_bar::session().unwrap();
        let side = four_bar::branch(&s).unwrap();
        drag(&mut s, target);
        let actual = four_bar::point(&s, "crank.end").unwrap();
        let nearest = target * (10.0 / target.len());
        assert!(actual.dist(nearest) < 1e-4, "target {target:?}, actual {actual:?}, expected {nearest:?}");
        assert!(four_bar::length_error(&s).unwrap() < 1e-6);
        assert_eq!(four_bar::branch(&s).unwrap(), side);
        let state = s.model.state();
        let report = &state.sketch(s.active_sketch.unwrap()).unwrap().report;
        assert!(report.ok());
        assert_eq!(report.dof, 1);
        s.execute("sketch.drag_end", &json!({})).unwrap();
        assert!(four_bar::point(&s, "crank.end").unwrap().dist(nearest) < 1e-4);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(four_bar::point(&s, "crank.end").unwrap().dist(Vec2::new(-10.0, 0.0)) < 1e-7);
    }
}

#[test]
fn four_bar_reverse_motion_and_stationary_cursor_do_not_drift() {
    let mut s = four_bar::session().unwrap();
    let side = four_bar::branch(&s).unwrap();
    for target in [Vec2::new(-10.0, 16.0), Vec2::new(-10.0, -16.0), Vec2::new(0.0, 25.0), Vec2::new(0.0, -25.0)] {
        drag(&mut s, target);
        let at = four_bar::point(&s, "crank.end").unwrap();
        assert!(
            at.dist(target * (10.0 / target.len())) < 1e-4,
            "target {target:?}, actual {at:?}, length error {}",
            four_bar::length_error(&s).unwrap()
        );
        for _ in 0..8 {
            drag(&mut s, target);
            assert!(four_bar::point(&s, "crank.end").unwrap().dist(at) < 1e-4);
            assert!(four_bar::length_error(&s).unwrap() < 1e-6);
            assert_eq!(four_bar::branch(&s).unwrap(), side);
        }
    }
}

#[test]
fn four_bar_one_event_and_split_moves_agree_and_keep_the_branch() {
    let target = Vec2::new(-10.0, 16.0);
    let mut single = four_bar::session().unwrap();
    let mut split = four_bar::session().unwrap();
    let start = four_bar::point(&single, "crank.end").unwrap();
    let side = four_bar::branch(&single).unwrap();
    drag(&mut single, target);
    for i in 1..=16 {
        drag(&mut split, start + (target - start) * (i as f64 / 16.0));
        assert!(four_bar::length_error(&split).unwrap() < 1e-6);
        assert_eq!(four_bar::branch(&split).unwrap(), side);
    }
    assert!(four_bar::point(&single, "crank.end").unwrap().dist(four_bar::point(&split, "crank.end").unwrap()) < 1e-4);
}

#[test]
fn four_bar_live_cursor_motion_updates_driven_dimensions_before_release() {
    let mut s = four_bar::session().unwrap();
    s.execute("sketch.dimension", &json!({"entities": ["crank"], "type": "horizontal", "driven": true})).unwrap();
    let id = s.active_sketch.unwrap();
    let dimension = s.doc.sketch(id).unwrap().constraints.last().unwrap().id.clone();
    drag(&mut s, Vec2::new(-10.0, 8.0));
    let at = four_bar::point(&s, "crank.end").unwrap();
    assert!(at.dist(Vec2::new(-10.0, 8.0) * (10.0 / 164.0_f64.sqrt())) < 1e-4);
    let state = s.model.state();
    for sketch in [s.doc.sketch(id).unwrap(), &state.sketch(id).unwrap().sketch] {
        let c = sketch.constraints.iter().find(|c| c.id == dimension).unwrap();
        assert!(c.driven);
        assert!((c.kind.value().unwrap() - at.x.abs()).abs() < 1e-6, "reference dimension must measure the final live pose: {:?}", c.kind);
    }
}

#[test]
fn four_bar_other_joint_follows_the_rocker_arc_and_its_limits() {
    let base = Vec2::new(42.0, 0.0);
    let lo = -(-1264.0_f64 / 1680.0).acos();
    let hi = -(0.2_f64).acos();
    for angle in [-2.6_f64, -2.1, -1.5, -1.1] {
        let mut s = four_bar::session().unwrap();
        let side = four_bar::branch(&s).unwrap();
        let from = four_bar::point(&s, "coupler.end").unwrap();
        let target = base + Vec2::new(angle.cos(), angle.sin()) * 60.0;
        let nearest = base + Vec2::new(angle.clamp(lo, hi).cos(), angle.clamp(lo, hi).sin()) * 20.0;
        s.execute("sketch.drag", &json!({"entity":"coupler.end","from":[from.x,from.y],"to":[target.x,target.y],"live":true})).unwrap();
        assert!(four_bar::point(&s, "coupler.end").unwrap().dist(nearest) < 1e-4, "angle {angle}");
        assert!(four_bar::length_error(&s).unwrap() < 1e-6);
        assert_eq!(four_bar::branch(&s).unwrap(), side);
        s.execute("sketch.drag_end", &json!({})).unwrap();
        assert!(four_bar::point(&s, "coupler.end").unwrap().dist(nearest) < 1e-4);
    }
}

fn grab(s: &solidcraft_engine::Session, line: &str, fraction: f64) -> Vec2 {
    let a = four_bar::point(s, &format!("{line}.start")).unwrap();
    let b = four_bar::point(s, &format!("{line}.end")).unwrap();
    a + (b - a) * fraction
}

#[test]
fn a_free_line_grabbed_slightly_off_its_edge_still_translates_rigidly() {
    for from in [Vec2::new(15.0, 12.0), Vec2::new(32.0, 11.0)] {
        let mut s = solidcraft_engine::Session::default();
        s.execute("sketch.create", &json!({"plane":"XZ"})).unwrap();
        s.execute("sketch.line", &json!({"points":[[10,10],[30,10]],"ids":["free"],"infer":false})).unwrap();
        let to = from + Vec2::new(3.0, 5.0);
        let r = s.execute("sketch.drag", &json!({"entity":"free","from":[from.x,from.y],"to":[to.x,to.y],"live":true})).unwrap();
        assert!(four_bar::point(&s, "free.start").unwrap().dist(Vec2::new(13.0, 15.0)) < 1e-6);
        assert!(four_bar::point(&s, "free.end").unwrap().dist(Vec2::new(33.0, 15.0)) < 1e-6);
        let at = Vec2::new(r["at"][0].as_f64().unwrap(), r["at"][1].as_f64().unwrap());
        assert!(at.dist(to) < 1e-6);
    }
}

#[test]
fn an_offset_edge_grab_rotates_with_the_link_and_does_not_drift() {
    for local in [Vec2::new(0.7, 0.1), Vec2::new(0.7, -0.1)] {
        let mut s = four_bar::session().unwrap();
        let target = Vec2::new(-7.0, 11.2);
        let closest = target * (10.0 * local.len() / target.len());
        let attached = |s: &solidcraft_engine::Session| {
            let a = four_bar::point(s, "crank.start").unwrap();
            let d = four_bar::point(s, "crank.end").unwrap() - a;
            a + d * local.x + Vec2::new(-d.y, d.x) * local.y
        };
        let mut from = attached(&s);
        for _ in 0..8 {
            let r = s.execute("sketch.drag", &json!({"entity":"crank","from":[from.x,from.y],"to":[target.x,target.y],"live":true})).unwrap();
            from = Vec2::new(r["at"][0].as_f64().unwrap(), r["at"][1].as_f64().unwrap());
            assert!(from.dist(attached(&s)) < 1e-6);
            assert!(attached(&s).dist(closest) < 1e-4);
            assert!(four_bar::length_error(&s).unwrap() < 1e-6);
            assert_eq!(four_bar::branch(&s).unwrap(), -1.0);
        }
    }
}

#[test]
fn crank_edge_large_drag_reaches_the_nearest_material_point() {
    for fraction in [0.2, 0.5, 0.8] {
        let mut s = four_bar::session().unwrap();
        let side = four_bar::branch(&s).unwrap();
        let from = grab(&s, "crank", fraction);
        let target = Vec2::new(-10.0, 16.0) * fraction;
        let nearest = target * (10.0 * fraction / target.len());
        let result = s.execute("sketch.drag", &json!({"entity":"crank", "from":[from.x,from.y], "to":[target.x,target.y], "live":true})).unwrap();
        let actual = grab(&s, "crank", fraction);
        assert!(four_bar::length_error(&s).unwrap() < 1e-6);
        assert_eq!(four_bar::branch(&s).unwrap(), side);
        assert!(actual.dist(nearest) < 1e-4, "fraction {fraction}: {actual:?}, expected {nearest:?}");
        let at = Vec2::new(result["at"][0].as_f64().unwrap(), result["at"][1].as_f64().unwrap());
        assert!(at.dist(actual) < 1e-6, "reported grab {at:?}, actual {actual:?}");
        s.execute("sketch.drag_end", &json!({})).unwrap();
        assert!(grab(&s, "crank", fraction).dist(nearest) < 1e-4);
    }
}

#[test]
fn rocker_edge_keeps_its_material_point_and_motion_limits() {
    let base = Vec2::new(42.0, 0.0);
    let lo = -(-1264.0_f64 / 1680.0).acos();
    let hi = -(0.2_f64).acos();
    for fraction in [0.2, 0.5, 0.8] {
        for angle in [-2.6_f64, -2.1, -1.5, -1.1] {
            let mut s = four_bar::session().unwrap();
            let from = grab(&s, "rocker", fraction);
            let target = base + Vec2::new(angle.cos(), angle.sin()) * (60.0 * (1.0 - fraction));
            let closest = base + Vec2::new(angle.clamp(lo, hi).cos(), angle.clamp(lo, hi).sin()) * (20.0 * (1.0 - fraction));
            s.execute("sketch.drag", &json!({"entity":"rocker","from":[from.x,from.y],"to":[target.x,target.y],"live":true})).unwrap();
            assert!(
                grab(&s, "rocker", fraction).dist(closest) < 1e-4,
                "fraction {fraction}, angle {angle}, actual {:?}, expected {closest:?}",
                grab(&s, "rocker", fraction)
            );
            assert!(four_bar::length_error(&s).unwrap() < 1e-6);
            assert_eq!(four_bar::branch(&s).unwrap(), -1.0);
        }
    }
}

#[test]
fn an_off_centre_edge_grab_agrees_for_one_move_split_moves_and_holding_still() {
    for fraction in [0.2, 0.5, 0.8] {
        for frames in [1, 16] {
            let mut s = four_bar::session().unwrap();
            let start = grab(&s, "crank", fraction);
            let target = Vec2::new(-10.0, 16.0) * fraction;
            let mut from = start;
            for i in 1..=frames + 8 {
                let to = start + (target - start) * (i.min(frames) as f64 / frames as f64);
                let result = s.execute("sketch.drag", &json!({"entity":"crank","from":[from.x,from.y],"to":[to.x,to.y],"live":true})).unwrap();
                from = Vec2::new(result["at"][0].as_f64().unwrap(), result["at"][1].as_f64().unwrap());
                assert!(from.dist(grab(&s, "crank", fraction)) < 1e-6);
                assert!(four_bar::length_error(&s).unwrap() < 1e-6);
            }
            let closest = target * (10.0 * fraction / target.len());
            assert!(grab(&s, "crank", fraction).dist(closest) < 1e-4);
        }
    }
}

fn coupler_point(angle: f64, along: f64) -> Vec2 {
    let a = Vec2::new(angle.cos(), angle.sin()) * 10.0;
    let d = Vec2::new(42.0, 0.0) - a;
    let len = d.len();
    let u = d * (1.0 / len);
    let x = (40.0 * 40.0 - 20.0 * 20.0 + len * len) / (2.0 * len);
    let c = a + u * x + Vec2::new(u.y, -u.x) * (1600.0 - x * x).sqrt();
    a + (c - a) * along
}

fn closest_coupler_point(target: Vec2, along: f64) -> Vec2 {
    // Independent one-dimensional oracle: sample the full crank revolution, then refine
    // the best interval. Both circle intersections keep the reported linkage's branch.
    let step = std::f64::consts::TAU / 4096.0;
    let best = (0..4096)
        .min_by(|a, b| coupler_point(*a as f64 * step, along).dist(target).total_cmp(&coupler_point(*b as f64 * step, along).dist(target)))
        .unwrap();
    let (mut lo, mut hi) = ((best as f64 - 1.0) * step, (best as f64 + 1.0) * step);
    for _ in 0..80 {
        let a = (2.0 * lo + hi) / 3.0;
        let b = (lo + 2.0 * hi) / 3.0;
        if coupler_point(a, along).dist(target) < coupler_point(b, along).dist(target) {
            hi = b;
        } else {
            lo = a;
        }
    }
    coupler_point((lo + hi) * 0.5, along)
}

#[test]
fn the_coupler_edge_follows_with_both_ends_free_to_rotate() {
    for along in [0.2, 0.5, 0.8] {
        for angle in [1.8, 2.3, 3.8] {
            let mut s = four_bar::session().unwrap();
            let from = grab(&s, "coupler", along);
            let target = coupler_point(angle, along) + Vec2::new(-3.0, 4.0);
            let closest = closest_coupler_point(target, along);
            let result = s.execute("sketch.drag", &json!({"entity":"coupler","from":[from.x,from.y],"to":[target.x,target.y],"live":true})).unwrap();
            let actual = grab(&s, "coupler", along);
            assert!(actual.dist(closest) < 1e-4, "along {along}, angle {angle}, actual {actual:?}, closest {closest:?}");
            let at = Vec2::new(result["at"][0].as_f64().unwrap(), result["at"][1].as_f64().unwrap());
            assert!(at.dist(actual) < 1e-6);
            assert!(four_bar::length_error(&s).unwrap() < 1e-6);
            assert_eq!(four_bar::branch(&s).unwrap(), -1.0);
        }
    }
}
