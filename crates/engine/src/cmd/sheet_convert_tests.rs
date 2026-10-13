//! Convert to Sheet Metal with bend edges: solid and shelled bodies.

use std::f64::consts::{FRAC_PI_2, PI, SQRT_2};

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn volume(s: &mut Session) -> f64 {
    run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN)
}

fn bbox(s: &mut Session) -> ([f64; 3], [f64; 3]) {
    let m = run(s, "inspect.measure", json!({}));
    let b = &m["bodies"][0]["bbox"];
    let v = |k: &str| [0, 1, 2].map(|i| b[k][i].as_f64().unwrap_or(f64::NAN));
    (v("min"), v("max"))
}

fn flat(s: &mut Session) -> Value {
    run(s, "sheet.flat_pattern", json!({}))
}

fn flat_size(s: &mut Session) -> (f64, f64) {
    let f = flat(s);
    let v = |i: usize| f["flat_size_mm"][i].as_f64().unwrap_or(f64::NAN);
    (v(0), v(1))
}

/// Either way round (the flat frame's x runs along the fixed face's longest edge).
fn same_size(got: (f64, f64), want: (f64, f64), tol: f64) -> bool {
    ((got.0 - want.0).abs() < tol && (got.1 - want.1).abs() < tol) || ((got.0 - want.1).abs() < tol && (got.1 - want.0).abs() < tol)
}

/// The bend table's rows as (angle in whole degrees, direction, length).
fn bend_rows(s: &mut Session) -> Vec<(i64, String, f64)> {
    let rows = flat(s)["bends"].as_array().cloned().unwrap_or_default();
    let row = |b: &Value| {
        (
            b["angle_deg"].as_f64().unwrap_or(0.0).round() as i64,
            b["direction"].as_str().unwrap_or("").to_string(),
            b["length"].as_f64().unwrap_or(0.0),
        )
    };
    rows.iter().map(row).collect()
}

fn rule(s: &mut Session, name: &str, t: f64, k: f64, r: f64, gap: f64) {
    let mm = |v: f64| format!("{v} mm");
    run(
        s,
        "sheet.manage_rules",
        json!({"name": name, "thickness": mm(t), "k_factor": format!("{k}"), "bend_radius": mm(r), "gap": mm(gap), "active": true}),
    );
}

/// Bend deduction: twice the outside setback less the allowance.
fn deduction(theta: f64, r: f64, t: f64, k: f64) -> f64 {
    2.0 * (r + t) * (theta / 2.0).tan() - theta * (r + k * t)
}

const BOTTOM_EDGES: [[f64; 3]; 4] = [[50.0, 0.0, 0.0], [100.0, 30.0, 0.0], [50.0, 60.0, 0.0], [0.0, 30.0, 0.0]];

/// A 100 x 60 x 30 box shelled to `t` with its top open.
fn tray(s: &mut Session, t: f64) {
    run(s, "solid.box", json!({"length": 100, "width": 60, "height": 30, "body_name": "Tray"}));
    run(s, "solid.shell", json!({"faces": [[50, 30, 30]], "thickness": t}));
}

/// The folded tray's volume: base, four 90° bends and four walls cut back at the corners.
fn tray_volume(l: f64, w: f64, h: f64, t: f64, r: f64, gap: f64) -> f64 {
    let s = r + t;
    let cut = t + gap / SQRT_2;
    let base = (l - 2.0 * s) * (w - 2.0 * s);
    let bends = FRAC_PI_2 * (r + t / 2.0) * 2.0 * ((l - 2.0 * s) + (w - 2.0 * s));
    let walls = (h - s) * 2.0 * ((l - 2.0 * cut) + (w - 2.0 * cut));
    t * (base + bends + walls)
}

/// A strip `t` thick along the polyline `outer` (its material to the left of the way the line
/// runs, its ends cut square), extruded `len` along z as the body `Section`. Returns a point on
/// the edge at each corner of the polyline.
fn section(s: &mut Session, outer: &[(f64, f64)], t: f64, len: f64) -> Vec<[f64; 3]> {
    let n = outer.len();
    let dir = |i: usize| {
        let (a, b) = (outer[i], outer[i + 1]);
        let l = (b.0 - a.0).hypot(b.1 - a.1);
        ((b.0 - a.0) / l, (b.1 - a.1) / l)
    };
    let shift = |p: (f64, f64), d: (f64, f64)| (p.0 - d.1 * t, p.1 + d.0 * t);
    let mut inner = vec![shift(outer[0], dir(0))];
    for (i, &corner) in outer.iter().enumerate().take(n - 1).skip(1) {
        let (d0, d1) = (dir(i - 1), dir(i));
        let (p0, p1) = (shift(corner, d0), shift(corner, d1));
        let along = ((p1.0 - p0.0) * d1.1 - (p1.1 - p0.1) * d1.0) / (d0.0 * d1.1 - d0.1 * d1.0);
        inner.push((p0.0 + d0.0 * along, p0.1 + d0.1 * along));
    }
    inner.push(shift(outer[n - 1], dir(n - 2)));
    let pts: Vec<[f64; 2]> = outer.iter().chain(inner.iter().rev()).map(|p| [p.0, p.1]).collect();
    run(s, "sketch.create", json!({"plane": "XY", "name": "Section"}));
    run(s, "sketch.line", json!({"points": pts, "closed": true}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"sketch": "Section", "distance": len, "body_name": "Section"}));
    outer[1..n - 1].iter().map(|p| [p.0, p.1, len / 2.0]).collect()
}

/// A base 200 wide with walls 40 long at 72° and returns 15 long at 108° more, 300 long.
fn trapezoid_section(s: &mut Session, t: f64) -> Vec<[f64; 3]> {
    let (c, sn) = (72f64.to_radians().cos(), 72f64.to_radians().sin());
    let (x, y) = (100.0 + 40.0 * c, 40.0 * sn);
    section(s, &[(-x + 15.0, y), (-x, y), (-100.0, 0.0), (100.0, 0.0), (x, y), (x - 15.0, y)], t, 300.0)
}

#[test]
fn a_shelled_tray_keeps_its_walls_and_unfolds_by_the_k_factor() {
    let (t, k, r, gap) = (2.0, 0.44, 2.0, 0.5);
    let mut s = Session::default();
    // The shell's own thickness counts, not the rule's.
    rule(&mut s, "Test", 3.0, k, r, gap);
    tray(&mut s, t);
    let out = run(&mut s, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES}));
    assert!(out["warning"].is_null(), "{out}");
    assert!((flat(&mut s)["thickness"].as_f64().unwrap_or(0.0) - t).abs() < 1e-9);
    // Four 90° bends up, each as long as the base is between its setbacks (R + T at either end).
    let mut rows = bend_rows(&mut s);
    rows.sort_by(|a, b| a.2.total_cmp(&b.2));
    let want = [52.0, 52.0, 92.0, 92.0];
    assert!(rows.len() == 4 && rows.iter().zip(want).all(|(row, l)| row.0 == 90 && row.1 == "up" && (row.2 - l).abs() < 1e-6), "{rows:?}");
    let allowance = flat(&mut s)["bends"][0]["allowance"].as_f64().unwrap_or(0.0);
    assert!((allowance - FRAC_PI_2 * (r + k * t)).abs() < 1e-9, "{allowance}");
    // The envelope stays: the sharp edges became bends inside it.
    let (lo, hi) = bbox(&mut s);
    assert!(
        lo.iter().all(|v| v.abs() < 1e-6) && (hi[0] - 100.0).abs() < 1e-6 && (hi[1] - 60.0).abs() < 1e-6 && (hi[2] - 30.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // Flat: the outside lengths less two bend deductions each way.
    let bd = deduction(FRAC_PI_2, r, t, k);
    let want = (100.0 + 60.0 - 2.0 * bd, 60.0 + 60.0 - 2.0 * bd);
    assert!(same_size(flat_size(&mut s), want, 1e-6), "{:?} vs {want:?}", flat_size(&mut s));
    let v = volume(&mut s);
    let expect = tray_volume(100.0, 60.0, 30.0, t, r, gap);
    assert!((v - expect).abs() < 2e-3 * expect, "{v} vs {expect}");
    // Unfold: flat, one sheet thick, the same outline; refold gives the tray back.
    run(&mut s, "sheet.unfold", json!({}));
    let (lo, hi) = bbox(&mut s);
    assert!((hi[2] - lo[2] - t).abs() < 1e-6, "{lo:?} {hi:?}");
    assert!(same_size((hi[0] - lo[0], hi[1] - lo[1]), want, 1e-6), "{lo:?} {hi:?}");
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - v).abs() < 1e-6 * v);
    // It exports like a sheet made from flanges: an outline and four bend lines up.
    let dxf = run(&mut s, "sheet.export_dxf", json!({}));
    assert_eq!(dxf["bends"], 4);
    let lines: Vec<&str> = dxf["dxf"].as_str().unwrap_or_default().lines().collect();
    assert_eq!(lines.windows(2).filter(|w| w[0].trim() == "8" && w[1].trim() == "BEND_UP").count(), 4, "four bend lines");
    assert_eq!(run(&mut s, "sheet.export_bend_table", json!({"format": "json"}))["bends"], 4);
}

#[test]
fn bends_picked_inside_and_a_fixed_face_inside_give_the_same_sheet() {
    let (t, k, r, gap) = (2.0, 0.44, 2.0, 0.5);
    let inner = [[50.0, 2.0, 2.0], [98.0, 30.0, 2.0], [50.0, 58.0, 2.0], [2.0, 30.0, 2.0]];
    let mut s = Session::default();
    rule(&mut s, "Test", t, k, r, gap);
    tray(&mut s, t);
    run(&mut s, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES}));
    let (v, f) = (volume(&mut s), flat_size(&mut s));
    // The inner edges (2 mm in and up) are the same bends.
    let mut a = Session::default();
    rule(&mut a, "Test", t, k, r, gap);
    tray(&mut a, t);
    run(&mut a, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 0], "bends": inner}));
    assert!((volume(&mut a) - v).abs() < 1e-6 * v && same_size(flat_size(&mut a), f, 1e-9));
    // From the floor inside, the sheet's other skin is the fixed one: the bends turn down from it
    // and the flat is the inside lengths less the inside setbacks, L' + 2H' - 4R + 2BA, which is
    // the same blank.
    let mut b = Session::default();
    rule(&mut b, "Test", t, k, r, gap);
    tray(&mut b, t);
    run(&mut b, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 2], "bends": BOTTOM_EDGES}));
    assert!(bend_rows(&mut b).iter().all(|row| row.0 == 90 && row.1 == "down"), "{:?}", bend_rows(&mut b));
    let ba = FRAC_PI_2 * (r + k * t);
    let inside = (96.0 + 2.0 * 28.0 - 4.0 * r + 2.0 * ba, 56.0 + 2.0 * 28.0 - 4.0 * r + 2.0 * ba);
    assert!(same_size(flat_size(&mut b), inside, 1e-6) && same_size(inside, f, 1e-6), "{:?} {inside:?} {f:?}", flat_size(&mut b));
    assert!((volume(&mut b) - v).abs() < 1e-6 * v, "{} vs {v}", volume(&mut b));
    let (lo, hi) = bbox(&mut b);
    assert!(lo.iter().all(|x| x.abs() < 1e-6) && (hi[0] - 100.0).abs() < 1e-6 && (hi[2] - 30.0).abs() < 1e-6, "{lo:?} {hi:?}");
}

#[test]
fn a_solid_block_takes_its_rules_thickness_inward() {
    let (k, r, gap) = (0.44, 2.0, 0.5);
    for (name, t) in [(None, 2.0), (Some("Thick"), 3.0)] {
        // The active rule, or the one the conversion names.
        let mut s = Session::default();
        rule(&mut s, "Thick", 3.0, k, r, gap);
        rule(&mut s, "Test", 2.0, k, r, gap);
        run(&mut s, "solid.box", json!({"length": 100, "width": 60, "height": 30, "body_name": "Block"}));
        let mut p = json!({"body": "Block", "face": [50, 30, 0], "bends": BOTTOM_EDGES});
        if let Some(n) = name {
            p["rule"] = json!(n);
        }
        run(&mut s, "sheet.convert", p);
        assert!((flat(&mut s)["thickness"].as_f64().unwrap_or(0.0) - t).abs() < 1e-9);
        // The block's faces are the sheet's outside; its top is left out.
        let (lo, hi) = bbox(&mut s);
        assert!(
            lo.iter().all(|v| v.abs() < 1e-6) && (hi[0] - 100.0).abs() < 1e-6 && (hi[1] - 60.0).abs() < 1e-6 && (hi[2] - 30.0).abs() < 1e-6,
            "{lo:?} {hi:?}"
        );
        let bd = deduction(FRAC_PI_2, r, t, k);
        assert!(same_size(flat_size(&mut s), (160.0 - 2.0 * bd, 120.0 - 2.0 * bd), 1e-6), "{:?}", flat_size(&mut s));
        let expect = tray_volume(100.0, 60.0, 30.0, t, r, gap);
        assert!((volume(&mut s) - expect).abs() < 2e-3 * expect, "{} vs {expect}", volume(&mut s));
    }
}

#[test]
fn a_trapezoid_section_unfolds_through_72_and_108_degree_bends() {
    let (t, k, r) = (2.0, 0.5, 1.0);
    let mut s = Session::default();
    rule(&mut s, "Test", t, k, r, 0.5);
    let bends = trapezoid_section(&mut s, t);
    run(&mut s, "sheet.convert", json!({"body": "Section", "face": [0, 0, 150], "bends": bends}));
    let rows = bend_rows(&mut s);
    assert!(rows.len() == 4 && rows.iter().all(|row| row.1 == "up" && (row.2 - 300.0).abs() < 1e-9), "{rows:?}");
    assert_eq!((rows.iter().filter(|row| row.0 == 72).count(), rows.iter().filter(|row| row.0 == 108).count()), (2, 2), "{rows:?}");
    let (a72, a108) = (72f64.to_radians(), 108f64.to_radians());
    let across = 200.0 + 2.0 * 40.0 + 2.0 * 15.0 - 2.0 * deduction(a72, r, t, k) - 2.0 * deduction(a108, r, t, k);
    assert!(same_size(flat_size(&mut s), (across, 300.0), 1e-6), "{:?} vs {across}", flat_size(&mut s));
    // The section keeps its outside faces: the returns' tops stay 40 sin 72° up and the length
    // stays; the widest point is on the round of the 108° bend (outer radius R + T), whose
    // centre lies (R + T) / sin 36° from the sharp corner along the corner's bisector.
    let (lo, hi) = bbox(&mut s);
    let (c72, s72) = (a72.cos(), a72.sin());
    let rho = r + t;
    let bisector_x = (-c72 - 1.0) / (-c72 - 1.0).hypot(-s72);
    let wide = 2.0 * (100.0 + 40.0 * c72 + bisector_x * rho / 36f64.to_radians().sin() + rho);
    // (The box is measured on the tessellation: the round comes out a chord's sag inside.)
    assert!((hi[0] - lo[0] - wide).abs() < 2e-3 && (hi[1] - 40.0 * s72).abs() < 1e-6 && (hi[2] - lo[2] - 300.0).abs() < 1e-6, "{lo:?} {hi:?} {wide}");
    run(&mut s, "sheet.unfold", json!({}));
    let (lo, hi) = bbox(&mut s);
    let flat_box = [(hi[0] - lo[0], hi[1] - lo[1]), (hi[0] - lo[0], hi[2] - lo[2])];
    assert!(flat_box.iter().any(|b| same_size(*b, (across, 300.0), 1e-6)), "{lo:?} {hi:?}");
}

#[test]
fn a_z_section_bends_up_at_one_edge_and_down_at_the_other() {
    let (t, k, r) = (2.0, 0.5, 1.0);
    let mut s = Session::default();
    rule(&mut s, "Test", t, k, r, 0.5);
    // Flange 60, web 40, flange 30 on one skin: its first corner is the outside of its bend,
    // its second the inside.
    let bends = section(&mut s, &[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (90.0, 40.0)], t, 100.0);
    run(&mut s, "sheet.convert", json!({"body": "Section", "face": [30, 0, 50], "bends": bends}));
    let mut directions: Vec<String> = bend_rows(&mut s).into_iter().map(|row| row.1).collect();
    directions.sort();
    assert_eq!(directions, ["down", "up"]);
    // Outside setbacks R + T at the first bend, inside setbacks R at the second, and a 90°
    // allowance for each: 60 + 40 + 30 - 2 (R + T) - 2 R + π (R + K T) = 128.2832.
    let across = 130.0 - 2.0 * (r + t) - 2.0 * r + PI * (r + k * t);
    assert!((across - 128.283185).abs() < 1e-6);
    assert!(same_size(flat_size(&mut s), (across, 100.0), 1e-6), "{:?} vs {across}", flat_size(&mut s));
    let (lo, hi) = bbox(&mut s);
    assert!(
        lo.iter().all(|v| v.abs() < 1e-6) && (hi[0] - 90.0).abs() < 1e-6 && (hi[1] - 42.0).abs() < 1e-6 && (hi[2] - 100.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // Three flat strips and two quarter rounds on the neutral radius R + T/2.
    let expect = t * 100.0 * ((60.0 - (r + t)) + (40.0 - (r + t) - r) + (30.0 - r) + PI * (r + t / 2.0));
    assert!((volume(&mut s) - expect).abs() < 2e-3 * expect, "{} vs {expect}", volume(&mut s));
}

#[test]
fn a_tapered_tray_converts_with_72_degree_walls_and_slanted_corners() {
    let (t, k, r, gap) = (2.0, 0.5, 1.0, 0.5);
    let mut s = Session::default();
    rule(&mut s, "Test", t, k, r, gap);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Floor"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [200, 120]}));
    run(&mut s, "sketch.finish", json!({}));
    // Walls leaning out 18°: 72° bends from the floor.
    run(&mut s, "solid.extrude", json!({"sketch": "Floor", "distance": 40, "taper": "18 deg", "body_name": "Tray"}));
    let (lo, hi) = bbox(&mut s);
    assert!(hi[0] - lo[0] > 200.0 + 1.0, "the taper widens the top: {lo:?} {hi:?}");
    run(&mut s, "solid.shell", json!({"faces": [[100, 60, 40]], "thickness": t}));
    run(&mut s, "sheet.convert", json!({"body": "Tray", "face": [100, 60, 0], "bends": [[100, 0, 0], [200, 60, 0], [100, 120, 0], [0, 60, 0]]}));
    let rows = bend_rows(&mut s);
    assert!(rows.len() == 4 && rows.iter().all(|row| row.0 == 72 && row.1 == "up"), "{rows:?}");
    let slant = 40.0 / 72f64.to_radians().sin();
    let bd = deduction(72f64.to_radians(), r, t, k);
    assert!(same_size(flat_size(&mut s), (200.0 + 2.0 * slant - 2.0 * bd, 120.0 + 2.0 * slant - 2.0 * bd), 1e-6), "{:?}", flat_size(&mut s));
    // The outside stays; the rim is cut square to the sheet, as a blank is, so the inside edge
    // of a wall leaning out 18° stands T sin 18° above the solid's level rim.
    let (lo2, hi2) = bbox(&mut s);
    assert!((0..2).all(|i| (lo2[i] - lo[i]).abs() < 1e-6 && (hi2[i] - hi[i]).abs() < 1e-6) && lo2[2].abs() < 1e-6, "{lo2:?} {hi2:?}");
    assert!((hi2[2] - 40.0 - t * 18f64.to_radians().sin()).abs() < 1e-6, "{hi2:?}");
    let v = volume(&mut s);
    run(&mut s, "sheet.unfold", json!({}));
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - v).abs() < 1e-6 * v);
}

#[test]
fn the_sheet_follows_the_solid_through_parameter_changes_undo_and_reopening() {
    let (t, k, r) = (2.0, 0.44, 2.0);
    let mut s = Session::default();
    rule(&mut s, "Test", t, k, r, 0.5);
    run(&mut s, "parameters.add", json!({"name": "L", "expression": "100 mm"}));
    run(&mut s, "parameters.add", json!({"name": "T", "expression": "2 mm"}));
    run(&mut s, "solid.box", json!({"length": "L", "width": 60, "height": 30, "body_name": "Tray"}));
    run(&mut s, "solid.shell", json!({"faces": [[50, 30, 30]], "thickness": "T"}));
    run(&mut s, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES}));
    let bd = |t: f64| deduction(FRAC_PI_2, r, t, k);
    assert!(same_size(flat_size(&mut s), (160.0 - 2.0 * bd(t), 120.0 - 2.0 * bd(t)), 1e-6));
    // Longer: the right-hand bend edge moved from x = 100 to x = 140 and is found by its name.
    run(&mut s, "parameters.change", json!({"name": "L", "expression": "140 mm"}));
    assert!(s.model.results.iter().all(|x| x.error.is_none()), "{:?}", s.model.results.iter().map(|x| (&x.name, &x.error)).collect::<Vec<_>>());
    assert!(same_size(flat_size(&mut s), (200.0 - 2.0 * bd(t), 120.0 - 2.0 * bd(t)), 1e-6), "{:?}", flat_size(&mut s));
    let (_, hi) = bbox(&mut s);
    assert!((hi[0] - 140.0).abs() < 1e-6, "{hi:?}");
    // Thicker walls: the sheet takes the shell's new thickness.
    run(&mut s, "parameters.change", json!({"name": "T", "expression": "3 mm"}));
    assert!(s.model.results.iter().all(|x| x.error.is_none()));
    assert!((flat(&mut s)["thickness"].as_f64().unwrap_or(0.0) - 3.0).abs() < 1e-9);
    let thick = (200.0 - 2.0 * bd(3.0), 120.0 - 2.0 * bd(3.0));
    assert!(same_size(flat_size(&mut s), thick, 1e-6), "{:?}", flat_size(&mut s));
    run(&mut s, "edit.undo", json!({}));
    assert!(same_size(flat_size(&mut s), (200.0 - 2.0 * bd(t), 120.0 - 2.0 * bd(t)), 1e-6));
    run(&mut s, "edit.redo", json!({}));
    assert!(same_size(flat_size(&mut s), thick, 1e-6));
    // The design file keeps the bend edges, and reopens to the same sheet.
    let saved = s.doc.to_json();
    assert!(saved.contains("\"bends\""), "{saved}");
    let mut reopened = Session::new(solidcraft_doc::Document::from_json(&saved).unwrap());
    assert!(same_size(flat_size(&mut reopened), thick, 1e-6));
    // Bend edges came with format 17: a build from before them asks for an update.
    let version = serde_json::from_str::<Value>(&saved).unwrap()["format"].as_str().and_then(solidcraft_doc::format::version_of);
    assert!(version >= Some(17), "{version:?}");
}

/// Designs from before bend edges open unchanged: a plate conversion saves without `bends`, as
/// it always did, and a format 16 file with one is the same document and the same sheet.
#[test]
fn designs_from_before_bend_edges_open_unchanged() {
    let mut s = Session::default();
    rule(&mut s, "Test", 2.0, 0.44, 2.0, 0.5);
    run(&mut s, "solid.box", json!({"length": 100, "width": 60, "height": 2, "body_name": "Plate"}));
    run(&mut s, "sheet.convert", json!({"body": "Plate", "face": [50, 30, 2]}));
    let mut older: Value = serde_json::from_str(&s.doc.to_json()).unwrap();
    assert!(!older.to_string().contains("\"bends\""), "{older}");
    older["format"] = json!("solidcraft/16");
    let doc = solidcraft_doc::Document::from_json(&older.to_string()).unwrap();
    assert_eq!(doc, *s.doc);
    let mut reopened = Session::new(doc);
    assert!(reopened.model.results.iter().all(|x| x.error.is_none()));
    assert!(same_size(flat_size(&mut reopened), (100.0, 60.0), 1e-9), "{:?}", flat_size(&mut reopened));
    assert!((volume(&mut reopened) - 100.0 * 60.0 * 2.0).abs() < 1e-6);
    assert_eq!(flat(&mut reopened), flat(&mut s));
}

#[test]
fn bend_edges_changed_in_an_edit_are_the_ones_bent() {
    let (t, k, r) = (2.0, 0.44, 2.0);
    let mut s = Session::default();
    rule(&mut s, "Test", t, k, r, 0.5);
    tray(&mut s, t);
    let made = run(&mut s, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES}));
    // Only the two long walls: a channel as long as the tray, 60 + 2 x 30 less two deductions
    // across. (By the names of the first two edges it would be the front and the right wall.)
    run(&mut s, "timeline.edit", json!({"feature": made["feature"], "set": {"bends": [BOTTOM_EDGES[0], BOTTOM_EDGES[2]]}}));
    let rows = bend_rows(&mut s);
    assert!(rows.len() == 2 && rows.iter().all(|row| row.0 == 90 && row.1 == "up" && (row.2 - 100.0).abs() < 1e-6), "{rows:?}");
    let across = 120.0 - 2.0 * deduction(FRAC_PI_2, r, t, k);
    assert!(same_size(flat_size(&mut s), (100.0, across), 1e-6), "{:?} vs {across}", flat_size(&mut s));
    // Its end walls are no faces of the sheet any more: the channel is open at both ends.
    let expect = t * 100.0 * ((60.0 - 2.0 * (r + t)) + 2.0 * (30.0 - (r + t)) + PI * (r + t / 2.0));
    assert!((volume(&mut s) - expect).abs() < 2e-3 * expect, "{} vs {expect}", volume(&mut s));
}

#[test]
fn edge_flanges_and_cuts_work_on_a_converted_sheet() {
    let mut s = Session::default();
    rule(&mut s, "Test", 2.0, 0.44, 2.0, 0.5);
    tray(&mut s, 2.0);
    run(&mut s, "sheet.convert", json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES}));
    let v0 = volume(&mut s);
    // A return flange on the front wall's top edge.
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 30]], "height": 10, "angle": 90}));
    let v1 = volume(&mut s);
    assert!(v1 > v0 + 500.0, "{v0} {v1}");
    assert_eq!(bend_rows(&mut s).len(), 5);
    // A hole cut in the flat stays in the base when the tray is folded again.
    run(&mut s, "sheet.unfold", json!({}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Hole"}));
    run(&mut s, "sketch.circle.center", json!({"center": [50, 30], "radius": 5}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Hole", "distance": 10, "operation": "cut", "direction": "symmetric"}));
    run(&mut s, "sheet.refold", json!({}));
    assert_eq!(flat(&mut s)["cutouts"].as_array().map(Vec::len), Some(1));
    let hole = PI * 25.0 * 2.0;
    assert!((volume(&mut s) - (v1 - hole)).abs() < 2e-3 * v1, "{} vs {}", volume(&mut s), v1 - hole);
}

#[test]
fn conversions_that_cannot_work_say_why() {
    let mut s = Session::default();
    rule(&mut s, "Wide", 2.0, 0.44, 40.0, 0.5);
    rule(&mut s, "Test", 2.0, 0.44, 2.0, 0.5);
    tray(&mut s, 2.0);
    let err = |s: &mut Session, p: Value| s.execute("sheet.convert", &p).err().map(|e| e.to_string()).unwrap_or_default();
    // A bend that doesn't reach the fixed face through other bends.
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, 0], "bends": [[0, 0, 15]]}));
    assert!(e.contains("does not connect"), "{e}");
    // No edge there, and no face there.
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, 0], "bends": [[50, 30, 0]]}));
    assert!(e.contains("no edge"), "{e}");
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, -20], "bends": BOTTOM_EDGES}));
    assert!(e.contains("no face"), "{e}");
    // A loop of bends.
    let mut bends: Vec<[f64; 3]> = BOTTOM_EDGES.to_vec();
    bends.push([0.0, 0.0, 15.0]);
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, 0], "bends": bends}));
    assert!(e.contains("loop"), "{e}");
    // Walls shorter than their bends' setbacks (a 40 mm bend radius on 30 mm walls).
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES, "rule": "Wide"}));
    assert!(e.contains("setback"), "{e}");
    // The sheet's values are the rule's; an empty list of bends is not a plate.
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, 0], "bends": BOTTOM_EDGES, "thickness": 3}));
    assert!(e.contains("the sheet metal rule sets it"), "{e}");
    let e = err(&mut s, json!({"body": "Tray", "face": [50, 30, 0], "bends": []}));
    assert!(e.contains("1…1000"), "{e}");
    // Curved faces.
    let mut c = Session::default();
    run(&mut c, "solid.cylinder", json!({"radius": 10, "height": 30, "body_name": "Rod"}));
    let e = err(&mut c, json!({"body": "Rod", "face": [0, 0, 30], "bends": [[10, 0, 30]]}));
    assert!(e.contains("curved"), "{e}");
    // Nothing changed.
    assert!(s.model.state().sheets.is_empty() && c.model.state().sheets.is_empty());
}
