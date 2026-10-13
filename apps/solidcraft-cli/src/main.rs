//! SolidCraft headless command line.
//!
//! ```text
//! solidcraft-cli run <script.json|design.solidcraft|part.step|mesh.3mf> [--in design|part.step|mesh.3mf] [--out FILE]... [--save FILE] [--quiet] [--allow-errors]
//! solidcraft-cli eval <script.json|design.solidcraft|part.step> (alias: inspect) measurements as JSON
//! solidcraft-cli bench-open <design.solidcraft | part.step> [--parse-only] loading timings as JSON lines
//! solidcraft-cli snapshot <script|design> --out shot.png [--width W] [--height H] [--view iso|front|top|…]
//! solidcraft-cli exec <command> [json-params]                 run one command on an empty design
//! solidcraft-cli commands                                     the command registry as JSON
//! solidcraft-cli recipe <recipe.json>                         translate an oracle recipe to a script
//! solidcraft-cli oracle <case-dir>...                         replay recipes and compare with expected.json
//! solidcraft-cli mcp [--in design|script] [--connect HOST:PORT] MCP server on stdio (docs/mcp.md)
//! solidcraft-cli licences                                     SolidCraft's licence and the third-party licences
//! solidcraft-cli icon --out F.png|F.ico [--size N]             the app icon (packaging; drawn, not a file)
//! ```
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod crashes;
mod fuzz;
mod oracle;
mod recipe;
mod spatial;

use std::process::ExitCode;

use serde_json::{Value, json};
#[path = "../../app_icon.rs"]
mod app_icon;

use solidcraft_engine::Session;
use solidcraft_engine::render::{StandardView, render_png};

const USAGE: &str = "usage:
  solidcraft-cli run <script.json|design.solidcraft|part.step> [--in design.solidcraft|part.step] [--out FILE]... [--save FILE] [--quiet] [--results] [--allow-errors]
  solidcraft-cli run --in <design.solidcraft|part.step> [--out FILE]...
      (a timeline feature that fails fails the run and nothing is written; --allow-errors writes anyway)
  solidcraft-cli eval <script.json|design.solidcraft|part.step>     (alias: inspect; or --in FILE)
  solidcraft-cli bench-open <design.solidcraft | part.step> [--parse-only]
  solidcraft-cli snapshot <script|design> --out shot.png [--width W] [--height H] [--view iso|front|back|top|bottom|left|right]
  solidcraft-cli exec <command> [json-params]
  solidcraft-cli commands
  solidcraft-cli recipe <recipe.json>
  solidcraft-cli oracle <case-dir>... [--json]
  solidcraft-cli fuzz [--from N] [--count N] [--jobs J] [--timeout S] [--steps N] [--minimise] [--out DIR] [--report FILE]
  solidcraft-cli mcp [--in design.solidcraft|script.json] [--connect 127.0.0.1:PORT]
  solidcraft-cli crashes [list [--json] | show <id> | replay <id> | dir | clear [<id>]]
  solidcraft-cli licences
  solidcraft-cli icon --out icon.png [--size 256] | --out icon.ico
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("run") => cmd_run(&args[1..]),
        Some("eval" | "inspect") => cmd_eval(&args[1..]),
        Some("snapshot") => cmd_snapshot(&args[1..]),
        Some("exec") => cmd_exec(&args[1..]),
        Some("commands") => {
            use std::io::Write as _;
            let mut s = Session::default();
            s.execute("engine.commands", &json!({})).map_err(|e| e.to_string()).and_then(|v| {
                // A pipeline consumer may stop early (e.g. `commands | head`).
                // Unlike println!, writing explicitly lets us handle that without a panic.
                match writeln!(std::io::stdout().lock(), "{}", pretty(&v)) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                    Err(e) => Err(format!("stdout: {e}")),
                }
            })
        }
        Some("recipe") => args.get(1).ok_or_else(|| USAGE.to_string()).and_then(|p| {
            let v = read_json(p)?;
            recipe::to_script(&v).map(|s| println!("{}", pretty(&s)))
        }),
        Some("oracle") => oracle::run(&args[1..]),
        Some("fuzz") => fuzz::run(&args[1..]),
        Some("fuzz-one") => fuzz::one(&args[1..]),
        Some("mcp") => cmd_mcp(&args[1..]),
        Some("crashes") => crashes::run(&args[1..]),
        Some("icon") => cmd_icon(&args[1..]),
        Some("licences" | "licenses") => {
            // Written, not print!ed: a closed pipe (`| head`) is not a crash.
            use std::io::Write as _;
            let _ = std::io::stdout().lock().write_all(solidcraft_engine::licences::all().as_bytes());
            Ok(())
        }
        // Hidden, for the kill-mid-save test: save a large design over and over.
        Some("save-stress") => cmd_save_stress(&args[1..]),
        // Hidden: time recomputes after parameter edits (docs/perf.md).
        Some("bench-edit") => cmd_bench_edit(&args[1..]),
        Some("bench-open") => cmd_bench_open(&args[1..]),
        Some("--version" | "-V") => {
            println!("solidcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

pub fn read_json(path: &str) -> Result<Value, String> {
    // Through the engine's file access, which refuses anything that is not a regular file and
    // reads no more than the cap: a character device or a FIFO would otherwise never end.
    let s = solidcraft_engine::io::vfs::read_to_string(path, solidcraft_engine::io::MAX_DESIGN_BYTES).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&s).map_err(|e| format!("{path}: {e}"))
}

/// A session from a design file, a STEP/3MF/STL file, a command script, or an oracle recipe.
pub fn load(path: &str) -> Result<Session, String> {
    let mut s = Session::default();
    if solidcraft_engine::io::is_step_path(path) || solidcraft_engine::io::is_mesh_path(path) {
        s.execute("doc.open", &json!({"path": path})).map_err(|e| e.to_string())?;
        return Ok(s);
    }
    let bytes = solidcraft_engine::io::vfs::read(path, solidcraft_engine::io::MAX_DESIGN_BYTES).map_err(|e| format!("{path}: {e}"))?;
    if solidcraft_engine::io::is_design_container(&bytes) {
        let doc = solidcraft_engine::io::read_design(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let mut session = Session::new(doc);
        session.path = Some(path.to_owned());
        return Ok(session);
    }
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))?;
    if v.get("format").and_then(Value::as_str).is_some_and(|f| solidcraft_engine::doc::format::version_of(f).is_some()) {
        s.execute("doc.open", &json!({"path": path})).map_err(|e| e.to_string())?;
    } else if v.get("features").is_some() && v.get("commands").is_none() {
        let script = recipe::to_script(&v)?;
        s.run_script(&script).map_err(|e| e.to_string())?;
    } else {
        s.run_script(&v).map_err(|e| e.to_string())?;
    }
    Ok(s)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn flags<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    args.windows(2).filter(|w| w[0] == name).map(|w| w[1].as_str()).collect()
}

/// The first argument that is not an option or an option's value.
fn positional(args: &[String]) -> Option<&String> {
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if a.starts_with("--") {
            i += if matches!(a.as_str(), "--quiet" | "--results" | "--allow-errors") { 1 } else { 2 };
            continue;
        }
        return Some(a);
    }
    None
}

fn cmd_run(args: &[String]) -> Result<(), String> {
    let base = flag(args, "--in");
    let path = positional(args);
    if path.is_none() && base.is_none() {
        return Err(USAGE.to_string());
    }
    // `--in` starts from a design or STEP file; the positional script then runs on top of it.
    let mut s = match base {
        Some(b) => load(b)?,
        None => Session::default(),
    };
    let results = if args.iter().any(|a| a == "--results") {
        // Collect every command's result (scripts only), then process requested files too.
        let v = read_json(path.ok_or("--results needs a script")?)?;
        let r = s.run_script(&v).map_err(|e| e.to_string())?;
        Some(Value::Array(r))
    } else {
        match (base, path) {
            (Some(_), Some(p)) => {
                let v = read_json(p)?;
                s.run_script(&v).map_err(|e| e.to_string())?;
            }
            (None, Some(p)) => s = load(p)?,
            _ => {}
        }
        None
    };
    // A feature that fails leaves the bodies as they were before it (a parameter change can
    // break a feature without failing the command that made it): writing them would hand out
    // the wrong part with success, so fail the run unless asked to write it anyway.
    if !args.iter().any(|a| a == "--allow-errors") {
        timeline_ok(&s)?;
    }
    for out in flags(args, "--out") {
        s.execute("file.export", &json!({"path": out})).map_err(|e| e.to_string())?;
    }
    if let Some(p) = flag(args, "--save") {
        s.execute("file.save_as", &json!({"path": p})).map_err(|e| e.to_string())?;
    }
    if let Some(results) = results {
        println!("{}", pretty(&results));
    } else if !args.iter().any(|a| a == "--quiet") {
        let m = s.execute("inspect.measure", &json!({})).map_err(|e| e.to_string())?;
        println!("{}", pretty(&m));
    }
    Ok(())
}

/// Every failing timeline feature, one per line, as an error; Ok when none fails.
fn timeline_ok(s: &Session) -> Result<(), String> {
    let failed: Vec<String> = s.model.results.iter().filter_map(|r| r.error.as_ref().map(|e| format!("  {}: {e}", r.name))).collect();
    if failed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} timeline feature(s) failed, so nothing was written (--allow-errors writes the bodies anyway):\n{}",
        failed.len(),
        failed.join("\n")
    ))
}

fn cmd_eval(args: &[String]) -> Result<(), String> {
    let path = flag(args, "--in").or_else(|| positional(args).map(String::as_str)).ok_or_else(|| USAGE.to_string())?;
    let mut s = load(path)?;
    let m = s.execute("inspect.measure", &json!({})).map_err(|e| e.to_string())?;
    let d = s.execute("document.inspect", &json!({})).map_err(|e| e.to_string())?;
    let out = json!({
        "body_count": m["body_count"],
        "bodies": m["bodies"],
        "total": m["total"],
        "timeline": d["timeline"],
        "params": d["params"],
        "sketches": d["sketches"],
    });
    println!("{}", pretty(&out));
    Ok(())
}

fn cmd_snapshot(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or_else(|| USAGE.to_string())?;
    let out = flag(args, "--out").ok_or("snapshot needs --out FILE.png")?;
    let w = flag(args, "--width").and_then(|x| x.parse().ok()).unwrap_or(1280usize);
    let h = flag(args, "--height").and_then(|x| x.parse().ok()).unwrap_or(800usize);
    let s = load(path)?;
    let mut cam = solidcraft_engine::view::home_camera(&s);
    if let Some(v) = flag(args, "--view") {
        cam.set_view(StandardView::parse(v).ok_or_else(|| format!("unknown view `{v}`"))?);
    }
    let scene = solidcraft_engine::view::scene(&s, &cam);
    let png = render_png(&scene, &cam, w, h).ok_or("render failed")?;
    std::fs::write(out, &png).map_err(|e| format!("{out}: {e}"))?;
    println!("{}", pretty(&json!({"path": out, "width": w, "height": h, "bytes": png.len()})));
    Ok(())
}

/// The app icon: the SolidCraft logo, drawn in code on a transparent background
/// (so packages need no committed image). `.ico` holds the Windows sizes as PNGs.
fn cmd_icon(args: &[String]) -> Result<(), String> {
    let out = flag(args, "--out").ok_or("icon needs --out FILE.png or FILE.ico")?;
    let size: usize = flag(args, "--size").and_then(|x| x.parse().ok()).unwrap_or(256).clamp(8, 2048);
    let bytes = if out.to_ascii_lowercase().ends_with(".ico") {
        let pngs: Vec<(usize, Vec<u8>)> =
            [16, 20, 24, 32, 40, 48, 64, 128, 256].into_iter().map(|s| icon_png(s).map(|p| (s, p))).collect::<Result<_, _>>()?;
        ico(&pngs)?
    } else {
        icon_png(size)?
    };
    std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    println!("{}", pretty(&json!({"path": out, "bytes": bytes.len()})));
    Ok(())
}

fn icon_png(size: usize) -> Result<Vec<u8>, String> {
    solidcraft_engine::render::png(size, size, app_icon::rgba(size)?).ok_or_else(|| "icon: PNG encoding failed".to_string())
}

/// A Windows icon file holding PNG images (Vista and later read these).
fn ico(images: &[(usize, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let n = u16::try_from(images.len()).map_err(|_| "too many icon sizes")?;
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&n.to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (size, data) in images {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&u32::try_from(data.len()).map_err(|_| "icon too large")?.to_le_bytes());
        out.extend_from_slice(&u32::try_from(offset).map_err(|_| "icon too large")?.to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        out.extend_from_slice(data);
    }
    Ok(out)
}

fn cmd_exec(args: &[String]) -> Result<(), String> {
    let id = args.first().ok_or_else(|| USAGE.to_string())?;
    let p: Value = match args.get(1) {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("params: {e}"))?,
        None => json!({}),
    };
    let mut s = Session::default();
    let v = s.execute(id, &p).map_err(|e| e.to_string())?;
    println!("{}", pretty(&v));
    Ok(())
}

/// MCP server on stdio: headless (optionally starting from a design or script) or bridged to
/// a running app's control port. stdout carries only protocol messages.
fn cmd_mcp(args: &[String]) -> Result<(), String> {
    let backend: Box<dyn solidcraft_mcp::Backend> = match (flag(args, "--connect"), flag(args, "--in")) {
        (Some(_), Some(_)) => return Err("mcp: use either --connect or --in, not both".into()),
        (Some(addr), None) => Box::new(solidcraft_mcp::Remote::connect(addr).map_err(|e| {
            let port = addr.rsplit(':').next().filter(|p| p.parse::<u16>().is_ok()).unwrap_or("PORT");
            format!(
                "cannot connect to {addr}: {e}\nno SolidCraft app is listening there; start it with \
                 `solidcraft --control {port}` (or SOLIDCRAFT_CONTROL_PORT={port}) and retry"
            )
        })?),
        (None, Some(path)) => Box::new(solidcraft_mcp::Headless::new(load(path)?)),
        (None, None) => Box::new(solidcraft_mcp::Headless::default()),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    solidcraft_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}

/// `save-stress <path> <rounds>`: save a design of a few hundred kilobytes `rounds` times, its
/// parameter `save_round` set to the round number each time.
fn cmd_save_stress(args: &[String]) -> Result<(), String> {
    let (Some(path), Some(rounds)) = (args.first(), args.get(1).and_then(|n| n.parse::<u64>().ok())) else {
        return Err("usage: solidcraft-cli save-stress <path> <rounds>".into());
    };
    let mut s = Session::default();
    let filler = (0..3000).map(|i| solidcraft_engine::doc::Param {
        name: format!("p{i}"),
        expr: format!("{i} mm + 0.5 mm"),
        unit: "mm".into(),
        comment: "filler to make the file large".into(),
        model: false,
    });
    s.doc_mut().params.extend(filler);
    for r in 0..rounds {
        s.doc_mut().set_param("save_round", &r.to_string(), None, None).map_err(|e| e.to_string())?;
        s.execute("file.save", &json!({ "path": path })).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// `bench-edit <script.json|recipe.json> <param> <expr>`: build the design, then time a
/// parameter edit, setting it back, undo and redo (ms and features recomputed each).
fn cmd_bench_edit(args: &[String]) -> Result<(), String> {
    let (Some(path), Some(param), Some(expr)) = (args.first(), args.get(1), args.get(2)) else {
        return Err("usage: solidcraft-cli bench-edit <script.json|recipe.json> <param> <expr>".into());
    };
    let v = read_json(path)?;
    let script = if v.get("features").is_some() { recipe::to_script(&v)? } else { v };
    let mut s = Session::default();
    let t = std::time::Instant::now();
    s.run_script(&script).map_err(|e| e.to_string())?;
    let build = t.elapsed().as_secs_f64() * 1000.0;
    let old = s.doc.all_param_exprs().into_iter().find(|(n, _, _)| n == param).map(|(_, e, _)| e).ok_or(format!("no parameter `{param}`"))?;
    let mut out = vec![json!({"step": "build", "ms": build, "features": s.doc.features.len()})];
    let mut time = |s: &mut Session, step: &str, id: &str, p: Value| -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = s.execute(id, &p).map_err(|e| format!("{step}: {e}"))?;
        let n = r.get("recomputed").cloned().unwrap_or_else(|| json!(s.model.last_recomputed));
        out.push(json!({"step": step, "ms": t.elapsed().as_secs_f64() * 1000.0, "recomputed": n}));
        Ok(())
    };
    let expr = expr.replace("{old}", &old);
    time(&mut s, "edit", "parameters.change", json!({"name": param, "expression": expr}))?;
    time(&mut s, "set back", "parameters.change", json!({"name": param, "expression": old}))?;
    time(&mut s, "undo", "edit.undo", json!({}))?;
    time(&mut s, "redo", "edit.redo", json!({}))?;
    println!("{}", pretty(&json!(out)));
    Ok(())
}

/// Separate file reading, JSON decoding/repair, timeline evaluation and display meshing.
fn cmd_bench_open(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("usage: solidcraft-cli bench-open <design.solidcraft | part.step> [--parse-only]")?;
    let t = std::time::Instant::now();
    let step = solidcraft_engine::io::is_step_path(path);
    let limit = if step { solidcraft_engine::io::MAX_STEP_BYTES } else { solidcraft_engine::io::MAX_DESIGN_BYTES };
    let bytes = solidcraft_engine::io::vfs::read(path, limit).map_err(|e| format!("{path}: {e}"))?;
    let read_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("{}", json!({"step": "read", "ms": read_ms, "bytes": bytes.len()}));
    let t = std::time::Instant::now();
    let doc = if step {
        let imported = solidcraft_engine::io::step_import_feature(&bytes, path).map_err(|e| e.to_string())?;
        let mut doc = solidcraft_engine::doc::Document::new(&imported.name);
        doc.add_feature(imported.kind, Some(&imported.name)).map_err(|e| e.to_string())?;
        doc
    } else {
        solidcraft_engine::io::read_design(&bytes).map_err(|e| e.to_string())?
    };
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("{}", json!({"step": "parse", "ms": parse_ms, "features": doc.features.len()}));
    if args.iter().any(|a| a == "--parse-only") {
        return Ok(());
    }
    let t = std::time::Instant::now();
    let s = Session::new(doc);
    println!("{}", json!({"step": "evaluate", "ms": t.elapsed().as_secs_f64() * 1000.0}));
    for r in &s.model.results {
        println!("{}", json!({"step": "feature", "id": r.id, "name": r.name, "ms": r.ms, "error": r.error}));
    }
    let t = std::time::Instant::now();
    let st = s.world_state();
    let triangles: usize = st.bodies.iter().map(|b| b.mesh().triangles.len()).sum();
    println!("{}", json!({"step": "mesh", "ms": t.elapsed().as_secs_f64() * 1000.0, "bodies": st.bodies.len(), "triangles": triangles}));
    Ok(())
}
