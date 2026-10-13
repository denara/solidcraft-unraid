//! First-launch adoption of a pre-rename settings or data folder. Compatibility shim (see
//! docs/compatibility.md): when the current folder doesn't exist yet and the legacy one does,
//! the legacy folder is copied over. The legacy folder is never changed or deleted, so an older
//! release installed side by side keeps working.

use std::path::{Path, PathBuf};

/// The Linux app id (Flatpak, desktop entry, AppStream, Wayland `app_id`).
pub const APP_ID: &str = "ai.storyteller.solidcraft";
/// The app id of releases before 0.8 (compatibility shim, docs/compatibility.md).
pub const LEGACY_APP_ID: &str = "ai.storyteller.solvecraft";

/// Most files and nesting copied from a legacy folder (it holds preferences, autosaves and
/// small caches; anything bigger is not ours).
const MAX_ENTRIES: usize = 10_000;
const MAX_DEPTH: usize = 8;

/// Copy `legacy` to `current` if `current` doesn't exist and `legacy` is a folder. Returns
/// whether a copy was made. Symbolic links are skipped. A copy that fails part way leaves what
/// was copied (the next launch sees `current` and doesn't try again) and reports the error.
pub fn adopt_legacy_dir(current: &Path, legacy: &Path) -> std::io::Result<bool> {
    if current.exists() || !legacy.is_dir() || current == legacy {
        return Ok(false);
    }
    let mut budget = MAX_ENTRIES;
    copy_tree(legacy, current, 0, &mut budget)?;
    Ok(true)
}

/// In the Flatpak sandbox, the same `path` under the pre-rename app's data folder: Flatpak keeps
/// each app's XDG folders under `~/.var/app/<app id>`, so after the app id changed, settings and
/// autosaves of earlier Flatpak releases sit under the old id. `None` outside the sandbox or for
/// paths outside our own `~/.var/app` folder. Flatpak moves that folder itself when an install
/// follows the end-of-life rebase; this covers installs made next to the old app, which reach it
/// read-only through the manifest's `--filesystem` grant.
pub fn legacy_flatpak_path(path: &Path) -> Option<PathBuf> {
    flatpak_twin(path, &std::env::var_os("FLATPAK_ID")?, Path::new(&std::env::var_os("HOME")?))
}

fn flatpak_twin(path: &Path, flatpak_id: &std::ffi::OsStr, home: &Path) -> Option<PathBuf> {
    if flatpak_id != APP_ID {
        return None;
    }
    let apps = home.join(".var").join("app");
    let rel = path.strip_prefix(apps.join(APP_ID)).ok()?;
    Some(apps.join(LEGACY_APP_ID).join(rel))
}

fn copy_tree(from: &Path, to: &Path, depth: usize, budget: &mut usize) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    if depth >= MAX_DEPTH {
        return Ok(());
    }
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if *budget == 0 {
            return Ok(());
        }
        *budget -= 1;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, depth + 1, budget)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("solidcraft-legacy-dir-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copies_legacy_folder_once_and_keeps_it() {
        let d = scratch("copy");
        let old = d.join("solvecraft");
        std::fs::create_dir_all(old.join("recovery")).unwrap();
        std::fs::write(old.join("graphics.json"), "{\"msaa\":8}").unwrap();
        std::fs::write(old.join("recovery").join("a.solvecraft"), "x").unwrap();
        let new = d.join("solidcraft");
        assert!(adopt_legacy_dir(&new, &old).unwrap());
        assert_eq!(std::fs::read_to_string(new.join("graphics.json")).unwrap(), "{\"msaa\":8}");
        assert!(new.join("recovery").join("a.solvecraft").is_file());
        // The legacy folder is untouched.
        assert!(old.join("graphics.json").is_file() && old.join("recovery").join("a.solvecraft").is_file());
        // Once the current folder exists, nothing is copied again.
        std::fs::write(old.join("later.json"), "{}").unwrap();
        assert!(!adopt_legacy_dir(&new, &old).unwrap());
        assert!(!new.join("later.json").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn flatpak_twin_maps_our_sandbox_data_to_the_old_app_id() {
        let home = Path::new("/home/u");
        let ours = home.join(".var/app/ai.storyteller.solidcraft/data/solidcraft");
        let id = std::ffi::OsStr::new(APP_ID);
        assert_eq!(flatpak_twin(&ours, id, home), Some(home.join(".var/app/ai.storyteller.solvecraft/data/solidcraft")));
        // Outside our sandbox folder, or in another app's sandbox, there is nothing to map.
        assert_eq!(flatpak_twin(&home.join(".local/share/solidcraft"), id, home), None);
        assert_eq!(flatpak_twin(&ours, std::ffi::OsStr::new("org.example.Other"), home), None);
    }

    #[test]
    fn nothing_to_adopt() {
        let d = scratch("none");
        let new = d.join("solidcraft");
        assert!(!adopt_legacy_dir(&new, &d.join("missing")).unwrap());
        assert!(!new.exists());
        // An existing current folder is never overwritten.
        let old = d.join("solvecraft");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("p.json"), "old").unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("p.json"), "new").unwrap();
        assert!(!adopt_legacy_dir(&new, &old).unwrap());
        assert_eq!(std::fs::read_to_string(new.join("p.json")).unwrap(), "new");
        let _ = std::fs::remove_dir_all(&d);
    }
}
