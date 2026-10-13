//! Booleans that failed, panicked or hung, replayed from the scripts in
//! `examples/regressions/` (each script's description says what went wrong).

use serde_json::{Value, json};

use crate::Session;

fn replay(script: &str) -> Session {
    let mut s = Session::default();
    let script: Value = serde_json::from_str(script).unwrap();
    s.run_script(&script).unwrap();
    s
}

fn volume(s: &Session, body: &str) -> f64 {
    s.model.state().body(body).map(|b| solidcraft_kernel::measure(&b.body).unwrap().volume).unwrap_or(0.0)
}

/// Slab less a 557.68 × 100 × 120 pocket and the loft below its floor (556.68 × 99 down 4 mm to
/// 80 × 80; prismatoid: h/6 · (A_top + A_bottom + 4 A_mid)).
fn slab_pocket_loft() -> (f64, f64) {
    let slab = 900.0 * 480.0 * 160.0;
    let loft = 4.0 / 6.0 * (556.68 * 99.0 + 80.0 * 80.0 + 4.0 * (556.68 + 80.0) / 2.0 * (99.0 + 80.0) / 2.0);
    (slab - 557.68 * 100.0 * 120.0, loft)
}

#[test]
fn loft_cut_beside_a_round_hole() {
    let s = replay(include_str!("../../../examples/regressions/loft_cut_beside_round_hole.json"));
    let (pocketed, loft) = slab_pocket_loft();
    let hole = std::f64::consts::PI * 17.5 * 17.5 * 25.0;
    let want = pocketed - loft - hole;
    let got = volume(&s, "Body1");
    assert!((got - want).abs() < 1e-6 * want, "{got} vs {want}");
}

#[test]
fn shallow_loft_cut_from_the_top_face() {
    let s = replay(include_str!("../../../examples/regressions/shallow_loft_cut_from_top_face.json"));
    let (_, loft) = slab_pocket_loft();
    let want = 900.0 * 480.0 * 160.0 - loft;
    let got = volume(&s, "Body1");
    assert!((got - want).abs() < 1e-9 * want, "{got} vs {want}");
}

#[test]
fn oblique_cut_through_a_small_arc_matches_a_split() {
    let scripts = [
        include_str!("../../../examples/regressions/oblique_cut_of_cylinder.json"),
        include_str!("../../../examples/regressions/oblique_cut_of_cylinder_hang.json"),
    ];
    // The same bar split by the prism's tilted top: the piece above it is the cut's result.
    let mut split = Session::default();
    let script: Value = serde_json::from_str(scripts[0]).unwrap();
    let steps = script.get("commands").and_then(Value::as_array).unwrap();
    split.run_script(&Value::Array(steps[..5].to_vec())).unwrap();
    let whole = volume(&split, "Tool");
    split.execute("solid.split_body", &json!({"body": "Tool", "plane": {"origin": [0, -472, 853.91], "normal": [0, 15.44, 40]}})).unwrap();
    let above = volume(&split, "Tool");
    assert!(above < whole - 10.0, "{above} of {whole}");
    for script in scripts {
        let s = replay(script);
        let got = volume(&s, "Tool");
        assert!((got - above).abs() < 1e-6 * above, "{got} vs {above}");
    }
}

/// Joining struts that end on a bore wall reached a truck-topology panic (#268). The guard caught it,
/// but no kernel call should panic at all (wasm aborts on a panic).
#[test]
fn struts_ending_on_a_bore_wall_join_without_a_panic() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let me = std::thread::current().id();
    let count = Arc::new(AtomicUsize::new(0));
    let c2 = count.clone();
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |_| {
        if std::thread::current().id() == me {
            c2.fetch_add(1, Ordering::SeqCst);
        }
    }));
    let mut s = Session::default();
    let script: Value = serde_json::from_str(include_str!("../../../examples/regressions/join_struts_on_bore_wall.json")).unwrap();
    let result = s.run_script(&script);
    std::panic::set_hook(prev);
    assert_eq!(count.load(Ordering::SeqCst), 0, "kernel panics during the join");
    result.unwrap();
    assert_eq!(s.model.state().bodies.len(), 1);
}

/// A blind Ø2 hole into the flat back of a long D-profile bar ends 2 mm short of the round front,
/// yet the cut was refused (#544). It removes π · 1² · 10 mm³ (within 5 %: the measure tessellates
/// the small bore) and adds the bore's wall and floor to the bar's six faces.
#[test]
#[ignore = "fails until #544 is fixed"]
fn blind_hole_into_a_long_d_profile_bar_stops_short_of_the_round_front() {
    let script: Value = serde_json::from_str(include_str!("../../../examples/regressions/blind_hole_in_long_d_profile.json")).unwrap();
    let steps = script.get("commands").and_then(Value::as_array).unwrap();
    let mut s = Session::default();
    s.run_script(&Value::Array(steps[..5].to_vec())).unwrap();
    let bar = volume(&s, "Slat");
    s.run_script(&Value::Array(steps[5..].to_vec())).unwrap();
    let hole = std::f64::consts::PI * 10.0;
    let removed = bar - volume(&s, "Slat");
    assert!((removed - hole).abs() < 0.05 * hole, "removed {removed} vs {hole}");
    let faces = s.model.state().body("Slat").map(|b| solidcraft_kernel::measure(&b.body).unwrap().merged.faces);
    assert_eq!(faces, Some(8));
}

/// A ring pocket cut from below a recess leaves a 0.5 mm floor joining the outer wall to the inner
/// block: one connected part, yet the cut left two bodies and lost volume (#545).
#[test]
#[ignore = "fails until #545 is fixed"]
fn ring_cut_below_a_recess_keeps_the_part_in_one_body() {
    let s = replay(include_str!("../../../examples/regressions/ring_cut_below_recess_thin_floor.json"));
    let bodies: Vec<(String, f64)> = s.model.state().bodies.iter().map(|b| (b.name.clone(), volume(&s, &b.name))).collect();
    let want = 100.0 * 100.0 * 30.0 - 88.0 * 88.0 * 12.0 - (95.0 * 95.0 - 68.0 * 68.0) * 17.5;
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    let got = volume(&s, "Frame");
    assert!((got - want).abs() < 1e-9 * want, "{got} vs {want}");
}
