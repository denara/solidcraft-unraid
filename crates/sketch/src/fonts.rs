//! Fonts for sketch text: the embedded default (Liberation Sans), a TrueType/OpenType file
//! named by its path, or an installed font named by its family or full name, looked up in the
//! system's and the user's font folders (plus `SOLIDCRAFT_FONT_DIRS`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::model::{Result, SketchError};

static DEFAULT_FONT: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");

/// Family name of the built-in font (also what an empty or absent font name means).
pub const DEFAULT_FONT_FAMILY: &str = "Liberation Sans";
/// Largest font file read.
const MAX_FONT_BYTES: u64 = 64 << 20;
/// Most font files looked at when searching the font folders.
const MAX_FONT_FILES: usize = 20_000;
/// Deepest folder level searched below a font folder.
const MAX_DIR_DEPTH: usize = 6;
/// Font files kept in memory once read, and the most bytes they may add up to.
const MAX_CACHED_FONTS: usize = 16;
const MAX_CACHED_BYTES: usize = 128 << 20;
/// Longest font name or path accepted.
const MAX_FONT_NAME: usize = 1024;

/// The bytes of a font file and the face to use in it (collections hold several faces).
#[derive(Clone)]
pub struct FontData {
    bytes: FontBytes,
    pub index: u32,
}

#[derive(Clone)]
enum FontBytes {
    Builtin,
    File(Arc<Vec<u8>>),
}

impl FontData {
    pub fn bytes(&self) -> &[u8] {
        match &self.bytes {
            FontBytes::Builtin => DEFAULT_FONT,
            FontBytes::File(b) => b.as_slice(),
        }
    }
}

/// The font a text names: `None`, an empty name or the built-in family give the built-in
/// font; a name with a path separator or a font file extension is a file (`~` and `%VAR%` are
/// expanded); anything else is the family (or full) name of an installed font.
pub fn font_data(name: Option<&str>) -> Result<FontData> {
    let name = name.map(str::trim).unwrap_or("");
    if name.is_empty() || normalize(name) == normalize(DEFAULT_FONT_FAMILY) {
        return Ok(FontData { bytes: FontBytes::Builtin, index: 0 });
    }
    if name.len() > MAX_FONT_NAME {
        return Err(SketchError::Invalid("the font name is too long".into()));
    }
    if is_path(name) {
        let path = expand(name);
        let bytes = read_font(&path).map_err(|e| SketchError::Invalid(format!("font file `{name}`: {e}")))?;
        let index = 0;
        ttf_parser::Face::parse(&bytes, index)
            .map_err(|e| SketchError::Invalid(format!("font file `{name}` is not a TrueType/OpenType font: {e}")))?;
        return Ok(FontData { bytes: FontBytes::File(bytes), index });
    }
    find_family_in(&font_dirs(), name)?
        .ok_or_else(|| SketchError::Invalid(format!("unknown font `{name}`: not an installed font family, and not a .ttf/.otf file path")))
}

/// An installed face whose family, full or PostScript name is `name`, searched in `dirs`.
pub fn find_family_in(dirs: &[PathBuf], name: &str) -> Result<Option<FontData>> {
    let want = normalize(name);
    if want.is_empty() {
        return Ok(None);
    }
    let files = font_files(dirs);
    // Files whose name resembles the family come first (arial.ttf for Arial, times.ttf for
    // Times New Roman): most lookups end there without opening every installed font.
    let (likely, rest): (Vec<&PathBuf>, Vec<&PathBuf>) = files.iter().partition(|p| {
        let stem = p.file_stem().map(|s| normalize(&s.to_string_lossy())).unwrap_or_default();
        stem.len() >= 3 && (stem.contains(&want) || want.contains(&stem))
    });
    for group in [likely, rest] {
        let mut best: Option<(u32, PathBuf, u32)> = None;
        for path in group {
            for face in face_names(path).iter() {
                let score = face.score(&want);
                if score > 0 && best.as_ref().is_none_or(|(b, _, _)| score > *b) {
                    best = Some((score, path.clone(), face.index));
                }
            }
            if best.as_ref().is_some_and(|(s, _, _)| *s >= 4) {
                break;
            }
        }
        if let Some((_, path, index)) = best {
            let bytes = read_font(&path).map_err(|e| SketchError::Invalid(format!("font file `{}`: {e}", path.display())))?;
            return Ok(Some(FontData { bytes: FontBytes::File(bytes), index }));
        }
    }
    Ok(None)
}

/// The names of one face of a font file (normalised), for matching.
struct FaceNames {
    index: u32,
    regular: bool,
    names: Vec<(u16, String)>,
}

impl FaceNames {
    /// How well the face answers to a (normalised) name: 4 full or PostScript name, 3 family
    /// name of a regular face, 2 family name of another style, 0 no match.
    fn score(&self, want: &str) -> u32 {
        use ttf_parser::name_id;
        self.names
            .iter()
            .filter(|(_, n)| n == want)
            .map(|(id, _)| match *id {
                name_id::FULL_NAME | name_id::POST_SCRIPT_NAME => 4,
                name_id::FAMILY | name_id::TYPOGRAPHIC_FAMILY if self.regular => 3,
                name_id::FAMILY | name_id::TYPOGRAPHIC_FAMILY => 2,
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }
}

/// The faces' names of a font file, read once per process (a lookup that finds nothing reads
/// every installed font once, not on every try).
fn face_names(path: &Path) -> Arc<Vec<FaceNames>> {
    static NAMES: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<FaceNames>>>>> = OnceLock::new();
    let cache = NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(c) = cache.lock()
        && let Some(n) = c.get(path)
    {
        return n.clone();
    }
    let mut faces = Vec::new();
    if let Ok(bytes) = read_file(path) {
        let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1).min(64);
        for index in 0..count {
            let Ok(face) = ttf_parser::Face::parse(&bytes, index) else { continue };
            let names = face
                .names()
                .into_iter()
                .filter_map(|n| {
                    let text = match n.to_string() {
                        Some(t) => t,
                        // Macintosh Roman names: their ASCII part.
                        None if n.platform_id == ttf_parser::PlatformId::Macintosh => {
                            n.name.iter().filter(|b| b.is_ascii()).map(|b| *b as char).collect()
                        }
                        None => return None,
                    };
                    Some((n.name_id, normalize(&text)))
                })
                .collect();
            faces.push(FaceNames { index, regular: face.is_regular() || (!face.is_bold() && !face.is_italic()), names });
        }
    }
    let faces = Arc::new(faces);
    if let Ok(mut c) = cache.lock()
        && c.len() < MAX_FONT_FILES
    {
        c.insert(path.to_path_buf(), faces.clone());
    }
    faces
}

/// Lower-case letters and digits only: "DejaVu_Sans", "Dejavu Sans" and "dejavu-sans" are one name.
fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn is_path(name: &str) -> bool {
    name.contains('/') || name.contains('\\') || has_font_extension(name)
}

fn has_font_extension(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".ttf", ".otf", ".ttc", ".otc"].iter().any(|e| lower.ends_with(e))
}

/// `~` at the start and `%VAR%` replaced from the environment.
fn expand(name: &str) -> PathBuf {
    let mut s = name.to_string();
    if let Some(rest) = s.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'))
        && let Some(home) = home()
    {
        s = format!("{}{rest}", home.display());
    }
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some((head, tail)) = rest.split_once('%') {
        out.push_str(head);
        match tail.split_once('%') {
            // A variable that is not set stays as written.
            Some((var, after)) if !var.is_empty() && !var.contains(['=', '\0']) => {
                match std::env::var(var) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(var);
                        out.push('%');
                    }
                }
                rest = after;
            }
            _ => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    PathBuf::from(out)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

/// The folders searched for installed fonts, in order.
pub fn font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("SOLIDCRAFT_FONT_DIRS").map(|v| std::env::split_paths(&v).collect()).unwrap_or_default();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(windows) {
        dirs.push(env("WINDIR").or_else(|| env("SystemRoot")).unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("Fonts"));
        if let Some(local) = env("LOCALAPPDATA") {
            dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = home() {
            dirs.push(h.join("Library").join("Fonts"));
        }
    } else {
        if let Some(d) = env("XDG_DATA_HOME") {
            dirs.push(d.join("fonts"));
        }
        if let Some(h) = home() {
            dirs.push(h.join(".local").join("share").join("fonts"));
            dirs.push(h.join(".fonts"));
        }
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        dirs.push(PathBuf::from("/usr/share/fonts"));
    }
    dirs
}

/// TrueType/OpenType files under `dirs` (bounded in depth and count).
fn font_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo: Vec<(PathBuf, usize)> = dirs.iter().rev().map(|d| (d.clone(), 0)).collect();
    while let Some((dir, depth)) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut sub = Vec::new();
        for e in entries.flatten() {
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if depth < MAX_DIR_DEPTH {
                    sub.push((path, depth + 1));
                }
            } else if has_font_extension(&path.to_string_lossy()) {
                out.push(path);
                if out.len() >= MAX_FONT_FILES {
                    return out;
                }
            }
        }
        sub.sort();
        todo.extend(sub.into_iter().rev());
    }
    out
}

/// A font file's bytes (bounded in size).
fn read_file(path: &Path) -> std::result::Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a file".into());
    }
    if meta.len() > MAX_FONT_BYTES {
        return Err("the file is too large for a font".into());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

/// A font file's bytes, from the cache when it was read before.
fn read_font(path: &Path) -> std::result::Result<Arc<Vec<u8>>, String> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(c) = cache.lock()
        && let Some(b) = c.get(path)
    {
        return Ok(b.clone());
    }
    let bytes = Arc::new(read_file(path)?);
    if let Ok(mut c) = cache.lock() {
        if c.len() >= MAX_CACHED_FONTS || c.values().map(|b| b.len()).sum::<usize>() + bytes.len() > MAX_CACHED_BYTES {
            c.clear();
        }
        c.insert(path.to_path_buf(), bytes.clone());
    }
    Ok(bytes)
}
