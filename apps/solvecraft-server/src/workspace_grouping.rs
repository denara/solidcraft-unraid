//! Existing top-level project folders take precedence over virtual root groups.
//! Legacy root exports can follow an exact native-model stem plus a separator;
//! no directory is created, and unrelated common prefixes never form a group.
use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

pub(super) fn format(name: &str) -> String {
    let extension = Path::new(name).extension().and_then(|part| part.to_str()).unwrap_or_default();
    match extension.to_ascii_lowercase().as_str() {
        "solidcraft" | "solvecraft" => "SolidCraft".into(),
        "stp" | "step" => "STEP".into(),
        "jpeg" | "jpg" => "JPEG".into(),
        "" => "File".into(),
        _ => extension.to_uppercase(),
    }
}

pub(super) fn assign(files: &mut [Value]) {
    // Exact, case-sensitive stems preserve distinct files on case-sensitive volumes.
    let mut roots: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for file in files.iter() {
        let Some(name) = file.get("name").and_then(Value::as_str) else { continue };
        if name.contains('/') || !matches!(file.get("kind").and_then(Value::as_str), Some("model" | "export")) {
            continue;
        }
        let Some(stem) = Path::new(name).file_stem().and_then(|part| part.to_str()) else { continue };
        let entry = roots.entry(stem.to_owned()).or_default();
        entry.0 += 1;
        entry.1 += usize::from(file.get("kind").and_then(Value::as_str) == Some("model"));
    }
    for file in files {
        let Some(name) = file.get("name").and_then(Value::as_str) else { continue };
        let project = if let Some((directory, _)) = name.split_once('/') {
            json!({"id":format!("directory:{directory}"),"name":directory,"basis":"directory"})
        } else {
            let stem = Path::new(name).file_stem().and_then(|part| part.to_str()).unwrap_or(name);
            let kind = file.get("kind").and_then(Value::as_str).unwrap_or_default();
            if let Some((group, basis)) = root_group(stem, kind, &roots) {
                json!({"id":format!("stem:{group}"),"name":group,"basis":basis})
            } else {
                json!({"id":"ungrouped","name":"Other files","basis":"ungrouped"})
            }
        };
        file["project"] = project;
    }
}

fn root_group<'a>(stem: &'a str, kind: &str, roots: &'a BTreeMap<String, (usize, usize)>) -> Option<(&'a str, &'static str)> {
    if !matches!(kind, "model" | "export") {
        return None;
    }
    let exact = roots.get(stem);
    if exact.is_some_and(|(_, models)| *models > 0) {
        return Some((stem, "design_stem"));
    }
    if kind == "export" {
        // Pick the longest complete model stem, never a prefix of a word/number.
        // Two native files with the same stem make a suffixed association ambiguous.
        let candidate = roots
            .iter()
            .filter(|(model, (_, count))| {
                *count > 0
                    && stem
                        .strip_prefix(model.as_str())
                        .is_some_and(|suffix| suffix.starts_with(['-', '_']) && suffix.get(1..).is_some_and(|tail| !tail.is_empty()))
            })
            .max_by_key(|(model, _)| model.len());
        if let Some((model, (_, count))) = candidate {
            return (*count == 1).then_some((model.as_str(), "design_stem"));
        }
    }
    exact.filter(|(count, _)| *count > 1).map(|_| (stem, "file_stem"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_existing_project_folders_and_case_sensitive_model_stems() {
        let mut files = vec![
            json!({"name":"house.solidcraft","kind":"model"}),
            json!({"name":"house.step","kind":"export"}),
            json!({"name":"house_render.png","kind":"export"}),
            json!({"name":"House.stl","kind":"export"}),
            json!({"name":"other/deep/a.step","kind":"export"}),
            json!({"name":"other/deep/b.solidcraft","kind":"model"}),
            json!({"name":"loose.stl","kind":"export"}),
            json!({"name":"loose.3mf","kind":"export"}),
            json!({"name":"house.txt","kind":"other"}),
        ];
        assign(&mut files);
        assert_eq!(files[0]["project"], files[1]["project"]);
        assert_eq!(files[2]["project"], files[0]["project"]);
        assert_eq!(files[3]["project"]["basis"], "ungrouped");
        assert_eq!(files[4]["project"], files[5]["project"]);
        assert_eq!(files[4]["project"]["name"], "other");
        assert_eq!(files[6]["project"], files[7]["project"]);
        assert_eq!(files[6]["project"]["basis"], "file_stem");
        assert_eq!(files[8]["project"]["basis"], "ungrouped");
    }

    #[test]
    fn suffixed_exports_choose_the_longest_native_stem_and_exact_matches_win() {
        let mut files = vec![
            json!({"name":"bracket.solidcraft","kind":"model"}),
            json!({"name":"bracket-southeast.png","kind":"export"}),
            json!({"name":"bracket-comparison.png","kind":"export"}),
            json!({"name":"bracket-detail.solvecraft","kind":"model"}),
            json!({"name":"bracket-detail-southeast.png","kind":"export"}),
            json!({"name":"bracket-detail.png","kind":"export"}),
            json!({"name":"bracket2-comparison.png","kind":"export"}),
            json!({"name":"Bracket-comparison.png","kind":"export"}),
            json!({"name":"bracket-.png","kind":"export"}),
            json!({"name":"bracket-notes.txt","kind":"other"}),
            json!({"name":"no-model-front.png","kind":"export"}),
            json!({"name":"no-model-back.png","kind":"export"}),
        ];
        assign(&mut files);
        assert_eq!(files[0]["project"], files[1]["project"]);
        assert_eq!(files[0]["project"], files[2]["project"]);
        assert_eq!(files[3]["project"], files[4]["project"]);
        assert_eq!(files[3]["project"], files[5]["project"]);
        for file in files.iter().skip(6) {
            assert_eq!(file["project"]["basis"], "ungrouped", "{file}");
        }
    }

    #[test]
    fn real_project_directories_override_native_stems_and_ambiguous_roots_stay_unassigned() {
        let mut files = vec![
            json!({"name":"bracket.solidcraft","kind":"model"}),
            json!({"name":"bracket.solvecraft","kind":"model"}),
            json!({"name":"bracket-comparison.png","kind":"export"}),
            json!({"name":"fixture/model.solidcraft","kind":"model"}),
            json!({"name":"fixture/renders/other-comparison.png","kind":"export"}),
            json!({"name":"fixture/notes.txt","kind":"other"}),
            json!({"name":"another/fixture.step","kind":"export"}),
        ];
        assign(&mut files);
        assert_eq!(files[0]["project"], files[1]["project"]);
        assert_eq!(files[2]["project"]["basis"], "ungrouped");
        assert_eq!(files[3]["project"], files[4]["project"]);
        assert_eq!(files[3]["project"], files[5]["project"]);
        assert_eq!(files[3]["project"]["name"], "fixture");
        assert_ne!(files[3]["project"], files[6]["project"]);
    }
}
