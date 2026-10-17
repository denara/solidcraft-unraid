//! Bounded, public-safe inventory of the gateway's persistent files.
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 4096;
const MAX_FILES: usize = 512;
const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;

pub struct Listing {
    pub files: Vec<Value>,
    pub truncated: bool,
}

/// Only relative, UTF-8 names without hidden or temporary components are public.
pub fn visible_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 1024
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || name.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return false;
    }
    let path = Path::new(name);
    if path.is_absolute() || path.components().count() > MAX_DEPTH + 1 {
        return false;
    }
    path.components().all(|part| {
        let Component::Normal(part) = part else { return false };
        let Some(part) = part.to_str() else { return false };
        let lower = part.to_ascii_lowercase();
        !part.starts_with('.')
            && !part.ends_with('~')
            && !lower.ends_with(".tmp")
            && !lower.ends_with(".part")
            && !lower.ends_with(".upload")
            && !lower.ends_with(".bak")
    })
}

fn kind(name: &str) -> &'static str {
    match Path::new(name).extension().and_then(|part| part.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("solidcraft" | "solvecraft") => "model",
        Some("step" | "stp" | "stl" | "3mf" | "obj" | "png" | "gif" | "svg" | "dxf" | "glb" | "gltf" | "pdf") => "export",
        _ => "other",
    }
}

pub fn list(root: &Path) -> Result<Listing, String> {
    let mut listing = Listing { files: Vec::new(), truncated: false };
    let metadata = fs::symlink_metadata(root).map_err(|_| "workspace files are unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("workspace files are unavailable".into());
    }
    let mut visited = 0;
    scan(root, "", 0, &mut visited, &mut listing)?;
    listing.files.sort_by(|left, right| left.get("name").and_then(Value::as_str).cmp(&right.get("name").and_then(Value::as_str)));
    Ok(listing)
}

fn scan(root: &Path, parent: &str, depth: usize, visited: &mut usize, listing: &mut Listing) -> Result<bool, String> {
    let directory =
        if parent.is_empty() { root.to_path_buf() } else { crate::paths::resolve(root, parent).map_err(|_| "workspace directory is unavailable")? };
    let entries = fs::read_dir(directory).map_err(|_| "workspace directory is unavailable")?;
    for entry in entries {
        if *visited >= MAX_ENTRIES {
            listing.truncated = true;
            return Ok(true);
        }
        *visited += 1;
        let Ok(entry) = entry else {
            listing.truncated = true;
            continue;
        };
        let Some(component) = entry.file_name().to_str().map(str::to_owned) else { continue };
        let name = if parent.is_empty() { component } else { format!("{parent}/{component}") };
        if !visible_name(&name) {
            continue;
        }
        let Ok(path) = crate::paths::resolve(root, &name) else { continue };
        let Ok(metadata) = fs::symlink_metadata(path) else {
            listing.truncated = true;
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if depth < MAX_DEPTH {
                if scan(root, &name, depth + 1, visited, listing)? {
                    return Ok(true);
                }
            } else {
                listing.truncated = true;
            }
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if metadata.len() > MAX_FILE_SIZE {
            listing.truncated = true;
            continue;
        }
        if listing.files.len() == MAX_FILES {
            listing.truncated = true;
            return Ok(true);
        }
        let modified_at = metadata.modified().ok().map(|time| DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Millis, true));
        listing.files.push(json!({
            "name": name,
            "kind": kind(&name),
            "size_bytes": metadata.len(),
            "modified_at": modified_at,
            "opened_by": [],
            "dirty": null,
        }));
    }
    Ok(false)
}

/// A strong, quoted ETag, independent of size/mtime, for conflict detection.
pub fn revision(root: &Path, name: &str) -> Result<Option<String>, String> {
    if !visible_name(name) {
        return Err("file name is unavailable".into());
    }
    let root_meta = fs::symlink_metadata(root).map_err(|_| "workspace files are unavailable")?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err("workspace files are unavailable".into());
    }
    let path = crate::paths::resolve(root, name).map_err(|_| "file name is unavailable")?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("file is unavailable".into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_FILE_SIZE {
        return Err("file is unavailable or exceeds the size limit".into());
    }
    let file = File::open(path).map_err(|_| "file cannot be read")?;
    let before = file.metadata().map_err(|_| "file is unavailable")?;
    if !before.is_file() || before.len() > MAX_FILE_SIZE {
        return Err("file is unavailable or exceeds the size limit".into());
    }
    let mut reader = file.take(MAX_FILE_SIZE + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = reader.read(&mut buffer).map_err(|_| "file cannot be read")?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > MAX_FILE_SIZE {
            return Err("file exceeds the size limit".into());
        }
        hash.update(buffer.get(..count).ok_or("file cannot be read")?);
    }
    let after = reader.get_ref().metadata().map_err(|_| "file is unavailable")?;
    if bytes != before.len() || before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("file changed while being read; retry the request".into());
    }
    Ok(Some(format!("\"{:x}\"", hash.finalize())))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("solidcraft-workspace-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn inventory_is_recursive_sorted_and_omits_hidden_temporary_files() {
        let root = Fixture::new();
        fs::create_dir(root.0.join("parts")).unwrap();
        fs::create_dir(root.0.join(".private")).unwrap();
        for name in [
            "z.STEP",
            "parts/a.solidcraft",
            "old.solvecraft",
            "notes.txt",
            ".upload-123",
            "notes.tmp",
            "notes.part",
            "notes~",
            ".private/secret.solidcraft",
            "part.solidcraft.bak",
            "part.solidcraft.BAK",
        ] {
            fs::write(root.0.join(name), b"abc").unwrap();
        }
        let listing = list(&root.0).unwrap();
        assert!(!listing.truncated);
        let names: Vec<_> = listing.files.iter().filter_map(|item| item["name"].as_str()).collect();
        assert_eq!(names, ["notes.txt", "old.solvecraft", "parts/a.solidcraft", "z.STEP"]);
        assert_eq!(listing.files[1]["kind"], "model");
        assert_eq!(listing.files[3]["kind"], "export");
        assert_eq!(listing.files[0]["size_bytes"], 3);
        assert!(listing.files[0]["modified_at"].as_str().is_some_and(|time| time.ends_with('Z')));
        assert_eq!(listing.files[0]["opened_by"], json!([]));
        assert_eq!(listing.files[0]["dirty"], Value::Null);
    }

    #[test]
    fn revisions_detect_equal_size_equal_timestamp_changes_and_reject_hidden_paths() {
        let root = Fixture::new();
        let path = root.0.join("part.solidcraft");
        fs::write(&path, b"first").unwrap();
        let timestamp = fs::metadata(&path).unwrap().modified().unwrap();
        let original = revision(&root.0, "part.solidcraft").unwrap().unwrap();
        fs::write(&path, b"other").unwrap();
        File::options().write(true).open(&path).unwrap().set_times(fs::FileTimes::new().set_modified(timestamp)).unwrap();
        assert_ne!(original, revision(&root.0, "part.solidcraft").unwrap().unwrap());
        assert_eq!(original.len(), 66);
        assert!(original.starts_with('"') && original.ends_with('"'));
        assert_eq!(revision(&root.0, "missing.solidcraft").unwrap(), None);
        for name in [
            "../secret",
            ".upload-123",
            "a/.hidden/file",
            "/etc/passwd",
            "foo\\bar",
            "file.tmp",
            "part.solidcraft.bak",
            "part.solidcraft.BAK",
            "a/./b",
            "a//b",
            "a\nb",
        ] {
            assert!(revision(&root.0, name).is_err());
        }
        fs::create_dir(root.0.join("folder")).unwrap();
        assert!(revision(&root.0, "folder").is_err());
    }

    #[test]
    fn inventory_bounds_file_count_depth_and_size() {
        let root = Fixture::new();
        for number in 0..MAX_FILES {
            fs::write(root.0.join(format!("{number:04}.solidcraft")), b"").unwrap();
        }
        assert!(!list(&root.0).unwrap().truncated);
        fs::write(root.0.join("extra.solidcraft"), b"").unwrap();
        let listing = list(&root.0).unwrap();
        assert!(listing.truncated);
        assert_eq!(listing.files.len(), MAX_FILES);
        let deep = Fixture::new();
        let mut path = deep.0.clone();
        for _ in 0..MAX_DEPTH + 2 {
            path.push("nested");
            fs::create_dir(&path).unwrap();
        }
        fs::write(path.join("hidden-by-depth.solidcraft"), b"").unwrap();
        assert!(list(&deep.0).unwrap().truncated);
        assert!(list(&deep.0).unwrap().files.is_empty());
        let large = Fixture::new();
        File::create(large.0.join("large.step")).unwrap().set_len(MAX_FILE_SIZE + 1).unwrap();
        assert!(list(&large.0).unwrap().truncated);
        assert!(list(&large.0).unwrap().files.is_empty());
        assert!(revision(&large.0, "large.step").is_err());
    }

    #[test]
    fn inventory_caps_even_excluded_entries() {
        let root = Fixture::new();
        for number in 0..MAX_ENTRIES + 1 {
            fs::write(root.0.join(format!(".upload-{number}")), b"").unwrap();
        }
        let listing = list(&root.0).unwrap();
        assert!(listing.truncated);
        assert!(listing.files.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_files_directories_and_non_utf8_names_are_never_exposed() {
        use std::os::unix::ffi::OsStrExt;
        let root = Fixture::new();
        let outside = Fixture::new();
        fs::write(outside.0.join("secret.solidcraft"), b"private").unwrap();
        std::os::unix::fs::symlink(&outside.0, root.0.join("linked")).unwrap();
        std::os::unix::fs::symlink(outside.0.join("secret.solidcraft"), root.0.join("secret.solidcraft")).unwrap();
        fs::write(root.0.join(std::ffi::OsStr::from_bytes(b"invalid-\xff")), b"hidden").unwrap();
        assert!(list(&root.0).unwrap().files.is_empty());
        assert!(revision(&root.0, "linked/secret.solidcraft").is_err());
        assert!(revision(&root.0, "secret.solidcraft").is_err());
    }
}
