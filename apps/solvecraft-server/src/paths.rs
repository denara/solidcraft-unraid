//! Client file parameters are relative to the persistent files directory.
//! Geometry paths (arrays or scalar curve names) are passed through unchanged.
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

pub fn resolve(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.len() > 1024 || name.contains(['\\', ':', '\0']) {
        return Err("file path must be a relative path inside the files directory".into());
    }
    // Engine replies expose their resolved path; accept that same path on reopen,
    // but never accept an absolute path outside the workspace.
    let path = Path::new(name);
    let relative = if path.is_absolute() { path.strip_prefix(root).map_err(|_| "file path is outside the files directory")? } else { path };
    if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("absolute paths and parent traversal are not allowed".into());
    }
    if relative.to_str().is_none_or(|name| !crate::workspace_files::visible_name(name)) {
        return Err("hidden, temporary and reserved file names are not allowed".into());
    }
    let mut out = root.to_path_buf();
    for component in relative.components() {
        out.push(component.as_os_str());
        match std::fs::symlink_metadata(&out) {
            Ok(meta) if meta.file_type().is_symlink() => return Err("symlinks are not allowed in file paths".into()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(out)
}

fn file_fields(root: &Path, value: &mut Value) -> Result<(), String> {
    if let Value::Object(object) = value {
        for key in ["path", "dir", "font_path"] {
            if let Some(item) = object.get_mut(key)
                && let Some(name) = item.as_str()
            {
                *item = Value::String(resolve(root, name)?.to_string_lossy().into_owned());
            }
        }
    }
    Ok(())
}

/// Renderer inputs may nest file sources in environments, textures and typed jobs.
fn render_file_fields(root: &Path, value: &mut Value, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("render arguments are nested too deeply".into());
    }
    if let Value::Object(object) = value {
        for key in ["path", "dir", "font_path"] {
            if let Some(item) = object.get_mut(key)
                && let Some(name) = item.as_str()
            {
                if key == "path"
                    && let Some(builtin) = name.strip_prefix("builtin://")
                {
                    // A closed registry lookup never opens a filesystem path.
                    if builtin.is_empty() || builtin.len() > 256 || builtin.chars().any(|c| !c.is_ascii_alphanumeric() && !"/_-".contains(c)) {
                        return Err("invalid built-in render resource identifier".into());
                    }
                } else {
                    *item = Value::String(resolve(root, name)?.to_string_lossy().into_owned());
                }
            }
        }
    }
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                render_file_fields(root, child, depth + 1)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                render_file_fields(root, child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn prepare_tool(root: &Path, tool: &str, value: &mut Value) -> Result<(), String> {
    match tool {
        "crash_reports" => Err("shared desktop crash reports are unavailable through the network gateway".into()),
        "open" | "save" | "export" | "screenshot" => file_fields(root, value),
        "execute" => commands(root, value, 0),
        "batch" => {
            // The MCP worker accepts `id` as a batch-step alias for `command`.
            // Normalize only actual steps, never arbitrary geometry/feature IDs.
            if let Some(steps) = value.get_mut("commands").and_then(Value::as_array_mut) {
                if steps.len() > 1000 {
                    return Err("too many batch commands".into());
                }
                for step in steps {
                    normalize_batch_step(step)?;
                }
            }
            commands(root, value, 0)
        }
        _ => Ok(()),
    }
}

/// Normalize the worker's tool aliases before confinement and revision preflight.
/// Only actual batch steps are changed; geometry/feature IDs remain data.
fn normalize_batch_step(step: &mut Value) -> Result<(), String> {
    let Some(object) = step.as_object_mut() else { return Ok(()) };
    let Some(command) = object.get("command").or_else(|| object.get("id")).and_then(Value::as_str).map(str::to_owned) else { return Ok(()) };
    let args = object.get("params").cloned().unwrap_or(Value::Null);
    let (mapped, params) = match command.as_str() {
        "crash_reports" => return Err("shared desktop crash reports are unavailable through the network gateway".into()),
        "execute" => {
            let inner = args.get("command").and_then(Value::as_str).ok_or("an execute batch step needs command")?;
            if !inner.contains('.') {
                return Err("an execute batch step needs an engine command, not a tool alias".into());
            }
            (inner, args.get("params").filter(|p| p.is_object()).cloned().unwrap_or_else(|| serde_json::json!({})))
        }
        "save" | "open" | "export" | "new_design" => {
            let (mapped, keys): (&str, &[&str]) = match command.as_str() {
                "save" => ("file.save", &["path", "format", "overwrite"]),
                "open" => ("doc.open", &["path", "up", "names"]),
                "export" => ("file.export", &["path", "format", "bodies", "mesh_prefix", "effort", "velocity"]),
                _ => ("file.new", &["name"]),
            };
            let params = keys.iter().filter_map(|key| args.get(*key).cloned().map(|value| ((*key).to_owned(), value))).collect();
            (mapped, Value::Object(params))
        }
        _ => (command.as_str(), args),
    };
    object.insert("command".into(), Value::String(mapped.to_owned()));
    object.insert("params".into(), params);
    Ok(())
}

fn commands(root: &Path, value: &mut Value, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("tool arguments are nested too deeply".into());
    }
    match value {
        Value::Object(object) => {
            // Recovery metadata can refer to arbitrary original file paths. Network clients
            // use explicit open/save instead; desktop recovery remains unchanged.
            if object.get("command").and_then(Value::as_str).is_some_and(|c| c.starts_with("doc.recover") || c.starts_with("diagnostics.crash")) {
                return Err("desktop recovery commands are unavailable through the network gateway; use open/save".into());
            }
            if object.get("command").and_then(Value::as_str) == Some("sketch.text")
                && let Some(font) = object.get_mut("params").and_then(|p| p.get_mut("font"))
                && let Some(name) = font.as_str()
            {
                let name = name.trim();
                let lower = name.to_ascii_lowercase();
                if name.contains(['/', '\\']) || [".ttf", ".otf", ".ttc", ".otc"].iter().any(|ext| lower.ends_with(ext)) {
                    if name.contains(['%', '~']) {
                        return Err("font path expansion is unavailable through the network gateway".into());
                    }
                    *font = Value::String(resolve(root, name)?.to_string_lossy().into_owned());
                }
            }
            // Sweep/pattern paths, construction references and occurrence paths refer
            // to geometry and may be scalar strings. Decal reads a file instead.
            let geometry = object.get("command").and_then(Value::as_str).is_some_and(|c| {
                (c.starts_with("solid.") && c != "solid.decal")
                    || c.starts_with("construct.")
                    || (c.starts_with("component.")
                        && !matches!(c, "component.insert_design" | "component.relink" | "component.extract" | "component.replace_with_link"))
                    || c.starts_with("occurrence.")
            });
            let render = object.get("command").and_then(Value::as_str).is_some_and(|id| id.starts_with("render."));
            if !geometry
                && object.get("command").is_some()
                && let Some(params) = object.get_mut("params")
            {
                if render {
                    render_file_fields(root, params, 0)?;
                } else {
                    file_fields(root, params)?;
                }
            }
            for item in object.values_mut() {
                commands(root, item, depth + 1)?;
            }
        }
        Value::Array(array) => {
            for item in array {
                commands(root, item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_traversal_and_rewrites_nested_file_calls() {
        let root = std::env::temp_dir();
        for path in ["../secret", "/etc/passwd", "C:\\secret", "foo/../../secret", "", "a\\b", "x\0y"] {
            assert!(resolve(&root, path).is_err(), "{path:?}");
        }
        let mut batch = json!({"commands":[{"command":"file.save","params":{"path":"part.solvecraft"}},
            {"command":"component.activate","params":{"path":[1,2]}}]});
        prepare_tool(&root, "batch", &mut batch).unwrap();
        assert_eq!(batch["commands"][0]["params"]["path"], root.join("part.solvecraft").to_string_lossy().as_ref());
        assert_eq!(batch["commands"][1]["params"]["path"], json!([1, 2]));
        assert!(prepare_tool(&root, "execute", &mut json!({"command":"doc.recover"})).is_err());
        let mut geometry = json!({"command":"solid.pattern.path","params":{"path":"l1"}});
        prepare_tool(&root, "execute", &mut geometry).unwrap();
        assert_eq!(geometry["params"]["path"], "l1");
        assert!(prepare_tool(&root, "screenshot", &mut json!({"path":"../escape","command":"solid.sweep"})).is_err());
        assert!(prepare_tool(&root, "execute", &mut json!({"command":"file.save","params":{"path":"../escape","command":"solid.sweep"}})).is_err());
    }

    #[test]
    fn confines_new_font_paths_and_disallows_shared_crash_reports() {
        let root = std::env::temp_dir();
        for name in ["../secret.ttf", "/etc/passwd", "%HOME%/font.ttf", "~/font.otf", "C:\\font.ttf"] {
            assert!(prepare_tool(&root, "execute", &mut json!({"command":"sketch.text","params":{"font":name}})).is_err());
        }
        let mut text = json!({"command":"sketch.text","params":{"font":"fonts/part.ttf"}});
        prepare_tool(&root, "execute", &mut text).unwrap();
        assert_eq!(text["params"]["font"], root.join("fonts/part.ttf").to_string_lossy().as_ref());
        let mut family = json!({"command":"sketch.text","params":{"font":"Liberation Sans"}});
        prepare_tool(&root, "execute", &mut family).unwrap();
        assert_eq!(family["params"]["font"], "Liberation Sans");
        assert!(prepare_tool(&root, "crash_reports", &mut json!({})).is_err());
        assert!(prepare_tool(&root, "batch", &mut json!({"commands":[{"command":"diagnostics.crash_reports.clear"}]})).is_err());
        assert!(prepare_tool(&root, "execute", &mut json!({"command":"component.insert_design","params":{"path":"/etc/passwd"}})).is_err());
        let mut link = json!({"command":"component.insert_design","params":{"path":"part.solidcraft"}});
        prepare_tool(&root, "execute", &mut link).unwrap();
        assert_eq!(link["params"]["path"], root.join("part.solidcraft").to_string_lossy().as_ref());
    }

    #[test]
    fn batch_aliases_get_the_same_path_and_recovery_checks() {
        let root = std::env::temp_dir();
        let mut save = json!({"commands":[{"id":"file.save","params":{"path":"part.solidcraft"}}]});
        prepare_tool(&root, "batch", &mut save).unwrap();
        assert_eq!(save["commands"][0]["command"], "file.save");
        assert_eq!(save["commands"][0]["params"]["path"], root.join("part.solidcraft").to_string_lossy().as_ref());
        for alias in ["save", "file.save"] {
            let mut overwrite = json!({"commands":[{"id":alias,"params":{"path":"part.sc","overwrite":true}}]});
            prepare_tool(&root, "batch", &mut overwrite).unwrap();
            assert_eq!(overwrite["commands"][0]["params"]["overwrite"], true);
            assert_eq!(overwrite["commands"][0]["params"]["path"], json!(root.join("part.sc")));
        }
        for (command, params) in [("doc.recover", json!({})), ("file.save", json!({"path":"../escape.solidcraft"}))] {
            assert!(prepare_tool(&root, "batch", &mut json!({"commands":[{"id":command,"params":params}]})).is_err());
        }
        // A string feature ID nested in data must not turn into a command.
        let mut data = json!({"commands":[{"command":"timeline.add","params":{"feature":{"id":"file.save"}}}]});
        prepare_tool(&root, "batch", &mut data).unwrap();
        assert!(data["commands"][0]["params"]["feature"].get("command").is_none());
    }

    #[test]
    fn hidden_temporary_and_backup_names_are_reserved() {
        let root = std::env::temp_dir();
        for name in [".private", "folder/.private", "part.tmp", "part.part", "part.upload", "part.solidcraft~", "part.solidcraft.bak", "part.BAK"] {
            assert!(resolve(&root, name).is_err(), "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_files_and_directories() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink("/tmp", root.join("escape")).unwrap();
        assert!(resolve(&root, "escape/file").is_err());
        std::fs::remove_file(root.join("escape")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn upstream_relink_uses_a_confined_file_path_instead_of_a_geometry_path() {
        let root = std::env::temp_dir();
        let mut call = json!({"command":"component.relink","params":{"component":"Part","path":"parts/source.solidcraft"}});
        commands(&root, &mut call, 0).unwrap();
        assert_eq!(call["params"]["path"], json!(root.join("parts/source.solidcraft")));
        let mut escape = json!({"command":"component.relink","params":{"component":"Part","path":"../secret.solidcraft"}});
        assert!(commands(&root, &mut escape, 0).is_err());
    }
    #[test]
    fn worker_tool_aliases_receive_confinement_and_recovery_checks_before_execution() {
        let root = std::env::temp_dir();
        for (command, params) in [
            ("save", json!({"path":"../outside.solidcraft"})),
            ("open", json!({"path":"/etc/passwd"})),
            ("execute", json!({"command":"component.extract","params":{"path":"../part.solidcraft"}})),
            ("execute", json!({"command":"diagnostics.crash_reports.clear"})),
            ("crash_reports", json!({})),
            ("component.replace_with_link", json!({"path":"../part.solidcraft"})),
        ] {
            assert!(prepare_tool(&root, "batch", &mut json!({"commands":[{"command":command,"params":params}]})).is_err(), "{command}");
        }
        let mut batch = json!({"commands":[{"id":"save","params":{"path":"part.solidcraft"}},
            {"command":"execute","params":{"command":"component.extract","params":{"component":"Part","path":"extract.solidcraft"}}}]});
        prepare_tool(&root, "batch", &mut batch).unwrap();
        assert_eq!(batch["commands"][0]["command"], "file.save");
        assert_eq!(batch["commands"][0]["params"]["path"], json!(root.join("part.solidcraft")));
        assert_eq!(batch["commands"][1]["command"], "component.extract");
        assert_eq!(batch["commands"][1]["params"]["path"], json!(root.join("extract.solidcraft")));
    }
    #[test]
    fn nested_render_sources_follow_the_project_scope_in_direct_and_alias_batches() {
        let root = std::env::temp_dir();
        let mut direct = json!({"command":"render.start","params":{"environment":{"kind":"hdr","path":"sky.hdr"},
            "raytrace":{"textures":[{"path":"colour.png"}]}}});
        prepare_tool(&root, "execute", &mut direct).unwrap();
        assert_eq!(direct["params"]["environment"]["path"], json!(root.join("sky.hdr")));
        assert_eq!(direct["params"]["raytrace"]["textures"][0]["path"], json!(root.join("colour.png")));
        let mut builtin = json!({"command":"render.preflight","params":{"raytrace":{"textures":[{"path":"builtin://SURF-001/base_color"}]}}});
        prepare_tool(&root, "execute", &mut builtin).unwrap();
        assert_eq!(builtin["params"]["raytrace"]["textures"][0]["path"], "builtin://SURF-001/base_color");
        for path in ["../secret.hdr", "/etc/passwd", "folder/.private", "colour.png.bak"] {
            let mut batch = json!({"commands":[{"command":"execute","params":{"command":"render.preflight","params":{"settings":{"job":{"options":{"environment":{"path":path}}}}}}}]});
            assert!(prepare_tool(&root, "batch", &mut batch).is_err(), "{path}");
        }
    }
}
