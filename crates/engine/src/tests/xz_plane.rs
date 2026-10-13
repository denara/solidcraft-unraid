//! The XZ origin plane (#78): sketch x = +X, y = −Z, normal +Y, so a positive extrude from XZ
//! (or from a plane offset from it) goes +Y. Designs saved before format 6 used y = +Z and
//! normal −Y; opening one keeps its geometry exactly where it was saved.

use serde_json::{Value, json};

use super::run;
use crate::*;

/// A 10 × 5 rectangle on XZ extruded 3, and a 4 × 4 square on a plane offset 7 from XZ
/// extruded 2.
fn build(s: &mut Session) {
    run(s, "sketch.create", json!({"plane": "XZ", "name": "Base"}));
    run(s, "sketch.line", json!({"points": [[0, 0], [10, 0], [10, 5], [0, 5]], "closed": true}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"sketch": "Base", "distance": 3, "body_name": "Slab"}));
    run(s, "construct.plane.offset", json!({"base": "XZ", "offset": "7 mm", "name": "Shelf"}));
    run(s, "sketch.create", json!({"plane": "Shelf", "name": "Top"}));
    run(s, "sketch.line", json!({"points": [[20, 1], [24, 1], [24, 5], [20, 5]], "closed": true}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"sketch": "Top", "distance": 2, "body_name": "Post", "operation": "new"}));
}

/// Volume and world bounding box (min, max) of each body.
fn bodies(s: &mut Session) -> Vec<(String, f64, [f64; 3], [f64; 3])> {
    let m = run(s, "inspect.measure", json!({}));
    let v3 = |v: &Value| -> [f64; 3] { [0, 1, 2].map(|i| v[i].as_f64().unwrap()) };
    m["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["name"].as_str().unwrap().to_string(), b["volume_mm3"].as_f64().unwrap(), v3(&b["bbox"]["min"]), v3(&b["bbox"]["max"])))
        .collect()
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

fn expect(got: &[(String, f64, [f64; 3], [f64; 3])], want: &[(&str, f64, [f64; 3], [f64; 3])]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for ((name, vol, min, max), (wname, wvol, wmin, wmax)) in got.iter().zip(want) {
        assert_eq!(name, wname);
        assert!((vol - wvol).abs() < 1e-6 * wvol, "{name}: volume {vol} vs {wvol}");
        assert!(close(*min, *wmin) && close(*max, *wmax), "{name}: bbox {min:?}..{max:?}, expected {wmin:?}..{wmax:?}");
    }
}

#[test]
fn positive_extrude_from_xz_goes_plus_y() {
    let mut s = Session::default();
    build(&mut s);
    expect(
        &bodies(&mut s),
        &[
            // Sketch y = −Z; extrude along +Y.
            ("Slab", 150.0, [0.0, 0.0, -5.0], [10.0, 3.0, 0.0]),
            // The offset plane sits at Y = +7 and its extrude continues to +Y.
            ("Post", 32.0, [20.0, 7.0, -5.0], [24.0, 9.0, -1.0]),
        ],
    );
}

#[test]
fn format_5_xz_design_opens_where_it_was_saved() {
    let dir = std::env::temp_dir().join(format!("solidcraft-xz78-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old-xz.solidcraft");
    let mut s = Session::default();
    build(&mut s);
    // This fixture edits the historical JSON metadata directly; request JSON explicitly.
    run(&mut s, "file.save_as", json!({"path": path.to_string_lossy(), "format": "json"}));
    // Stamp the file as format 5: its XZ references then mean the old frame (sketch y = +Z,
    // normal −Y), which is what a build before format 6 wrote and evaluated.
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(v.to_string().contains("\"XZ\""), "the saved design names the XZ plane");
    v["format"] = Value::from("solidcraft/5");
    std::fs::write(&path, v.to_string()).unwrap();

    let mut old = Session::default();
    run(&mut old, "doc.open", json!({"path": path.to_string_lossy()}));
    let want = [
        // The old frame: the same sketch coordinates land at +Z and the extrude goes −Y.
        ("Slab", 150.0, [0.0, -3.0, 0.0], [10.0, 0.0, 5.0]),
        ("Post", 32.0, [20.0, -9.0, 1.0], [24.0, -7.0, 5.0]),
    ];
    expect(&bodies(&mut old), &want);

    // Saving the migrated design writes the current format and keeps the geometry.
    let resaved = dir.join("resaved.solidcraft");
    run(&mut old, "file.save_as", json!({"path": resaved.to_string_lossy(), "format": "json"}));
    let text = std::fs::read_to_string(&resaved).unwrap();
    assert!(text.contains(&format!("\"solidcraft/{}\"", solidcraft_doc::format::FORMAT)), "resaved at the current format");
    let mut again = Session::default();
    run(&mut again, "doc.open", json!({"path": resaved.to_string_lossy()}));
    expect(&bodies(&mut again), &want);
    let _ = std::fs::remove_dir_all(&dir);
}
