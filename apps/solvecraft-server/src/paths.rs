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

pub fn prepare_tool(root: &Path, tool: &str, value: &mut Value) -> Result<(), String> {
    match tool {
        "open" | "save" | "export" | "screenshot" => file_fields(root, value),
        "execute" | "batch" => commands(root, value, 0),
        _ => Ok(()),
    }
}

fn commands(root: &Path, value: &mut Value, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("tool arguments are nested too deeply".into());
    }
    match value {
        Value::Object(object) => {
            // Recovery metadata can refer to arbitrary original file paths. Network clients
            // use explicit open/save instead; desktop recovery remains unchanged.
            if object.get("command").and_then(Value::as_str).is_some_and(|c| c.starts_with("doc.recover")) {
                return Err("desktop recovery commands are unavailable through the network gateway; use open/save".into());
            }
            // Sweep/pattern paths, construction references and occurrence paths refer
            // to geometry and may be scalar strings. Decal reads a file instead.
            let geometry = object.get("command").and_then(Value::as_str).is_some_and(|c| {
                (c.starts_with("solid.") && c != "solid.decal")
                    || c.starts_with("construct.")
                    || c.starts_with("component.")
                    || c.starts_with("occurrence.")
            });
            if !geometry
                && object.get("command").is_some()
                && let Some(params) = object.get_mut("params")
            {
                file_fields(root, params)?;
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
}
