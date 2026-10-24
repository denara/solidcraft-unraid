//! Explicit per-session project folders. This is organization within one trusted
//! workspace, not a security boundary between users sharing its Bearer token.
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub fn validate(name: &str) -> Result<(), String> {
    let base = name.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"]
            .iter()
            .any(|prefix| base.strip_prefix(prefix).is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")));
    if name.is_empty()
        || name.len() > 96
        || name.trim() != name
        || name.ends_with('.')
        || name.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || !crate::workspace_files::visible_name(name)
        || reserved
    {
        return Err("project name must be one visible folder name (1–96 UTF-8 bytes), without surrounding spaces, trailing dots, reserved names or path separators".into());
    }
    Ok(())
}

pub fn select(root: &Path, name: &str) -> Result<PathBuf, String> {
    validate(name)?;
    // Refuse spelling variants even on a case-sensitive development filesystem:
    // exports must keep the same identity when moved between supported hosts.
    for (index, entry) in std::fs::read_dir(root).map_err(|_| "project directory unavailable")?.enumerate() {
        if index >= 4096 {
            return Err("project directory scan limit reached".into());
        }
        let entry = entry.map_err(|_| "project directory unavailable")?;
        let existing = entry.file_name().to_string_lossy().into_owned();
        if existing != name && existing.eq_ignore_ascii_case(name) {
            return Err("a workspace entry with a different spelling already exists; use its exact name".into());
        }
    }
    let path = crate::paths::resolve(root, name)?;
    match std::fs::create_dir(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("project folder could not be created".into()),
    }
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "project folder unavailable")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("project name must identify a real folder".into());
    }
    Ok(path)
}

pub fn list(root: &Path) -> Result<Value, String> {
    let mut projects = Vec::new();
    let mut truncated = false;
    for (index, entry) in std::fs::read_dir(root).map_err(|_| "project directory unavailable")?.enumerate() {
        if index >= 4096 || projects.len() >= 256 {
            truncated = true;
            break;
        }
        let entry = entry.map_err(|_| "project directory unavailable")?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
        let kind = entry.file_type().map_err(|_| "project directory unavailable")?;
        if validate(&name).is_ok() && kind.is_dir() && !kind.is_symlink() {
            projects.push(name);
        }
    }
    projects.sort();
    Ok(json!({"projects":projects,"truncated":truncated}))
}

pub fn tools() -> [Value; 2] {
    [
        json!({"name":"project_list","description":"List real project folders in this shared workspace. Choose an existing exact name or create a new one using project_select before CAD tools.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        json!({"name":"project_select","description":"Required before CAD tools: bind this session to one real project folder. Creates it if missing. Cannot switch projects within a session; initialize a new session instead. CAD paths are relative to this folder; workspace_list_files paths and expected_revisions keys are workspace-relative.","inputSchema":{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":96,"description":"Exact project folder name; no slashes, hidden or reserved names."}},"required":["name"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
    ]
}

pub fn arguments(tool: &str, args: &Value) -> Result<(), String> {
    let object = args.as_object().ok_or("arguments must be an object")?;
    let valid = if tool == "project_select" { object.len() == 1 && object.get("name").is_some_and(Value::is_string) } else { object.is_empty() };
    if valid { Ok(()) } else { Err("project_select accepts only name (string); project_list accepts no arguments".into()) }
}

pub fn prepare(root: &Path, project: &str, message: &mut Value) -> Result<(), String> {
    let project_root = crate::paths::resolve(root, project)?;
    // Recheck on each request; do not follow a replaced project symlink.
    let metadata = std::fs::symlink_metadata(&project_root).map_err(|_| "selected project folder unavailable")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("selected project folder unavailable".into());
    }
    let tool = message.pointer("/params/name").and_then(Value::as_str).unwrap_or_default().to_owned();
    let Some(args) = message.get_mut("params").and_then(Value::as_object_mut).map(|params| params.entry("arguments").or_insert_with(|| json!({})))
    else {
        return Err("tool arguments unavailable".into());
    };
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    let legacy = args.as_object_mut().and_then(|args| args.remove("from_workspace"));
    if let Some(value) = legacy.as_ref()
        && (tool != "open" || !value.is_boolean())
    {
        return Err("from_workspace is a boolean option for open only".into());
    }
    let source_root = if legacy == Some(json!(true)) { root } else { &project_root };
    crate::paths::prepare_tool(source_root, &tool, args)
}

pub fn check_writes(project: &str, operations: &[crate::workspace_model::FileOperation]) -> Result<(), String> {
    for operation in operations {
        if let crate::workspace_model::FileOperation::Save(name) | crate::workspace_model::FileOperation::Write(name) = operation
            && !name.strip_prefix(project).is_some_and(|suffix| suffix.starts_with('/'))
        {
            return Err(
                "output must stay in the selected project; save an opened workspace source with an explicit project-relative path first".into()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::workspace_model::FileOperation;

    #[test]
    fn safe_names_and_existing_folders_are_not_silently_renamed() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("old.solidcraft"), b"unchanged").unwrap();
        for name in ["", ".", "..", ".private", "../escape", "a/b", "a\\b", "C:project", " name", "name ", "name.", "CON", "com1.txt", "x\nname"] {
            assert!(select(&root, name).is_err(), "{name:?}");
        }
        let path = select(&root, "Demo-Bracket").unwrap();
        assert!(path.is_dir());
        assert_eq!(select(&root, "Demo-Bracket").unwrap(), path);
        assert!(select(&root, "demo-bracket").is_err());
        assert!(select(&root, "old.solidcraft").is_err());
        assert_eq!(list(&root).unwrap()["projects"], json!(["Demo-Bracket"]));
        assert_eq!(std::fs::read(root.join("old.solidcraft")).unwrap(), b"unchanged");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gateway_tools_validate_their_declared_argument_schema() {
        assert!(arguments("project_select", &json!({"name":"Demo"})).is_ok());
        assert!(arguments("project_list", &json!({})).is_ok());
        for args in [Value::Null, json!([]), json!({}), json!({"name":1}), json!({"name":"Demo","from_workspace":true})] {
            assert!(arguments("project_select", &args).is_err());
        }
        assert!(arguments("project_list", &json!({"name":"Demo"})).is_err());
    }

    #[test]
    fn all_writes_including_implicit_legacy_saves_remain_project_bound() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        select(&root, "Demo").unwrap();
        let mut message = json!({"method":"tools/call","params":{"name":"batch","arguments":{"commands":[{"id":"file.save","params":{"path":"part.solidcraft"}},{"command":"render.export","params":{"path":"part.png"}}]}}});
        prepare(&root, "Demo", &mut message).unwrap();
        let ops = crate::workspace_model::operations(&root, &message, None).unwrap();
        assert_eq!(ops, vec![FileOperation::Save("Demo/part.solidcraft".into()), FileOperation::Write("Demo/part.png".into())]);
        check_writes("Demo", &ops).unwrap();
        assert!(check_writes("Demo", &[FileOperation::Save("legacy.solidcraft".into())]).is_err());
        assert!(check_writes("Demo", &[FileOperation::Write("Demo-other/part.png".into())]).is_err());
        let mut open = json!({"method":"tools/call","params":{"name":"open","arguments":{"path":"legacy.solidcraft","from_workspace":true}}});
        prepare(&root, "Demo", &mut open).unwrap();
        assert_eq!(open.pointer("/params/arguments/path"), Some(&json!(root.join("legacy.solidcraft"))));
        let save = json!({"params":{"name":"save","arguments":{}}});
        assert!(check_writes("Demo", &crate::workspace_model::operations(&root, &save, Some("legacy.solidcraft")).unwrap()).is_err());
        let mut escape =
            json!({"params":{"name":"batch","arguments":{"commands":[{"command":"file.save","params":{"path":"../escape.solidcraft"}}]}}});
        assert!(prepare(&root, "Demo", &mut escape).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_project_symlinks() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(std::env::temp_dir(), root.join("Escape")).unwrap();
        assert!(select(&root, "Escape").is_err());
        assert_eq!(list(&root).unwrap()["projects"], json!([]));
        std::fs::remove_file(root.join("Escape")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn normalized_tool_batches_and_component_extract_share_strong_write_revisions() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        let project = select(&root, "Demo").unwrap();
        std::fs::write(project.join("part.solidcraft"), b"existing").unwrap();
        for step in [
            json!({"command":"save","params":{"path":"part.solidcraft"}}),
            json!({"command":"execute","params":{"command":"component.extract","params":{"component":"Part","path":"part.solidcraft"}}}),
        ] {
            let mut message = json!({"params":{"name":"batch","arguments":{"commands":[step]}}});
            prepare(&root, "Demo", &mut message).unwrap();
            let ops = crate::workspace_model::operations(&root, &message, None).unwrap();
            assert_eq!(ops.len(), 1);
            assert!(matches!(&ops[0], FileOperation::Save(name) | FileOperation::Write(name) if name == "Demo/part.solidcraft"));
            check_writes("Demo", &ops).unwrap();
            assert!(crate::workspace_model::check_revisions(&root, &Default::default(), &ops, &Value::Null).is_err());
            let revision = crate::workspace_files::revision(&root, "Demo/part.solidcraft").unwrap().unwrap();
            crate::workspace_model::check_revisions(&root, &Default::default(), &ops, &json!({"Demo/part.solidcraft":revision})).unwrap();
            assert_eq!(std::fs::read(project.join("part.solidcraft")).unwrap(), b"existing");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
