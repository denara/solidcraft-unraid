//! Strict, bounded manifests shared by confirmation and streaming archives.
use std::fs::{self, Metadata};
use std::path::Path;

use sha2::{Digest, Sha256};

pub(super) const MAX_ENTRIES: usize = 2048;
const MAX_BYTES: u64 = 16 << 30;
const MAX_NAMES: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub name: String,
    pub directory: bool,
    pub bytes: u64,
    pub stamp: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub entries: Vec<Entry>,
    pub excluded: usize,
    pub bytes: u64,
    pub files: usize,
    pub folders: usize,
    pub revision: String,
}

pub(super) fn stamp(meta: &Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}",
            meta.dev(),
            meta.ino(),
            meta.mode(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
            meta.ctime(),
            meta.ctime_nsec()
        )
    }
    #[cfg(not(unix))]
    {
        format!("{:?}:{:?}:{}:{}", meta.modified(), meta.created(), meta.len(), meta.is_dir())
    }
}

pub(crate) fn inspect(root: &Path, name: &str, content: bool) -> Result<Manifest, &'static str> {
    collect(root, name, content, false)
}

/// ZIPs contain the unfiltered workspace file set, without private auxiliary entries.
/// Deletion intentionally keeps the complete, strict manifest above.
pub(crate) fn archive(root: &Path, name: &str) -> Result<Manifest, &'static str> {
    collect(root, name, false, true)
}

fn collect(root: &Path, name: &str, content: bool, exclude_auxiliary: bool) -> Result<Manifest, &'static str> {
    if !crate::workspace_files::visible_name(name) || Path::new(name).is_absolute() {
        return Err("Ungültiger relativer Pfad. Der Speicher-Root ist geschützt.");
    }
    let root_meta = fs::symlink_metadata(root).map_err(|_| "Dateispeicher nicht verfügbar.")?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err("Ungültiger Dateispeicher.");
    }
    let mut result = Manifest { entries: Vec::new(), excluded: 0, bytes: 0, files: 0, folders: 0, revision: String::new() };
    let mut pending = vec![name.to_owned()];
    let mut names = name.len();
    let mut inspected = 1usize;
    while let Some(name) = pending.pop() {
        let path = crate::paths::resolve(root, &name).map_err(|_| "Unsicherer oder reservierter Pfad im Ordner.")?;
        let meta = fs::symlink_metadata(&path).map_err(|_| "Inhalt nicht verfügbar oder während der Prüfung verändert.")?;
        if meta.file_type().is_symlink() || (!meta.is_file() && !meta.is_dir()) {
            return Err("Verknüpfungen und spezielle Dateien werden nicht unterstützt.");
        }
        if result.entries.len() >= MAX_ENTRIES || names > MAX_NAMES {
            return Err("Ordner zu groß: höchstens 2048 Einträge und 1 MiB Pfadnamen.");
        }
        let directory = meta.is_dir();
        if directory {
            result.folders += 1;
            for child in fs::read_dir(&path).map_err(|_| "Ordner kann nicht vollständig gelesen werden.")? {
                inspected = inspected.saturating_add(1);
                if inspected > MAX_ENTRIES {
                    return Err("Ordner hat mehr als 2048 Einträge.");
                }
                let child = child.map_err(|_| "Ordner kann nicht vollständig gelesen werden.")?;
                let basename = child.file_name().into_string().map_err(|_| "Dateiname ist kein gültiger Unicode-Text.")?;
                let child_name = format!("{name}/{basename}");
                names = names.saturating_add(child_name.len());
                if names > MAX_NAMES {
                    return Err("Ordner zu groß: höchstens 2048 Einträge und 1 MiB Pfadnamen.");
                }
                if exclude_auxiliary && crate::workspace_files::excluded_component(&basename) {
                    // Never follow a link or read an excluded file/directory's contents.
                    let kind = child.file_type().map_err(|_| "Inhalt nicht verfügbar oder während der Prüfung verändert.")?;
                    if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                        return Err("Verknüpfungen und spezielle Dateien werden nicht unterstützt.");
                    }
                    result.excluded += 1;
                    continue;
                }
                let child = child_name;
                // Confirmation must never silently omit protected entries.
                if !crate::workspace_files::visible_name(&child) {
                    return Err("Ordner enthält geschützte, versteckte oder zu tief verschachtelte Einträge.");
                }
                pending.push(child);
            }
        } else {
            result.files += 1;
            result.bytes = result.bytes.checked_add(meta.len()).filter(|n| *n <= MAX_BYTES).ok_or("Ordner überschreitet die Grenze von 16 GiB.")?;
        }
        result.entries.push(Entry { name, directory, bytes: if directory { 0 } else { meta.len() }, stamp: stamp(&meta) });
    }
    result.entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut digest = Sha256::new();
    for entry in &result.entries {
        digest.update((entry.name.len() as u64).to_le_bytes());
        digest.update(entry.name.as_bytes());
        digest.update([u8::from(entry.directory)]);
        digest.update(entry.stamp.as_bytes());
        if content && !entry.directory {
            // Deletion uses the same strong revision definition as HTTP/MCP writes.
            let revision = crate::workspace_files::revision(root, &entry.name)
                .map_err(|_| "Dateirevision nicht verfügbar (maximal 256 MiB pro Datei beim Löschen).")?
                .ok_or("Datei wurde verändert.")?;
            digest.update(revision.as_bytes());
        }
    }
    result.revision = format!("{:x}", digest.finalize());
    Ok(result)
}
