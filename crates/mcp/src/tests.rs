//! An agent builds parts through MCP only and verifies them through MCP only.

use std::f64::consts::PI;

use serde_json::{Value, json};

use crate::{Headless, PROTOCOL_VERSION, Server, tool_definitions};

fn server() -> Server {
    Server::new(Box::new(Headless::default()))
}

fn call(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    let r = s.handle_line(&line).expect("reply");
    serde_json::from_str(&r).unwrap()
}

fn raw_tool(s: &mut Server, name: &str, args: Value) -> Value {
    call(s, 99, "tools/call", json!({"name": name, "arguments": args}))
}

/// Call a tool that must succeed; returns its JSON text (or the first content block).
fn tool(s: &mut Server, name: &str, args: Value) -> Value {
    let r = raw_tool(s, name, args);
    let c = &r["result"]["content"][0];
    assert_ne!(r["result"]["isError"], json!(true), "{name} failed: {c}");
    assert!(r.get("error").is_none(), "{name}: {r}");
    if c["type"] == "text" { serde_json::from_str(c["text"].as_str().unwrap()).unwrap_or(Value::Null) } else { c.clone() }
}

#[test]
fn crash_reports_are_listed_and_read_without_changing_the_design() {
    let mut s = server();
    let list = tool(&mut s, "crash_reports", json!({"limit": 3}));
    assert!(list["reports"].as_array().is_some_and(|r| r.len() <= 3), "{list}");
    assert!(list["dir"].is_string(), "{list}");
    let r = raw_tool(&mut s, "crash_reports", json!({"id": "../../etc"}));
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
    let r = raw_tool(&mut s, "crash_reports", json!({"id": 5}));
    assert!(r.get("error").is_some() || r["result"]["isError"] == json!(true), "{r}");
}

fn volume(s: &mut Server) -> f64 {
    tool(s, "measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-12)
}

#[test]
fn handshake_and_tools_list() {
    let mut s = server();
    let init =
        call(&mut s, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "solidcraft");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["instructions"].as_str().unwrap().contains("headless"));
    // An unknown version gets ours.
    let init = call(&mut s, 2, "initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(init["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(s.is_initialized());
    assert_eq!(call(&mut s, 3, "ping", json!({}))["result"], json!({}));

    let tools = call(&mut s, 4, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "list_commands",
        "execute",
        "batch",
        "inspect_design",
        "measure",
        "body_topology",
        "set_parameter",
        "screenshot",
        "export",
        "undo",
        "redo",
        "crash_reports",
        "new_design",
        "open",
        "save",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    for t in tools["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        assert!(t["description"].as_str().unwrap().len() > 20, "{t}");
    }

    let cmds = tool(&mut s, "list_commands", json!({"filter": "fillet"}));
    assert!(cmds.as_array().unwrap().iter().any(|c| c["id"] == "solid.fillet"), "{cmds}");
    assert!(cmds.as_array().unwrap().len() < 5);

    let res = call(&mut s, 5, "resources/list", json!({}));
    assert_eq!(res["result"]["resources"].as_array().unwrap().len(), 2);
    let d = call(&mut s, 6, "resources/read", json!({"uri": "solidcraft://design"}));
    assert!(d["result"]["contents"][0]["text"].as_str().unwrap().contains("timeline"));
    assert_eq!(call(&mut s, 7, "resources/read", json!({"uri": "solidcraft://nope"}))["error"]["code"], -32002);
    assert_eq!(call(&mut s, 8, "nope/nope", json!({}))["error"]["code"], -32601);
}

/// Box → fillet → hole in one batch, then verify with measure / inspect_design and look at it.
#[test]
fn agent_builds_box_fillet_hole() {
    let mut s = server();
    let r = tool(
        &mut s,
        "batch",
        json!({"commands": [
            {"command": "sketch.create", "params": {"plane": "XY"}},
            {"command": "sketch.rectangle.two_point", "params": {"p0": [0, 0], "p1": [40, 30]}},
            {"command": "sketch.finish"},
            {"command": "solid.extrude", "params": {"distance": 20}},
            {"command": "solid.fillet", "params": {"edges": [[0, 0, 10]], "radius": 3}},
            {"command": "sketch.create", "params": {"plane": "XY"}},
            {"command": "sketch.circle.center", "params": {"center": [20, 15], "radius": 5}},
            {"command": "solid.extrude", "params": {"distance": 20, "operation": "cut"}},
        ]}),
    );
    assert_eq!(r["completed"], 8, "{r}");
    let expect = 24000.0 - (9.0 - PI * 9.0 / 4.0) * 20.0 - PI * 25.0 * 20.0;
    let v = volume(&mut s);
    assert!(rel(v, expect) < 5e-4, "{v} vs {expect}");
    let m = tool(&mut s, "measure", json!({"body": "Body1"}));
    // 6 box faces + fillet + hole wall (the kernel may keep the wall as two halves).
    assert!((8..=9).contains(&m["bodies"][0]["faces"].as_u64().unwrap()), "{m}");
    assert!(rel(m["bodies"][0]["bbox"]["max"][0].as_f64().unwrap(), 40.0) < 1e-6, "{m}");

    let d = tool(&mut s, "inspect_design", json!({}));
    assert_eq!(d["timeline"].as_array().unwrap().len(), 5, "{d}");
    assert!(d["timeline"].as_array().unwrap().iter().all(|f| f["error"].is_null()), "{d}");
    assert_eq!(d["sketches"][0]["curves"].as_array().unwrap().len(), 4, "{d}");
    assert!(d["sketches"][0]["status"].is_string(), "{d}");
    assert!(d["bodies"][0]["volume_mm3"].as_f64().unwrap() > 0.0);

    let topo = tool(&mut s, "body_topology", json!({"body": "Body1"}));
    assert!(topo["edges"].as_array().unwrap().len() >= 12, "{topo}");
    assert!(topo["faces"].as_array().unwrap().len() >= 8, "{topo}");

    // Undo the hole, redo it.
    tool(&mut s, "undo", json!({}));
    assert!(rel(volume(&mut s), 24000.0 - (9.0 - PI * 9.0 / 4.0) * 20.0) < 5e-4);
    tool(&mut s, "redo", json!({}));
    assert!(rel(volume(&mut s), expect) < 5e-4);

    // Look at it from the top: a real PNG comes back as an image block.
    let img = tool(&mut s, "screenshot", json!({"view": "top", "width": 320, "height": 200}));
    assert_eq!(img["type"], "image");
    assert_eq!(img["mimeType"], "image/png");
    assert!(img["data"].as_str().unwrap().starts_with("iVBORw0KGgo"), "not a PNG");

    // Export, save, start over, open.
    let dir = std::env::temp_dir().join(format!("solidcraft-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let step = dir.join("part.step");
    tool(&mut s, "export", json!({"path": step.to_string_lossy()}));
    assert!(std::fs::read_to_string(&step).unwrap().contains("ISO-10303-21"));
    let stl = dir.join("part.stl");
    tool(&mut s, "export", json!({"path": stl.to_string_lossy(), "format": "stl"}));
    assert!(std::fs::metadata(&stl).unwrap().len() > 1000);
    let shot = dir.join("shot.png");
    tool(&mut s, "screenshot", json!({"path": shot.to_string_lossy(), "width": 64, "height": 64}));
    assert!(std::fs::metadata(&shot).unwrap().len() > 100);
    let design = dir.join("part.solidcraft");
    tool(&mut s, "save", json!({"path": design.to_string_lossy()}));
    tool(&mut s, "new_design", json!({"name": "Empty"}));
    assert_eq!(tool(&mut s, "measure", json!({}))["body_count"], 0);
    tool(&mut s, "open", json!({"path": design.to_string_lossy()}));
    assert!(rel(volume(&mut s), expect) < 5e-4);
    // A STEP file opens as a new design with the same body.
    let r = tool(&mut s, "open", json!({"path": step.to_string_lossy()}));
    assert_eq!(r["features"], 1, "{r}");
    assert!(rel(volume(&mut s), expect) < 5e-4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_selects_container_or_json_and_reopens_by_contents() {
    let mut s = server();
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": 10, "width": 20, "height": 30}}));
    let dir = std::env::temp_dir().join(format!("solidcraft-mcp-encodings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, format, is_json) in [
        ("default.solidcraft", None, false),
        ("explicit.solidcraft", Some("json"), true),
        ("legacy.solvecraft", None, false),
        ("inferred.json", None, true),
        ("container.json", Some("container"), false),
    ] {
        let path = dir.join(name);
        let mut args = json!({"path": path.to_string_lossy()});
        if let Some(format) = format {
            args["format"] = json!(format);
        }
        tool(&mut s, "save", args);
        let bytes = std::fs::read(&path).unwrap();
        if is_json {
            assert!(serde_json::from_slice::<Value>(&bytes).unwrap().is_object());
        } else {
            assert!(bytes.starts_with(b"PK\x03\x04"));
        }
        tool(&mut s, "new_design", json!({}));
        tool(&mut s, "open", json!({"path": path.to_string_lossy()}));
        assert!(rel(volume(&mut s), 6000.0) < 1e-9);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parameters_drive_the_design() {
    let mut s = server();
    tool(&mut s, "set_parameter", json!({"name": "len", "value": "40 mm"}));
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": "len", "width": 30, "height": "len / 2", "corner": [0, 0, 0]}}));
    assert!(rel(volume(&mut s), 40.0 * 30.0 * 20.0) < 1e-9);
    let r = tool(&mut s, "set_parameter", json!({"name": "len", "value": 50}));
    assert_eq!(r["value"], 50.0, "{r}");
    assert_eq!(r["errors"].as_array().unwrap().len(), 0, "{r}");
    assert!(rel(volume(&mut s), 50.0 * 30.0 * 25.0) < 1e-9);
    let d = tool(&mut s, "inspect_design", json!({"measure": false, "sketches": false}));
    assert!(d["params"].as_array().unwrap().iter().any(|p| p["name"] == "len" && p["value"] == 50.0), "{d}");
}

#[test]
fn batch_stops_at_first_error_and_rolls_back() {
    let mut s = server();
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}}));
    let r = raw_tool(
        &mut s,
        "batch",
        json!({"commands": [
            {"command": "solid.sphere", "params": {"radius": 3, "center": [50, 0, 0]}},
            {"command": "solid.extrude", "params": {"distance": "bogus +"}},
            {"command": "solid.sphere", "params": {"radius": 3, "center": [80, 0, 0]}},
        ]}),
    );
    assert_eq!(r["result"]["isError"], true, "{r}");
    let body: Value = serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["failed"]["index"], 1, "{body}");
    assert_eq!(body["failed"]["command"], "solid.extrude");
    assert_eq!(body["undone"], 1, "{body}");
    assert_eq!(body["results"].as_array().unwrap().len(), 1);
    // Only the first box is left, and its undo step is intact.
    assert_eq!(tool(&mut s, "measure", json!({}))["body_count"], 1);
    assert_eq!(tool(&mut s, "inspect_design", json!({"measure": false}))["undo"], 1);

    // Without rollback the completed steps stay.
    let r = raw_tool(
        &mut s,
        "batch",
        json!({"rollback": false, "commands": [{"command": "solid.sphere", "params": {"radius": 3, "center": [50, 0, 0]}}, {"command": "NoSuchCommand"}]}),
    );
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("NoSuchCommand"));
    assert_eq!(tool(&mut s, "measure", json!({}))["body_count"], 2);
}

/// Regression: with a full undo history a failed batch reported `undone: 0` and left its
/// sketches behind (an empty "Sketch1", two sketches named "TapHole").
#[test]
fn batch_rolls_back_with_a_full_undo_history() {
    let mut s = server();
    let fill: Vec<Value> =
        (0..230).map(|i| json!({"command": "parameters.add", "params": {"name": format!("p{i}"), "expression": "1 mm"}})).collect();
    tool(&mut s, "batch", json!({"commands": fill}));
    let undo = tool(&mut s, "inspect_design", json!({"measure": false}))["undo"].clone();
    for _ in 0..2 {
        let r = raw_tool(
            &mut s,
            "batch",
            json!({"commands": [
                {"command": "sketch.create", "params": {"plane": "XY", "name": "TapHole"}},
                {"command": "sketch.circle.center", "params": {"center": [0, 0], "radius": 2}},
                {"command": "sketch.dimension", "params": {"entities": ["nope"], "value": 3}},
            ]}),
        );
        let body: Value = serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["failed"]["index"], 2, "{body}");
        assert_eq!(body["undone"], 2, "{body}");
    }
    let d = tool(&mut s, "inspect_design", json!({"measure": false, "sketches": false}));
    assert_eq!(d["sketches"].as_array().map(Vec::len), Some(0), "{d}");
    assert!(d["active_sketch"].is_null(), "{d}");
    assert_eq!(d["undo"], undo);
}

#[test]
fn helper_sketches_can_be_hidden() {
    let mut s = server();
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": 40, "width": 40, "height": 2}}));
    let sk = tool(&mut s, "execute", json!({"command": "sketch.create", "params": {"plane": "XY"}}))["sketch"].clone();
    tool(&mut s, "execute", json!({"command": "sketch.line", "params": {"points": [[0, 0], [40, 0]], "construction": true}}));
    tool(&mut s, "execute", json!({"command": "sketch.finish"}));
    let visible = |s: &mut Server| tool(s, "inspect_design", json!({"measure": false}))["sketches"][0]["visible"].clone();
    assert_eq!(visible(&mut s), true);
    tool(&mut s, "execute", json!({"command": "browser.visibility", "params": {"items": [sk], "visible": false}}));
    assert_eq!(visible(&mut s), false);
    let r = raw_tool(&mut s, "execute", json!({"command": "browser.visibility", "params": {"items": ["Nope"], "visible": false}}));
    assert_eq!(r["result"]["isError"], true);
    tool(&mut s, "execute", json!({"command": "browser.visibility", "params": {"folder": "sketches", "visible": true}}));
    assert_eq!(visible(&mut s), true);
}

#[test]
fn unknown_tools_and_bad_arguments_are_clean_errors() {
    let mut s = server();
    let r = raw_tool(&mut s, "make_coffee", json!({}));
    assert_eq!(r["error"]["code"], -32602, "{r}");
    assert!(r["error"]["message"].as_str().unwrap().contains("make_coffee"));
    assert_eq!(raw_tool(&mut s, "execute", json!({}))["error"]["code"], -32602);
    assert_eq!(raw_tool(&mut s, "execute", json!({"command": 5}))["error"]["code"], -32602);
    assert_eq!(raw_tool(&mut s, "measure", json!({"bogus": 1}))["error"]["code"], -32602);
    assert_eq!(raw_tool(&mut s, "screenshot", json!({"source": "sideways"}))["error"]["code"], -32602);
    // `view` also takes saved named views, so an unknown name is a tool error naming it.
    let r = raw_tool(&mut s, "screenshot", json!({"view": "sideways"}));
    assert_eq!(r["result"]["isError"], true, "{r}");
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("sideways"), "{r}");
    assert_eq!(call(&mut s, 1, "tools/call", json!({"arguments": {}}))["error"]["code"], -32602);
    // Engine errors are tool errors the model can read.
    let r = raw_tool(&mut s, "execute", json!({"command": "solid.extrude", "params": {"distance": 10}}));
    assert_eq!(r["result"]["isError"], true, "{r}");
    let r = raw_tool(&mut s, "execute", json!({"command": "NoSuchCommand"}));
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("unknown command"));
    // Framing errors.
    assert!(s.handle_line("{not json").unwrap().contains("-32700"));
    assert!(s.handle_line("[]").unwrap().contains("-32600"));
    assert!(s.handle_line("42").unwrap().contains("-32600"));
    assert!(s.handle_line("   ").is_none());
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).is_none());
    let both = s.handle_line(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/x"}]"#).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&both).unwrap().as_array().unwrap().len(), 1);
}

fn hostile_values() -> Vec<Value> {
    vec![
        Value::Null,
        json!(true),
        json!(-1),
        json!(0),
        json!(1e308),
        json!(-1e308),
        json!(""),
        json!("nope"),
        json!("../../../../../../nonexistent/dir/x.step"),
        json!("1/0"),
        json!([]),
        json!([1e308, -1e308]),
        json!([{"command": "solid.extrude", "params": {"distance": 1e308}}]),
        json!([{"command": 7}, null, "x"]),
        json!({}),
        json!({"x": 1}),
    ]
}

/// Every tool, every argument it documents, every hostile value: replies are well-formed
/// JSON-RPC and nothing panics.
#[test]
fn hostile_arguments_never_panic() {
    let mut s = server();
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}}));
    let defs = tool_definitions();
    let mut n = 0;
    for def in &defs {
        let name = def["name"].as_str().unwrap();
        // Hostile strings are relative paths: don't let the sweep write files into the crate.
        let writes = matches!(name, "save" | "screenshot" | "export");
        let keys: Vec<String> = def["inputSchema"]["properties"].as_object().unwrap().keys().filter(|k| !(writes && *k == "path")).cloned().collect();
        for k in &keys {
            for v in hostile_values() {
                let mut args = json!({});
                // Fill the other required arguments with something plausible.
                for r in def["inputSchema"]["required"].as_array().into_iter().flatten() {
                    args[r.as_str().unwrap()] = json!("Body1");
                }
                args[k.as_str()] = v;
                if name == "screenshot" && (k == "width" || k == "height") {
                    args["view"] = json!("front");
                }
                let r = raw_tool(&mut s, name, args.clone());
                assert!(r.get("result").is_some() || r["error"]["code"].is_i64(), "{name} {args}: {r}");
                n += 1;
            }
        }
    }
    assert!(n > 100);
    // Execute every command with hostile params through MCP too.
    let cmds = tool(&mut s, "list_commands", json!({}));
    for c in cmds.as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        if matches!(id, "doc.open" | "file.save" | "file.save_as" | "file.export" | "file.save_mesh") {
            continue;
        }
        for v in hostile_values() {
            let r = raw_tool(&mut s, "execute", json!({"command": id, "params": {"distance": v.clone(), "edges": v.clone(), "plane": v}}));
            assert!(r.get("result").is_some(), "{id}: {r}");
        }
    }
    // Arguments that are not an object.
    for v in hostile_values().into_iter().filter(|v| !v.is_object() && !v.is_null()) {
        let r = call(&mut s, 1, "tools/call", json!({"name": "measure", "arguments": v}));
        assert_eq!(r["error"]["code"], -32602, "{r}");
    }
}

#[test]
fn base64_matches_rfc4648() {
    use crate::tools::base64;
    for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
        assert_eq!(base64(i.as_bytes()), o);
    }
}

/// Filtering by a command id returns that command, not every command whose docs mention it (#269).
#[test]
fn list_commands_filter_by_a_command_id_matches_ids() {
    let mut s = server();
    let cmds = tool(&mut s, "list_commands", json!({"filter": "sketch.delete"}));
    let ids: Vec<&str> = cmds.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
    assert!(ids.contains(&"sketch.delete"), "{ids:?}");
    assert!(ids.iter().all(|id| id.contains("sketch.delete")), "{} commands matched: {ids:?}", ids.len());
}

/// Compatibility shims (docs/compatibility.md): a design saved before the rename (legacy
/// extension and format tag) opens through MCP, and the legacy resource URIs still read.
#[test]
fn legacy_design_files_and_resource_uris() {
    let mut s = server();
    let saved = String::from_utf8(solidcraft_engine::io::write_design(&solidcraft_engine::doc::Document::default())).unwrap();
    assert!(saved.contains("\"solidcraft/"), "{saved}");
    let legacy = saved.replace("\"solidcraft/", "\"solvecraft/");
    let path = std::env::temp_dir().join(format!("solidcraft-mcp-legacy-{}.solvecraft", std::process::id()));
    std::fs::write(&path, legacy).unwrap();
    tool(&mut s, "open", json!({"path": path.to_string_lossy()}));
    let _ = std::fs::remove_file(&path);
    let d = call(&mut s, 1, "resources/read", json!({"uri": "solvecraft://design"}));
    assert!(d["result"]["contents"][0]["text"].as_str().unwrap().contains("timeline"));
}

/// #528: an agent saves the angle of a product photo as a named view once, then renders the
/// model from that view after each change. The render must use the saved camera, not a
/// standard view.
#[test]
fn screenshot_renders_a_saved_named_view() {
    let mut s = server();
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": 40, "width": 20, "height": 10}}));
    let camera = json!({"target": [20, 10, 5], "yaw": 0.5, "pitch": -0.6, "distance": 150});
    tool(&mut s, "execute", json!({"command": "view.save", "params": {"name": "Photo", "camera": camera.clone()}}));
    let shot = |s: &mut Server, view: &str| {
        let img = tool(s, "screenshot", json!({"view": view, "width": 96, "height": 64}));
        img["data"].as_str().unwrap_or_default().to_string()
    };
    let iso = shot(&mut s, "iso");
    let photo = shot(&mut s, "Photo");
    assert!(photo.starts_with("iVBORw0KGgo"), "not a PNG");
    assert_ne!(photo, iso, "the named view rendered like the standard iso view");
    // The same camera given directly renders the same picture; a standard view turns back.
    let direct = tool(&mut s, "screenshot", json!({"camera": camera, "width": 96, "height": 64}));
    assert_eq!(direct["data"].as_str().unwrap_or_default(), photo);
    assert_eq!(shot(&mut s, "iso"), iso);
}

/// #530: in a design with many bodies every feature result lists all of them, so a batch reply
/// is mostly the same body list once per step. A compact batch reply names only what each step
/// added or changed.
#[test]
fn compact_batch_reply_does_not_repeat_the_body_list() {
    let mut s = server();
    let cube =
        |x: i64, y: i64| json!({"command": "solid.box", "params": {"length": 1, "width": 1, "height": 1, "corner": [x, y, 0], "operation": "new"}});
    let existing: Vec<Value> = (0..50).map(|i| cube(2 * i, 0)).collect();
    tool(&mut s, "batch", json!({"commands": existing}));
    let more: Vec<Value> = (0..3).map(|i| cube(2 * i, 5)).collect();
    let r = raw_tool(&mut s, "batch", json!({"commands": more, "verbose": false}));
    assert!(r.get("error").is_none() && r["result"]["isError"] != json!(true), "{r}");
    let reply = r["result"]["content"][0]["text"].as_str().unwrap_or_default();
    assert_eq!(tool(&mut s, "measure", json!({}))["body_count"], 53);
    // The new bodies are named; the 50 untouched ones are not repeated.
    for new in ["Body51", "Body52", "Body53"] {
        assert!(reply.contains(&format!("\"{new}\"")), "{new} missing from the compact reply: {reply}");
    }
    for old in ["Body1", "Body25", "Body50"] {
        assert!(!reply.contains(&format!("\"{old}\"")), "untouched {old} repeated in the compact reply ({} bytes): {reply}", reply.len());
    }
}

/// A compact batch reply keeps what each step did: the body a cut changed, and a sketch
/// command's new ids once (under `created`); the default reply is unchanged.
#[test]
fn compact_batch_reply_keeps_changed_bodies_and_created_ids() {
    let mut s = server();
    let steps = json!([
        {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "Base"}},
        {"command": "solid.box", "params": {"length": 4, "width": 4, "height": 10, "corner": [3, 3, 5], "operation": "cut", "targets": ["Base"]}},
        {"command": "sketch.create", "params": {"plane": "XY"}},
        {"command": "sketch.rectangle.two_point", "params": {"p0": [20, 0], "p1": [30, 10]}},
    ]);
    let full = tool(&mut s, "batch", json!({"commands": steps}));
    assert_eq!(full["results"][1]["bodies"], json!(["Base"]), "{full}");
    assert_eq!(full["results"][3]["curves"], full["results"][3]["created"]["curves"], "{full}");
    tool(&mut s, "new_design", json!({}));
    let r = tool(&mut s, "batch", json!({"commands": steps, "verbose": false}));
    let cut = &r["results"][1];
    assert!(cut.get("bodies").is_none(), "{r}");
    assert_eq!(cut["changed_bodies"], json!(["Base"]), "{r}");
    let rect = &r["results"][3];
    assert!(rect.get("curves").is_none() && rect.get("constraints").is_none(), "{r}");
    assert_eq!(rect["created"]["curves"].as_array().map(Vec::len), Some(4), "{r}");
    assert!(rect["sides"].is_object(), "results that are not repeated stay: {r}");
}

/// #531: a product model keeps an optional variant hidden in its default presentation. Hiding
/// it must survive saving and reopening the design (suppressing it would change the design).
#[test]
fn hidden_body_stays_hidden_after_save_and_reopen() {
    let mut s = server();
    tool(&mut s, "execute", json!({"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "Base"}}));
    tool(
        &mut s,
        "execute",
        json!({"command": "solid.box", "params": {"length": 5, "width": 5, "height": 5, "corner": [20, 0, 0], "operation": "new", "body_name": "Variant"}}),
    );
    tool(&mut s, "execute", json!({"command": "browser.visibility", "params": {"items": ["Variant"], "visible": false}}));
    let visible = |s: &mut Server, body: &str| {
        let d = tool(s, "inspect_design", json!({"measure": false}));
        d["bodies"].as_array().into_iter().flatten().find(|b| b["name"] == body).map(|b| b["visible"].clone()).unwrap_or(Value::Null)
    };
    assert_eq!(visible(&mut s, "Variant"), false);

    let dir = std::env::temp_dir().join(format!("solidcraft-mcp-visibility-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let design = dir.join("variants.solidcraft");
    tool(&mut s, "save", json!({"path": design.to_string_lossy()}));
    tool(&mut s, "new_design", json!({"name": "Empty"}));
    tool(&mut s, "open", json!({"path": design.to_string_lossy()}));
    let (base, variant) = (visible(&mut s, "Base"), visible(&mut s, "Variant"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(base, true);
    assert_eq!(variant, false, "the hidden body is visible again after reopening");
}

/// Single execute calls share batch compaction without changing geometry or inspection.
#[test]
fn compact_execute_keeps_changes_ids_and_default_replies() {
    let mut s = server();
    let commands = json!([
        {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "Base"}},
        {"command": "solid.box", "params": {"length": 4, "width": 4, "height": 10, "corner": [3, 3, 5], "operation": "cut", "targets": ["Base"]}},
        {"command": "sketch.create", "params": {"plane": "XY"}},
        {"command": "sketch.rectangle.two_point", "params": {"p0": [20, 0], "p1": [30, 10]}}
    ]);
    let full: Vec<Value> = commands.as_array().unwrap().iter().map(|a| tool(&mut s, "execute", a.clone())).collect();
    assert_eq!(full[0]["bodies"], json!(["Base"]));
    assert_eq!(full[3]["curves"], full[3]["created"]["curves"]);
    let expected_volume = volume(&mut s);
    for verbose in [true, false] {
        tool(&mut s, "new_design", json!({}));
        for (a, expected) in commands.as_array().unwrap().iter().zip(&full) {
            let mut a = a.clone();
            a["verbose"] = json!(verbose);
            let mut actual = tool(&mut s, "execute", a);
            let mut expected = expected.clone();
            // Independent executions have different timings; the field must still be present.
            if expected.get("ms").is_some() {
                assert!(actual["ms"].is_number());
                actual.as_object_mut().unwrap().remove("ms");
                expected.as_object_mut().unwrap().remove("ms");
            }
            if !verbose {
                // Compare against the full result with only the documented repeated fields removed.
                let o = expected.as_object_mut().unwrap();
                if o.contains_key("created_bodies") {
                    o.remove("bodies");
                }
                for key in ["points", "curves", "constraints", "dimensions"] {
                    if o.get(key).is_some_and(|v| o.get("created").and_then(|c| c.get(key)) == Some(v)) {
                        o.remove(key);
                    }
                }
            }
            assert_eq!(actual, expected);
        }
        assert!((volume(&mut s) - expected_volume).abs() < 1e-9);
        let inspect = tool(&mut s, "execute", json!({"command": "sketch.inspect", "verbose": false}));
        assert_eq!(inspect["curves"].as_array().map(Vec::len), Some(4));
        let invalid = raw_tool(&mut s, "execute", json!({"command": "solid.box", "verbose": "false"}));
        assert_eq!(invalid["error"]["code"], -32602);
        assert!((volume(&mut s) - expected_volume).abs() < 1e-9);
        let error = raw_tool(&mut s, "execute", json!({"command": "not.a.command", "verbose": false}));
        assert_eq!(error["result"]["isError"], true);
    }
}

/// A connected app sees exactly one engine call; no inventory inspection is needed.
#[test]
fn compact_execute_uses_one_backend_call_and_keeps_removed_bodies() {
    struct ReplyOnly {
        calls: usize,
    }
    impl crate::backend::Backend for ReplyOnly {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
            assert_eq!(method, "engine.execute");
            assert_eq!(params, json!({"command": "test.feature", "params": {}}));
            self.calls += 1;
            Ok(
                json!({"feature": 7, "name": "Combine1", "bodies": ["Base"], "created_bodies": [], "changed_bodies": ["Base"], "removed_bodies": ["Tool"]}),
            )
        }
        fn has_ui(&self) -> bool {
            true
        }
        fn describe(&self) -> String {
            "test".into()
        }
    }
    let mut b = ReplyOnly { calls: 0 };
    let r = crate::tools::call_tool(&mut b, "execute", &json!({"command": "test.feature", "verbose": false}));
    let v: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(b.calls, 1);
    assert_eq!(v, json!({"feature": 7, "name": "Combine1", "created_bodies": [], "changed_bodies": ["Base"], "removed_bodies": ["Tool"]}));
}

/// The worked example in `skills/solidcraft-modeling/reference.md` is what agents copy, so it has
/// to keep building with the volume the file documents, and follow a parameter change.
#[test]
fn skill_reference_example_builds_as_documented() {
    let doc = include_str!("../../../skills/solidcraft-modeling/reference.md");
    let start = doc.find("{\"commands\":[").expect("the reference has a worked example");
    let end = start + doc[start..].find("```").expect("the example's code block ends");
    let example: Value = serde_json::from_str(&doc[start..end]).expect("the example is JSON");
    let mut s = server();
    tool(&mut s, "batch", example);
    // Plate with four r5 corners, minus four counterbored holes (Ø3.4 through, Ø6.5 × 3.3).
    let expected = |l: f64| {
        let (w, t) = (50.0, 6.0);
        let plate = l * w * t - 4.0 * 25.0 * (1.0 - PI / 4.0) * t;
        let hole = PI * 1.7 * 1.7 * (t - 3.3) + PI * 3.25 * 3.25 * 3.3;
        plate - 4.0 * hole
    };
    assert!((expected(80.0) - 23335.2).abs() < 0.05, "the reference states {}", expected(80.0));
    // Volume can't see where the holes are: each counterbore floor (z = T - 3.3) is centred on its
    // hole, at (±(L/2 - inset), ±(W/2 - inset)).
    let holes_at = |s: &mut Server, x: f64| {
        let topo = tool(s, "body_topology", json!({"body": "Body1"}));
        let mut floors: Vec<(i64, i64)> = topo["faces"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["centroid"].as_array())
            .filter(|c| (c[2].as_f64().unwrap() - 2.7).abs() < 1e-6)
            .map(|c| ((c[0].as_f64().unwrap() * 1000.0).round() as i64, (c[1].as_f64().unwrap() * 1000.0).round() as i64))
            .collect();
        floors.sort_unstable();
        let x = (x * 1000.0).round() as i64;
        assert_eq!(floors, vec![(-x, -17000), (-x, 17000), (x, -17000), (x, 17000)], "{topo}");
    };
    let m = tool(&mut s, "measure", json!({}));
    assert!(rel(m["total"]["volume_mm3"].as_f64().unwrap(), expected(80.0)) < 1e-5, "{m}");
    assert_eq!(m["bodies"][0]["faces"], json!(22), "{m}");
    holes_at(&mut s, 32.0);
    let r = tool(&mut s, "set_parameter", json!({"name": "L", "value": "120 mm"}));
    assert_eq!(r["errors"].as_array().map(Vec::len), Some(0), "{r}");
    assert!(rel(volume(&mut s), expected(120.0)) < 1e-5);
    holes_at(&mut s, 52.0);
}

/// The skill reference's sketch-plane table: where sketch (10, 20) lands and which way +distance goes.
#[test]
fn sketch_plane_axes_match_the_skill_reference() {
    let mut s = server();
    for plane in ["XY", "XZ", "YZ"] {
        tool(
            &mut s,
            "batch",
            json!({"commands": [
                {"command": "sketch.create", "params": {"plane": plane}},
                {"command": "sketch.circle.center", "params": {"center": [10, 20], "radius": 1}},
                {"command": "sketch.finish"},
                {"command": "solid.extrude", "params": {"distance": 3, "operation": "new"}},
            ]}),
        );
    }
    let m = tool(&mut s, "measure", json!({}));
    let centres: Vec<Vec<f64>> = m["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["center_of_mass"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect())
        .collect();
    let expected = [[10.0, 20.0, 1.5], [10.0, 1.5, -20.0], [1.5, 10.0, 20.0]];
    assert_eq!(centres.len(), 3, "{m}");
    for (got, want) in centres.iter().zip(expected) {
        assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6), "{got:?} != {want:?}");
    }
}

/// Every tool and command the agent skills call exists, so a rename or removal can't leave the
/// skills sending agents to names the server no longer has.
#[test]
fn skills_name_only_real_tools_and_commands() {
    let docs = [
        include_str!("../../../skills/solidcraft-modeling/SKILL.md"),
        include_str!("../../../skills/solidcraft-modeling/reference.md"),
        include_str!("../../../skills/solidcraft-editing/SKILL.md"),
    ];
    let mut s = server();
    let tools: Vec<String> = tool_definitions().iter().filter_map(|t| t["name"].as_str()).map(str::to_owned).collect();
    let commands: Vec<String> =
        tool(&mut s, "list_commands", json!({})).as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).map(str::to_owned).collect();
    let mut checked = 0;
    for doc in docs {
        // A call in backticks is `name {}`, `name {"json":…}` or `name {placeholder}`: a tool when the
        // name is snake_case, a command when it is a dotted id. Field lists such as
        // `sides {bottom,right,top,left}` and `created: {…}` aren't calls. JSON examples name
        // commands as "command":"id".
        let ident = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
        let calls = doc.split('`').skip(1).step_by(2).filter_map(|span| {
            let (head, args) = span.split_once(" {")?;
            let name = head.rsplit(' ').next()?;
            let args = args.strip_suffix('}')?;
            let call_like = args.is_empty() || args.starts_with('"') || args.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            (ident(name) && call_like).then_some(name)
        });
        let in_json = doc.split("\"command\":\"").skip(1).filter_map(|rest| rest.split('"').next());
        for name in calls.chain(in_json) {
            let known = if name.contains('.') { commands.iter().any(|c| c == name) } else { tools.iter().any(|t| t == name) };
            assert!(known, "a skill calls `{name}`, which is neither a tool nor a command");
            checked += 1;
        }
    }
    assert!(checked > 30, "only {checked} tool and command names found in the skills");
}

/// Every tool the server instructions name (a bare `snake_case` word in backticks) is a real tool.
#[test]
fn instructions_name_only_real_tools() {
    let mut s = server();
    let init = call(&mut s, 1, "initialize", json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": {}}));
    let text = init["result"]["instructions"].as_str().unwrap().to_owned();
    let tools: Vec<String> = tool_definitions().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
    let named: Vec<&str> =
        text.split('`').skip(1).step_by(2).filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '_')).collect();
    assert!(named.contains(&"body_topology"), "{named:?}");
    for name in named {
        assert!(tools.iter().any(|t| t == name), "the instructions name `{name}`, which is not a tool");
    }
}
