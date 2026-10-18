//! Virtual groups based only on existing folders and exact file stems.
//! No directory is created, and no association is guessed from a common prefix.
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
    let mut roots: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    for file in files.iter() {
        let Some(name) = file.get("name").and_then(Value::as_str) else { continue };
        if name.contains('/') || !matches!(file.get("kind").and_then(Value::as_str), Some("model" | "export")) {
            continue;
        }
        let Some(stem) = Path::new(name).file_stem().and_then(|part| part.to_str()) else { continue };
        let entry = roots.entry(stem.to_owned()).or_default();
        entry.0 += 1;
        entry.1 |= file.get("kind").and_then(Value::as_str) == Some("model");
    }
    for file in files {
        let Some(name) = file.get("name").and_then(Value::as_str) else { continue };
        let project = if let Some((directory, _)) = name.rsplit_once('/') {
            json!({"id":format!("directory:{directory}"),"name":directory,"basis":"directory"})
        } else {
            let stem = Path::new(name).file_stem().and_then(|part| part.to_str()).unwrap_or(name);
            let association = roots.get(stem).filter(|(count, model)| *model || *count > 1);
            if let Some((_, model)) = association.filter(|_| matches!(file.get("kind").and_then(Value::as_str), Some("model" | "export"))) {
                json!({"id":format!("stem:{stem}"),"name":stem,"basis":if *model {"design_stem"} else {"file_stem"}})
            } else {
                json!({"id":"ungrouped","name":"Other files","basis":"ungrouped"})
            }
        };
        file["project"] = project;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_existing_folders_and_exact_stems_without_prefix_or_case_guesses() {
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
        assert_eq!(files[2]["project"]["basis"], "ungrouped");
        assert_eq!(files[3]["project"]["basis"], "ungrouped");
        assert_eq!(files[4]["project"], files[5]["project"]);
        assert_eq!(files[4]["project"]["name"], "other/deep");
        assert_eq!(files[6]["project"], files[7]["project"]);
        assert_eq!(files[6]["project"]["basis"], "file_stem");
        assert_eq!(files[8]["project"]["basis"], "ungrouped");
    }
}
