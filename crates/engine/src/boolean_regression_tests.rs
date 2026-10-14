//! Booleans that failed, panicked or hung, replayed from the scripts in
//! `examples/regressions/` (each script's description says what went wrong).

use serde_json::{Value, json};

use crate::Session;

/// Runs in PR CI as well as the heavier fixed-seed suite. Two pattern copies are
/// disconnected, and one cylinder overlaps the block by half its volume.
#[test]
fn partial_face_contact_pattern_has_analytic_material() {
    // Repeated construction exposes re-imprinting a cylinder through both of its
    // side faces, which can create a redundant arc along the first imprint.
    for _ in 0..8 {
        check_partial_face_contact_pattern();
    }
}

fn check_partial_face_contact_pattern() {
    let s = replay(include_str!("../../../examples/regressions/partial_face_contact_pattern.json"));
    let (mut volume, mut area, mut shells) = (0.0, 0.0, 0);
    for b in &s.model.state().bodies {
        assert!(b.body.validity().is_empty(), "{:?}", b.body.validity());
        let m = b.body.tessellate(1e-5).unwrap().measure();
        volume += m.volume;
        area += m.area;
        shells += b.body.shell_count();
    }
    let expected_volume = 25.0 * 47.5 * 37.5 + 3.5 * std::f64::consts::PI * 25.0 * 2.5;
    let expected_area = 2.0 * (25.0 * 47.5 + 25.0 * 37.5 + 47.5 * 37.5) + 237.5 * std::f64::consts::PI - 25.0;
    assert!((volume - expected_volume).abs() < 0.02, "{volume} expected {expected_volume}");
    assert!((area - expected_area).abs() < 0.02, "{area} expected {expected_area}");
    assert_eq!(shells, 3);
    println!("#674 volume {volume:.6} / {expected_volume:.6} mm³; area {area:.6} / {expected_area:.6} mm²; {shells} shells");
}

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
fn ring_cut_below_a_recess_keeps_the_part_in_one_body() {
    let s = replay(include_str!("../../../examples/regressions/ring_cut_below_recess_thin_floor.json"));
    let bodies: Vec<(String, f64)> = s.model.state().bodies.iter().map(|b| (b.name.clone(), volume(&s, &b.name))).collect();
    let want = 100.0 * 100.0 * 30.0 - 88.0 * 88.0 * 12.0 - (95.0 * 95.0 - 68.0 * 68.0) * 17.5;
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    let got = volume(&s, "Frame");
    assert!((got - want).abs() < 1e-9 * want, "{got} vs {want}");
}

/// Four ribs joined on a rounded lid, standing on the plate over the skirt and flush with the
/// window's sides: each adds exactly its own material and nothing changes away from it. The
/// joins displaced the tool and one left a slit through the far side of the skirt and plate
/// (#616), which no volume shows: the area does. The reply of each join carries its warning, as
/// the timeline does (an agent placing parts over MCP saw none).
#[test]
fn ribs_flush_with_a_window_change_nothing_away_from_them() {
    let script: Value = serde_json::from_str(include_str!("../../../examples/regressions/lid_ribs_flush_with_window.json")).unwrap();
    let steps = script.get("commands").and_then(Value::as_array).unwrap();
    let (lid, ribs) = steps.split_at(steps.len() - 4);
    let mut s = Session::default();
    s.run_script(&Value::Array(lid.to_vec())).unwrap();
    let measure = |s: &Session| s.model.state().body("Lid").map(|b| solidcraft_kernel::measure(&b.body).unwrap()).unwrap();
    let (v0, a0) = (measure(&s).volume, measure(&s).area);
    assert!((v0 - 6703.5222).abs() < 1e-3, "{v0}");
    for (i, rib) in ribs.iter().enumerate() {
        let reply = s.execute(rib["command"].as_str().unwrap(), &rib["params"]).unwrap();
        let id = reply["feature"].as_u64().unwrap();
        let result = s.model.result(id).unwrap();
        assert_eq!(reply["warning"], json!(result.warning), "the reply says what the timeline says");
        assert!(!result.warning.as_deref().unwrap_or_default().contains("tool displacement"), "{:?}", result.warning);
        let m = measure(&s);
        let n = (i + 1) as f64;
        // 1.6 x 6 x 11.7 less the 1.35 x 6 x 3 inside the skirt.
        assert!((m.volume - (v0 + n * 88.02)).abs() < 1e-6, "rib {}: {}", i + 1, m.volume - v0);
        // Top 9.6, outer side 6 x 8.7 above the skirt, inner side 6 x 11.7 (flush with the
        // window's side), ends 2 x (1.6 x 11.7 - 1.35 x 3); the skirt top 8.1, its inner side
        // 18 and 1.5 of the plate go under the rib.
        let area = 9.6 + 52.2 + 70.2 + 29.34 - 8.1 - 18.0 - 1.5;
        assert!((m.area - (a0 + n * area)).abs() < 1e-6, "rib {}: area {} (no slit adds any)", i + 1, m.area - a0);
    }
    let bounds = s.model.state().body("Lid").unwrap().body.tessellate(0.01).unwrap().bounds();
    assert!((bounds.max.z - 14.1).abs() < 1e-9, "{}", bounds.max.z);
}

/// #679: one sketch of a 2 × 2 grid of 10 mm squares; three cells extruded up 10 mm (XO/XX) and
/// three down 10 mm, joined (XX/OX). The two L's meet back to back over two whole cells, along
/// an edge and at the centre: one solid of 6 cells × 10 mm × 100 mm².
#[test]
fn ls_extruded_up_and_down_from_one_grid_join() {
    let mut s = Session::default();
    let run = |s: &mut Session, id: &str, p: Value| s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Grid"}));
    for (p0, p1) in [([-10, -10], [0, 0]), ([0, -10], [10, 0]), ([-10, 0], [0, 10]), ([0, 0], [10, 10])] {
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": p0, "p1": p1}));
    }
    run(&mut s, "sketch.finish", json!({}));
    let cells = |pts: &[[f64; 2]]| Value::Array(pts.iter().map(|p| json!({"point": p})).collect());
    run(
        &mut s,
        "solid.extrude",
        json!({"sketch": "Grid", "profiles": cells(&[[-5., 5.], [-5., -5.], [5., -5.]]), "distance": 10, "body_name": "Logo"}),
    );
    let reply = run(
        &mut s,
        "solid.extrude",
        json!({"sketch": "Grid", "profiles": cells(&[[-5., 5.], [5., 5.], [5., -5.]]), "distance": "10 mm", "direction": "negative", "operation": "join"}),
    );
    assert!(reply["warning"].as_str().is_none_or(|w| !w.contains("tool displacement")), "{reply}");
    let st = s.model.state();
    assert_eq!(st.bodies.len(), 1);
    let body = &st.body("Logo").unwrap().body;
    assert_eq!(body.shell_count(), 1);
    assert!(body.validity().is_empty(), "{:?}", body.validity());
    let v = volume(&s, "Logo");
    assert!((v - 6000.0).abs() < 1e-6, "{v}");
}

/// This process's CPU time (all threads) and peak resident memory, where the system says
/// (Linux: /proc/self); `None` elsewhere.
fn cpu_and_peak_memory() -> Option<(std::time::Duration, u64)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // utime and stime, in clock ticks (100 a second on Linux), after the command's name.
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let ticks: u64 = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: u64 = status.lines().find_map(|l| l.strip_prefix("VmHWM:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some((std::time::Duration::from_millis(ticks * 10), kb * 1024))
}

/// Press/pull on the sensor enclosure's lid top: joining the pulled face's prism imprints the
/// contact (#825), and boxing the lid's faces by dividing their curved edges down to the
/// contact tolerance ran away (over 24 GB and many minutes). It takes about 9 s of CPU here and
/// no memory beyond the design's. The budgets are work, not wall time, which a loaded runner
/// stretches (61-87 s for the same work on CI).
#[test]
fn press_pull_on_the_enclosure_lid_is_prompt() {
    let mut s = Session::default();
    let script: Value = serde_json::from_str(include_str!("../../../examples/acceptance/sensor_enclosure.json")).unwrap();
    s.run_script(&script).unwrap();
    let total = |s: &Session| -> f64 { s.model.state().bodies.iter().map(|b| solidcraft_kernel::measure(&b.body).unwrap().volume).sum() };
    let before = total(&s);
    let start = cpu_and_peak_memory();
    s.execute("solid.press_pull", &json!({"face": [-12.0, 27.4, 23.0], "distance": 5.0})).unwrap();
    if let (Some((cpu0, peak0)), Some((cpu1, peak1))) = (start, cpu_and_peak_memory()) {
        let grew = peak1.saturating_sub(peak0);
        assert!(grew < 1 << 30, "the press/pull grew the peak memory by {} MB", grew >> 20);
        // Other tests in this process (cargo test) count too, so the bound is generous.
        assert!(cpu1 - cpu0 < std::time::Duration::from_secs(300), "the press/pull took {:?} of CPU", cpu1 - cpu0);
    }
    assert!(total(&s) > before, "{before} -> {}", total(&s));
}
