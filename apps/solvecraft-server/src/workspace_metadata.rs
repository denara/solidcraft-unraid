//! Bounded informational metadata extraction. Never evaluate geometry, hydrate blobs,
//! extract archive entries to disk, or publish arbitrary namespaced properties/paths.
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::{Value, json};

const MAX_METADATA: u64 = 8 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 10_000;
const MAX_ZIP_DIRECTORY: u64 = 4 * 1024 * 1024;
const MAX_TEXT: usize = 4096;
const MAX_ITEMS: usize = 10_000;

pub(super) fn read(path: &Path) -> Value {
    match read_inner(path) {
        Ok(metadata) => metadata,
        // Fixed public message: parser errors can include embedded secrets and paths.
        Err(reason) => json!({"status":"unavailable","reason":reason}),
    }
}

fn read_inner(path: &Path) -> Result<Value, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "file_unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > super::MAX_FILE_SIZE {
        return Err("file_unavailable");
    }
    let mut file = File::open(path).map_err(|_| "file_unavailable")?;
    let before = file.metadata().map_err(|_| "file_unavailable")?;
    if !before.is_file() || before.len() > super::MAX_FILE_SIZE {
        return Err("file_unavailable");
    }
    let mut signature = [0; 4];
    file.read_exact(&mut signature).map_err(|_| "invalid_design")?;
    file.seek(SeekFrom::Start(0)).map_err(|_| "file_unavailable")?;
    let (bytes, storage) = if &signature == b"PK\x03\x04" {
        (container_document(&mut file, before.len())?, "container")
    } else {
        if before.len() > MAX_METADATA {
            return Err("metadata_limit");
        }
        (read_limited(&mut file, MAX_METADATA)?, "json")
    };
    let after = file.metadata().map_err(|_| "file_unavailable")?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("file_changed");
    }
    let document: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid_design")?;
    summarize(&document, storage)
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).map_err(|_| "invalid_design")?;
    if bytes.len() as u64 > limit {
        return Err("metadata_limit");
    }
    Ok(bytes)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let value: [u8; 2] = bytes.get(offset..offset + 2).ok_or("invalid_container")?.try_into().map_err(|_| "invalid_container")?;
    Ok(u16::from_le_bytes(value))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let value: [u8; 4] = bytes.get(offset..offset + 4).ok_or("invalid_container")?.try_into().map_err(|_| "invalid_container")?;
    Ok(u32::from_le_bytes(value))
}

fn container_document(file: &mut File, length: u64) -> Result<Vec<u8>, &'static str> {
    // Bound the central directory BEFORE zip allocates its name index. Native design
    // packages need neither ZIP64 nor multiple disks, just as the native IO reader.
    let tail_length = length.min(65_557);
    file.seek(SeekFrom::Start(length - tail_length)).map_err(|_| "invalid_container")?;
    let tail = read_limited(&mut *file, 65_557)?;
    let end_start = tail.windows(4).rposition(|part| part == b"PK\x05\x06").ok_or("invalid_container")?;
    let end = tail.get(end_start..).filter(|part| part.len() >= 22).ok_or("invalid_container")?;
    let count = usize::from(u16_at(end, 10)?);
    let directory_size = u64::from(u32_at(end, 12)?);
    let directory_offset = u64::from(u32_at(end, 16)?);
    if count > MAX_ZIP_ENTRIES
        || u16_at(end, 4)? != 0
        || u16_at(end, 6)? != 0
        || usize::from(u16_at(end, 8)?) != count
        || end.len() != 22 + usize::from(u16_at(end, 20)?)
        || directory_size > MAX_ZIP_DIRECTORY
        || directory_offset.checked_add(directory_size) != Some(length - tail_length + end_start as u64)
    {
        return Err("invalid_container");
    }
    let config = zip::read::Config { archive_offset: zip::read::ArchiveOffset::Known(0) };
    let mut archive = zip::ZipArchive::with_config(config, file).map_err(|_| "invalid_container")?;
    if archive.len() != count {
        return Err("invalid_container");
    }
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let part = archive.by_index_raw(index).map_err(|_| "invalid_container")?;
        let name = part.name().map_err(|_| "invalid_container")?;
        if name.is_empty()
            || name.len() > 1024
            || name.contains(['\\', ':'])
            || name.chars().any(char::is_control)
            || name.split('/').any(|part| matches!(part, "" | "." | ".."))
            || !names.insert(name.into_owned())
            || part.size() > super::MAX_FILE_SIZE
        {
            return Err("invalid_container");
        }
        total = total.checked_add(part.size()).ok_or("metadata_limit")?;
        if total > 512 * 1024 * 1024 {
            return Err("metadata_limit");
        }
    }
    let manifest = archive.by_name("container.json").map_err(|_| "invalid_container")?;
    if manifest.size() > 4096 {
        return Err("metadata_limit");
    }
    let manifest: Value = serde_json::from_slice(&read_limited(manifest, 4096)?).map_err(|_| "invalid_container")?;
    // Legacy container tag is the existing explicit IO compatibility shim.
    if !matches!(manifest.get("format").and_then(Value::as_str), Some("solidcraft-container/1" | "solvecraft-container/1")) {
        return Err("unsupported_container");
    }
    let document = archive.by_name("document.json").map_err(|_| "invalid_container")?;
    let expected = document.size();
    if expected > MAX_METADATA {
        return Err("metadata_limit");
    }
    let bytes = read_limited(document, MAX_METADATA)?;
    if bytes.len() as u64 != expected {
        return Err("invalid_container");
    }
    Ok(bytes)
}

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty() && text.len() <= MAX_TEXT && !text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
}

fn summarize(document: &Value, storage: &str) -> Result<Value, &'static str> {
    if !document.is_object() {
        return Err("invalid_design");
    }
    let format = text(document.get("format")).ok_or("invalid_design")?;
    // This is an informational schema check, not a promise that geometry rebuilds.
    let (product, version) = format.split_once('/').ok_or("invalid_design")?;
    if !matches!(product, "solidcraft" | "solvecraft") || version.is_empty() || version.len() > 8 || !version.bytes().all(|v| v.is_ascii_digit()) {
        return Err("unsupported_design");
    }
    let features = document.get("features").and_then(Value::as_array).filter(|items| items.len() <= MAX_ITEMS).ok_or("invalid_design")?;
    let params = document.get("params").and_then(Value::as_array).filter(|items| items.len() <= MAX_ITEMS).ok_or("invalid_design")?;
    let mut out = json!({
        "status":"available", "storage":storage, "format":format,
        "title":text(document.get("name")),
        "units":text(document.get("units")),
        "feature_count":features.len(), "parameter_count":params.len(),
        "sketch_count":features.iter().filter(|feature| feature.get("type").and_then(Value::as_str) == Some("sketch") || feature.get("kind").and_then(|kind| kind.get("type")).and_then(Value::as_str) == Some("sketch")).count(),
    });
    for (source, target) in [("components", "component_count"), ("occurrences", "occurrence_count")] {
        if let Some(value) = document.get(source) {
            let items = value.as_array().filter(|items| items.len() <= MAX_ITEMS).ok_or("invalid_design")?;
            out[target] = json!(items.len());
        }
    }
    for key in ["project", "part_number", "revision", "author", "organization", "description", "license"] {
        if let Some(value) = text(document.get("metadata").and_then(|metadata| metadata.get(key))) {
            out[key] = json!(value);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn design() -> Value {
        json!({"format":"solidcraft/1","name":"Bracket <safe>","units":"mm","features":[{"kind":{"type":"sketch"}},{"kind":{"type":"extrude"}}],"params":[{}],"metadata":{"author":"Example","project":"Fixture","description":"A test","org.example.private":{"path":"/private/location","token":"secret"}},"links":{"123":{"path":"/private/design.solidcraft"}}})
    }

    fn package(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer.start_file(*name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn read_bytes(bytes: &[u8]) -> Value {
        let path = std::env::temp_dir().join(format!("solidcraft-metadata-{}.solidcraft", uuid::Uuid::new_v4()));
        fs::write(&path, bytes).unwrap();
        let out = read(&path);
        fs::remove_file(path).unwrap();
        out
    }

    #[test]
    fn json_and_native_container_extract_only_saved_whitelist() {
        let bytes = serde_json::to_vec(&design()).unwrap();
        for (storage, bytes) in [
            ("json", bytes.clone()),
            (
                "container",
                package(&[
                    ("container.json", br#"{"format":"solidcraft-container/1"}"#.to_vec()),
                    ("document.json", bytes),
                    ("blobs/unused.step", b"never read or evaluated".to_vec()),
                ]),
            ),
        ] {
            let metadata = read_bytes(&bytes);
            assert_eq!(metadata["status"], "available");
            assert_eq!(metadata["storage"], storage);
            assert_eq!(metadata["title"], "Bracket <safe>");
            assert_eq!(metadata["author"], "Example");
            assert_eq!(metadata["feature_count"], 2);
            assert_eq!(metadata["sketch_count"], 1);
            assert_eq!(metadata["parameter_count"], 1);
            assert!(!metadata.to_string().contains("private"));
            assert!(!metadata.to_string().contains("secret"));
        }
    }

    #[test]
    fn committed_design_fixtures_have_readable_saved_metadata() {
        for bytes in [
            include_bytes!("../../../tests/action_matrix/designs/pi-case.solidcraft").as_slice(),
            include_bytes!("../../../tests/action_matrix/designs/pi-case-0.8.solidcraft").as_slice(),
        ] {
            let metadata = read_bytes(bytes);
            assert_eq!(metadata["status"], "available");
            assert!(metadata["feature_count"].as_u64().is_some_and(|count| count > 0));
            assert!(metadata["sketch_count"].as_u64().is_some_and(|count| count > 0));
            assert_eq!(metadata["units"], "mm");
        }
    }

    #[test]
    fn corrupt_unsupported_and_hostile_metadata_are_bounded_unavailable() {
        for bytes in [
            b"not a design".to_vec(),
            vec![],
            b"PK\x03\x04broken".to_vec(),
            serde_json::to_vec(&json!({"format":"other/1","features":[],"params":[]})).unwrap(),
            vec![b' '; MAX_METADATA as usize + 1],
        ] {
            assert_eq!(read_bytes(&bytes)["status"], "unavailable");
        }
        for name in ["../escape", "/absolute", "nested/../escape", "bad\\name", "C:private"] {
            let bytes = package(&[
                ("container.json", br#"{"format":"solidcraft-container/1"}"#.to_vec()),
                ("document.json", serde_json::to_vec(&design()).unwrap()),
                (name, vec![]),
            ]);
            assert_eq!(read_bytes(&bytes)["status"], "unavailable", "{name}");
        }
        let bytes = package(&[
            ("container.json", br#"{"format":"solidcraft-container/1"}"#.to_vec()),
            ("document.json", vec![b' '; MAX_METADATA as usize + 1]),
        ]);
        assert_eq!(read_bytes(&bytes)["reason"], "metadata_limit");
        let mut invalid = design();
        invalid["metadata"]["author"] = json!("x".repeat(MAX_TEXT + 1));
        invalid["metadata"]["description"] = json!(42);
        let metadata = read_bytes(&serde_json::to_vec(&invalid).unwrap());
        assert_eq!(metadata["status"], "available");
        assert!(metadata.get("author").is_none());
        assert!(metadata.get("description").is_none());
    }

    #[test]
    fn truncated_archives_forged_directories_and_deep_json_never_panic() {
        let bytes = package(&[
            ("container.json", br#"{"format":"solidcraft-container/1"}"#.to_vec()),
            ("document.json", serde_json::to_vec(&design()).unwrap()),
        ]);
        for length in (0..bytes.len()).step_by(19) {
            assert_eq!(read_bytes(&bytes[..length])["status"], "unavailable");
        }
        let end = bytes.windows(4).rposition(|part| part == b"PK\x05\x06").unwrap();
        for (offset, value) in [(10, 255), (13, 255), (16, 255)] {
            let mut forged = bytes.clone();
            forged[end + offset] = value;
            assert_eq!(read_bytes(&forged)["status"], "unavailable");
        }
        let deep = format!("{}0{}", "[".repeat(256), "]".repeat(256));
        assert_eq!(read_bytes(deep.as_bytes())["status"], "unavailable");
        let excessive = json!({"format":"solidcraft/1","features":vec![Value::Null; MAX_ITEMS+1],"params":[]});
        assert_eq!(read_bytes(&serde_json::to_vec(&excessive).unwrap())["status"], "unavailable");
    }
}
