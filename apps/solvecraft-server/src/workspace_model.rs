//! Authoritative per-process model metadata and shared-file revision checks.
use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};

#[derive(Default, Clone)]
pub struct ModelStatus {
    pub project: Option<String>,
    pub current_file: Option<String>,
    pub dirty: Option<bool>,
    pub has_model: bool,
    pub revision: Option<u64>,
    pub loaded_revision: Option<String>,
    pub last_change: Option<Instant>,
}

pub fn relative(root: &Path, value: &str) -> Option<String> {
    let path = crate::paths::resolve(root, value).ok()?;
    let name = path.strip_prefix(root).ok()?.to_str()?.replace('\\', "/");
    crate::workspace_files::visible_name(&name).then_some(name)
}

impl ModelStatus {
    pub fn snapshot(&self) -> Value {
        json!({"project":self.project,"current_file":self.current_file,"dirty":self.dirty,"has_model":self.has_model,"model_revision":self.revision,
            "loaded_revision":self.loaded_revision,"last_model_change_seconds":self.last_change.map(|t|t.elapsed().as_secs())})
    }

    pub fn update(&mut self, root: &Path, status: &Value, refreshed: bool) {
        let next = status.get("current_file").and_then(Value::as_str).and_then(|s| relative(root, s));
        let revision = status.get("revision").and_then(Value::as_u64);
        if self.revision.is_some() && self.revision != revision {
            self.last_change = Some(Instant::now());
        }
        if refreshed {
            self.loaded_revision = next.as_deref().and_then(|name| crate::workspace_files::revision(root, name).ok().flatten());
        } else if self.current_file != next {
            // A failed batch may have replaced the current design before it stopped.
            // Never bless the current disk bytes as its base without a proven open/save.
            self.loaded_revision = None;
        }
        self.current_file = next;
        self.revision = revision;
        self.dirty = status.get("dirty").and_then(Value::as_bool);
        self.has_model = status.get("has_model").and_then(Value::as_bool).unwrap_or(false);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOperation {
    /// Linked-source reads share the file gate but never bless a save revision.
    ReadSources,
    Open(String),
    Save(String),
    Write(String),
}

/// Call only after a wholly successful worker request. Failed batches can leave
/// file writes and Session.path behind despite their document checkpoint rollback.
pub fn refreshes_current(ops: &[FileOperation], status: &Value, root: &Path) -> bool {
    let Some(current) = status.get("current_file").and_then(Value::as_str).and_then(|name| relative(root, name)) else { return false };
    ops.iter()
        .rev()
        .find_map(|op| match op {
            FileOperation::Open(name) | FileOperation::Save(name) if name == &current => Some(true),
            FileOperation::Write(name) if name == &current => Some(false),
            _ => None,
        })
        .unwrap_or(false)
}

/// Inspect executable tool steps in their actual order. This is a conservative
/// preflight: a failed step stops the worker batch, so later planned writes never run.
/// Paths have already passed the gateway's confinement before this function runs.
pub fn operations(root: &Path, message: &Value, current: Option<&str>) -> Result<Vec<FileOperation>, String> {
    fn name(root: &Path, args: &Value) -> Result<Option<String>, String> {
        args.get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(|path| relative(root, path).ok_or_else(|| "workspace file name is not permitted".to_string()))
            .transpose()
    }
    fn imports_as_new(path: &str) -> bool {
        Path::new(path).extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
            ["step", "stp", "igs", "iges", "fcstd", "3dm", "3mf", "stl", "obj", "glb", "gltf"].iter().any(|import| ext.eq_ignore_ascii_case(import))
        })
    }
    fn command(root: &Path, id: &str, args: &Value, current: &mut Option<String>, out: &mut Vec<FileOperation>) -> Result<(), String> {
        match id {
            "file.new" => *current = None,
            "component.insert_design" | "component.relink" | "component.update_links" => out.push(FileOperation::ReadSources),
            "doc.open" => {
                if let Some(path) = name(root, args)? {
                    *current = (!imports_as_new(&path)).then(|| path.clone());
                    out.push(FileOperation::Open(path));
                }
            }
            "file.save" | "file.save_as" => {
                let explicit = name(root, args)?;
                let target = if id == "file.save" { explicit.or_else(|| current.clone()) } else { explicit };
                if let Some(path) = target {
                    *current = Some(path.clone());
                    out.push(FileOperation::Save(path));
                }
            }
            "file.export" => {
                // URDF has a data-dependent set of sibling mesh destinations. Until the
                // worker can preflight that exact set, it cannot be protected by one ETag.
                let format = args.get("format").and_then(Value::as_str).or_else(|| args.get("path").and_then(Value::as_str));
                if format.is_some_and(|format| format.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("urdf"))) {
                    return Err("URDF multi-file export is not supported through the network gateway; export STEP or 3MF instead".into());
                }
                if let Some(path) = name(root, args)? {
                    out.push(FileOperation::Write(path));
                }
            }
            "sheet.export_dxf" => {
                if args.get("per_body").and_then(Value::as_bool) == Some(true) && name(root, args)?.is_some() {
                    return Err(
                        "per-body DXF file export is not supported through the network gateway; omit per_body or omit path to return DXF text".into(),
                    );
                }
                if let Some(path) = name(root, args)? {
                    out.push(FileOperation::Write(path));
                }
            }
            "motion.export_animation" => {
                if let Some(path) = name(root, args)? {
                    if !path.to_ascii_lowercase().ends_with(".gif") {
                        return Err("PNG frame-directory export is not supported through the network gateway; use a .gif path".into());
                    }
                    out.push(FileOperation::Write(path));
                }
            }
            "file.save_mesh"
            | "render.export"
            | "motion.export"
            | "parameters.export"
            | "timeline.export_script"
            | "sketch.export_dxf"
            | "sheet.export_bend_table"
            | "file.export_view_dxf"
            | "library.export"
            | "assembly.bom" => {
                if let Some(path) = name(root, args)? {
                    out.push(FileOperation::Write(path));
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut out = Vec::new();
    let mut current = current.map(str::to_owned);
    let args = message.pointer("/params/arguments").unwrap_or(&Value::Null);
    match message.pointer("/params/name").and_then(Value::as_str) {
        Some("open") => command(root, "doc.open", args, &mut current, &mut out)?,
        Some("save") => command(root, "file.save", args, &mut current, &mut out)?,
        Some("new_design") => command(root, "file.new", args, &mut current, &mut out)?,
        Some("export") => command(root, "file.export", args, &mut current, &mut out)?,
        Some("screenshot") => {
            if let Some(path) = name(root, args)? {
                out.push(FileOperation::Write(path));
            }
        }
        Some("execute") => {
            if let Some(id) = args.get("command").and_then(Value::as_str) {
                command(root, id, args.get("params").filter(|p| p.is_object()).unwrap_or(&Value::Null), &mut current, &mut out)?;
            }
        }
        Some("batch") => {
            if let Some(steps) = args.get("commands").and_then(Value::as_array) {
                if steps.len() > 1000 {
                    return Err("too many file operations".into());
                }
                for step in steps {
                    // Match the worker exactly: a present invalid command does not fall back to id.
                    if let Some(id) = step.get("command").or_else(|| step.get("id")).and_then(Value::as_str) {
                        command(root, id, step.get("params").unwrap_or(&Value::Null), &mut current, &mut out)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

pub fn check_revisions(root: &Path, status: &ModelStatus, ops: &[FileOperation], expected: &Value) -> Result<(), String> {
    if !expected.is_null() && (!expected.is_object() || expected.as_object().is_some_and(|o| o.len() > 1000)) {
        return Err("expected_revisions must be an object of relative paths and revisions".into());
    }
    let mut opened = std::collections::HashSet::new();
    for op in ops {
        match op {
            FileOperation::ReadSources => {}
            FileOperation::Open(name) => {
                opened.insert(name);
            }
            FileOperation::Save(name) | FileOperation::Write(name) => {
                let actual = crate::workspace_files::revision(root, name)?;
                let supplied = expected
                    .get(name)
                    .and_then(Value::as_str)
                    .or_else(|| (status.current_file.as_ref() == Some(name)).then_some(status.loaded_revision.as_deref()).flatten());
                if !opened.contains(name) && actual.as_deref() != supplied && (actual.is_some() || supplied.is_some()) {
                    return Err(format!(
                        "file revision conflict for {name}: reopen the current file or save under a new name; the session model is retained"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn same_size_replacement_conflicts_and_new_names_remain_available() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("part.solidcraft"), b"old").unwrap();
        let mut status = ModelStatus::default();
        status.update(&root, &json!({"current_file":root.join("part.solidcraft"),"revision":1,"dirty":false,"has_model":true}), true);
        std::fs::write(root.join("part.solidcraft"), b"new").unwrap();
        let ops = vec![FileOperation::Write("part.solidcraft".into())];
        assert!(check_revisions(&root, &status, &ops, &Value::Null).is_err());
        assert!(check_revisions(&root, &status, &[FileOperation::Write("copy.solidcraft".into())], &Value::Null).is_ok());
        let revision = crate::workspace_files::revision(&root, "part.solidcraft").unwrap();
        assert!(check_revisions(&root, &status, &ops, &json!({ "part.solidcraft": revision })).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nested_commands_and_implicit_save_are_guarded() {
        let root = std::env::temp_dir();
        let message = json!({"params":{"name":"batch","arguments":{"commands":[{"command":"doc.open","params":{"path":root.join("a.solidcraft")}}, {"command":"file.save_as","params":{"path":root.join("b.solidcraft")}}, {"command":"file.save"}]}}});
        let ops = operations(&root, &message, Some("current.solidcraft")).unwrap();
        assert_eq!(
            ops,
            vec![FileOperation::Open("a.solidcraft".into()), FileOperation::Save("b.solidcraft".into()), FileOperation::Save("b.solidcraft".into())]
        );
    }

    fn batch(steps: Value) -> Value {
        json!({"params":{"name":"batch","arguments":{"commands":steps}}})
    }

    #[test]
    fn aliases_and_ordered_implicit_save_follow_worker_semantics() {
        let root = std::env::temp_dir();
        let message = batch(json!([
            {"id":"doc.open","params":{"path":"opened.solidcraft"}},
            {"id":"file.save"},
            {"command":"file.save","params":{"path":"saved.solidcraft"}},
            {"command":"file.save"},
            {"command":"file.new"},
            {"command":"file.save"}
        ]));
        assert_eq!(
            operations(&root, &message, Some("old.solidcraft")).unwrap(),
            vec![
                FileOperation::Open("opened.solidcraft".into()),
                FileOperation::Save("opened.solidcraft".into()),
                FileOperation::Save("saved.solidcraft".into()),
                FileOperation::Save("saved.solidcraft".into()),
            ]
        );
        // Imported formats open as a new, unsaved design, even if a file was open.
        for extension in ["STEP", "3mf", "obj", "fcstd", "3dm", "gltf"] {
            let message = batch(json!([{"command":"doc.open","params":{"path":format!("import.{extension}")}}, {"command":"file.save"}]));
            assert_eq!(operations(&root, &message, Some("old.solidcraft")).unwrap(), vec![FileOperation::Open(format!("import.{extension}"))]);
        }
        // A present invalid command is not replaced by an id, matching the worker.
        assert!(operations(&root, &batch(json!([{"command":null,"id":"file.save"}])), Some("old.solidcraft")).unwrap().is_empty());
    }

    #[test]
    fn every_scalar_writer_is_guarded_and_derived_output_is_rejected() {
        let root = std::env::temp_dir();
        for command in [
            "file.export",
            "file.save_mesh",
            "render.export",
            "motion.export",
            "parameters.export",
            "timeline.export_script",
            "sketch.export_dxf",
            "sheet.export_dxf",
            "sheet.export_bend_table",
            "file.export_view_dxf",
            "library.export",
        ] {
            let message = json!({"params":{"name":"execute","arguments":{"command":command,"params":{"path":"output.png"}}}});
            assert_eq!(operations(&root, &message, None).unwrap(), vec![FileOperation::Write("output.png".into())], "{command}");
        }
        for (command, params) in [
            ("file.export", json!({"path":"robot.URDF"})),
            ("file.export", json!({"path":"robot.data","format":"urdf"})),
            ("sheet.export_dxf", json!({"path":"flat.dxf","per_body":true})),
            ("motion.export_animation", json!({"path":"frames"})),
        ] {
            let message = json!({"params":{"name":"execute","arguments":{"command":command,"params":params}}});
            assert!(operations(&root, &message, None).is_err(), "{command}");
        }
        let gif = json!({"params":{"name":"execute","arguments":{"command":"motion.export_animation","params":{"path":"motion.gif"}}}});
        assert_eq!(operations(&root, &gif, None).unwrap(), vec![FileOperation::Write("motion.gif".into())]);
        let text = json!({"params":{"name":"execute","arguments":{"command":"sheet.export_dxf","params":{"per_body":true}}}});
        assert!(operations(&root, &text, None).unwrap().is_empty());
        // Feature data is not another executable step.
        let feature = batch(json!([{"command":"timeline.add","params":{"feature":{"command":"file.save","params":{"path":"fake.solidcraft"}}}}]));
        assert!(operations(&root, &feature, None).unwrap().is_empty());
    }

    #[test]
    fn only_proven_open_or_save_refreshes_the_current_model_base() {
        let root = std::env::temp_dir();
        let status = json!({"current_file":root.join("part.solidcraft")});
        assert!(refreshes_current(&[FileOperation::Open("part.solidcraft".into())], &status, &root));
        assert!(refreshes_current(&[FileOperation::Save("part.solidcraft".into()), FileOperation::Write("picture.png".into())], &status, &root));
        assert!(!refreshes_current(&[FileOperation::Write("picture.png".into())], &status, &root));
        assert!(!refreshes_current(&[FileOperation::Open("part.solidcraft".into()), FileOperation::Write("part.solidcraft".into())], &status, &root));
        let mut model =
            ModelStatus { current_file: Some("old.solidcraft".into()), loaded_revision: Some("old-revision".into()), ..Default::default() };
        model.update(&root, &status, false);
        assert_eq!(model.current_file.as_deref(), Some("part.solidcraft"));
        assert_eq!(model.loaded_revision, None);
    }
    #[test]
    fn optional_upstream_bom_output_is_preflighted_as_one_revision_checked_file() {
        let root = std::env::temp_dir();
        let call = |params| json!({"params":{"name":"execute","arguments":{"command":"assembly.bom","params":params}}});
        assert!(operations(&root, &call(json!({})), None).unwrap().is_empty());
        let expected = vec![FileOperation::Write("Public/bom.csv".into())];
        assert_eq!(operations(&root, &call(json!({"path":root.join("Public/bom.csv")})), None).unwrap(), expected);
        assert!(crate::projects::check_writes("Other", &expected).is_err());
        crate::projects::check_writes("Public", &expected).unwrap();
    }
    #[test]
    fn linked_source_reads_take_the_gate_without_blessing_existing_output_revisions() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("source.solidcraft"), b"public fixture").unwrap();
        for command in ["component.insert_design", "component.relink", "component.update_links"] {
            let call = json!({"params":{"name":"execute","arguments":{"command":command,"params":{"path":root.join("source.solidcraft")}}}});
            assert_eq!(operations(&root, &call, None).unwrap(), vec![FileOperation::ReadSources]);
        }
        assert!(
            check_revisions(
                &root,
                &ModelStatus::default(),
                &[FileOperation::ReadSources, FileOperation::Write("source.solidcraft".into())],
                &Value::Null
            )
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
