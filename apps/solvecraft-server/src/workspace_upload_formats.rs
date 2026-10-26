//! A deliberately bounded, data-only upload policy. Import still happens explicitly in CAD.
use serde_json::Value;
use std::io::{Cursor, Read};
use std::path::Path;
pub(crate) const EXTENSIONS: &[&str] =
    &["solidcraft", "solvecraft", "json", "step", "stp", "igs", "iges", "stl", "3mf", "obj", "glb", "gltf", "dxf", "svg", "png", "jpg", "jpeg"];
pub(crate) fn extension(name: &str) -> Result<String, &'static str> {
    let ext = Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if EXTENSIONS.contains(&ext.as_str()) { Ok(ext) } else { Err("Dateityp nicht unterstützt. Erlaubte Formate stehen im Upload-Bereich.") }
}
fn native(bytes: &[u8]) -> Result<(), &'static str> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Ungültige SolidCraft-JSON-Datei.")?;
    if value.get("format").and_then(Value::as_str).is_some_and(|s| {
        s.strip_prefix("solidcraft/")
            .or_else(|| s.strip_prefix("solvecraft/"))
            .is_some_and(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()))
    }) && value.get("features").is_some_and(Value::is_array)
    {
        Ok(())
    } else {
        Err("JSON muss ein SolidCraft-Design sein; Skripte werden nicht importiert.")
    }
}
fn container(bytes: &[u8], design: bool) -> Result<(), &'static str> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Ungültiger CAD-Container.")?;
    if zip.len() > 512 {
        return Err("CAD-Container enthält zu viele Einträge.");
    }
    let mut total = 0u64;
    let mut found = false;
    for i in 0..zip.len() {
        let mut item = zip.by_index(i).map_err(|_| "CAD-Container ist nicht lesbar.")?;
        let name = item.name().map_err(|_| "Ungültiger Name im CAD-Container.")?.into_owned();
        if name.starts_with('/')
            || name.contains(['\\', ':'])
            || name.split('/').any(|p| matches!(p, "." | ".."))
            || item.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Unsichere Pfade im CAD-Container.");
        }
        total = total.checked_add(item.size()).ok_or("CAD-Container ist zu groß.")?;
        if total > 128 << 20 || item.size() > 64 << 20 {
            return Err("Entpackter CAD-Container überschreitet die Grenze.");
        }
        if design && name == "document.json" {
            if item.size() > 8 << 20 {
                return Err("Design-Metadaten überschreiten 8 MiB.");
            }
            let mut data = Vec::new();
            item.by_ref().take((8 << 20) + 1).read_to_end(&mut data).map_err(|_| "Design-Metadaten sind beschädigt.")?;
            native(&data)?;
            found = true;
        } else if !design && name == "3D/3dmodel.model" {
            found = true;
        }
    }
    if found { Ok(()) } else { Err("CAD-Container enthält kein unterstütztes Modell.") }
}
fn gltf(value: &Value) -> Result<(), &'static str> {
    if value.pointer("/asset/version").and_then(Value::as_str) != Some("2.0") {
        return Err("glTF muss Version 2.0 verwenden.");
    }
    fn scan(v: &Value, depth: u8) -> bool {
        if depth > 32 {
            return false;
        }
        match v {
            Value::Object(o) => o.iter().all(|(k, v)| {
                if k == "uri" {
                    v.as_str().is_some_and(|s| {
                        [
                            "data:application/octet-stream;base64,",
                            "data:application/gltf-buffer;base64,",
                            "data:image/png;base64,",
                            "data:image/jpeg;base64,",
                        ]
                        .iter()
                        .any(|p| s.starts_with(p))
                    })
                } else {
                    scan(v, depth + 1)
                }
            }),
            Value::Array(a) => a.iter().all(|v| scan(v, depth + 1)),
            _ => true,
        }
    }
    if scan(value, 0) {
        Ok(())
    } else {
        Err("glTF mit externen Begleitdateien wird nicht hochgeladen. Bitte selbstenthaltendes GLB oder eingebettete Daten verwenden.")
    }
}
pub(crate) fn validate(name: &str, bytes: &[u8]) -> Result<(), &'static str> {
    let ext = extension(name)?;
    if bytes.is_empty() {
        return Err("Leere Dateien werden nicht hochgeladen.");
    }
    match ext.as_str() {
        "solidcraft" | "solvecraft" => {
            if bytes.starts_with(b"PK\x03\x04") {
                container(bytes, true)
            } else {
                native(bytes)
            }
        }
        "json" => native(bytes),
        "3mf" => container(bytes, false),
        "png" | "jpg" | "jpeg" => {
            if bytes.len() > 8 << 20 {
                return Err("Bildschablonen dürfen höchstens 8 MiB groß sein.");
            }
            let format = if ext == "png" { image::ImageFormat::Png } else { image::ImageFormat::Jpeg };
            let reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            let (w, h) = reader.into_dimensions().map_err(|_| "Ungültige Bildschablone.")?;
            if w == 0 || h == 0 || w > 4096 || h > 4096 {
                return Err("Bildschablonen dürfen höchstens 4096 × 4096 Pixel groß sein.");
            }
            image::ImageReader::with_format(Cursor::new(bytes), format).decode().map_err(|_| "Bildschablone ist beschädigt.")?;
            Ok(())
        }
        "glb" => {
            if bytes.get(..4) != Some(b"glTF")
                || bytes.get(4..8) != Some(&2u32.to_le_bytes())
                || bytes.get(12..16).is_none()
                || bytes.get(16..20) != Some(b"JSON")
            {
                return Err("Ungültiger GLB-Container.");
            }
            let word = |at: usize| bytes.get(at..at + 4).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes);
            if word(8).map(u64::from) != Some(bytes.len() as u64) {
                return Err("GLB-Länge stimmt nicht überein.");
            }
            let size = word(12).ok_or("GLB-Metadaten fehlen.")? as usize;
            let v: Value = serde_json::from_slice(bytes.get(20..20 + size).ok_or("GLB-Metadaten fehlen.")?).map_err(|_| "Ungültiges GLB-JSON.")?;
            gltf(&v)
        }
        "gltf" => {
            let v: Value = serde_json::from_slice(bytes).map_err(|_| "Ungültiges glTF-JSON.")?;
            gltf(&v)
        }
        "stl"
            if bytes.len() >= 84
                && bytes
                    .get(80..84)
                    .and_then(|s| s.try_into().ok())
                    .map(u32::from_le_bytes)
                    .is_some_and(|n| 84u64 + u64::from(n) * 50 == bytes.len() as u64 && n > 0) =>
        {
            Ok(())
        }
        _ => {
            let text = std::str::from_utf8(bytes).map_err(|_| "Dieses Austauschformat muss als Textdatei vorliegen.")?;
            match ext.as_str() {
                "step" | "stp" if text.trim_start().starts_with("ISO-10303-21;") && text.contains("END-ISO-10303-21;") && text.contains("DATA;") => {
                    Ok(())
                }
                "igs" | "iges"
                    if text.lines().any(|l| l.as_bytes().get(72) == Some(&b'T')) && text.lines().any(|l| l.as_bytes().get(72) == Some(&b'S')) =>
                {
                    Ok(())
                }
                "stl" if text.trim_start().starts_with("solid") && text.contains("facet normal") && text.contains("endsolid") => Ok(()),
                "obj" if text.lines().any(|l| l.starts_with("v ")) && text.lines().any(|l| l.starts_with("f ")) => {
                    if text.lines().any(|l| l.trim_start().starts_with("mtllib ")) {
                        Err("OBJ-Begleitmaterialien werden nicht hochgeladen. Bitte eine selbstenthaltende Geometriedatei oder GLB verwenden.")
                    } else {
                        Ok(())
                    }
                }
                "dxf" if text.lines().count() <= 200000 && text.lines().any(|l| l.trim() == "SECTION") && text.lines().any(|l| l.trim() == "EOF") => {
                    Ok(())
                }
                "svg" => {
                    if bytes.len() > 1 << 20 {
                        return Err("SVG-Schablonen dürfen höchstens 1 MiB groß sein.");
                    }
                    let doc = roxmltree::Document::parse(text).map_err(|_| "Ungültiges SVG.")?;
                    if doc.root_element().tag_name().name() != "svg" || doc.descendants().count() > 20000 {
                        return Err("SVG-Struktur nicht unterstützt.");
                    }
                    for node in doc.descendants().filter(|n| n.is_element()) {
                        if matches!(
                            node.tag_name().name().to_ascii_lowercase().as_str(),
                            "style" | "script" | "foreignobject" | "iframe" | "object" | "embed"
                        ) {
                            return Err("Aktive SVG-Inhalte sind nicht erlaubt.");
                        }
                        for attr in node.attributes() {
                            let key = attr.name().to_ascii_lowercase();
                            let val = attr.value().trim();
                            if key.starts_with("on") || key == "href" && !val.starts_with('#') || val.to_ascii_lowercase().contains("url(") {
                                return Err("SVG darf keine aktiven oder externen Ressourcen enthalten.");
                            }
                        }
                    }
                    Ok(())
                }
                _ => Err("Dateiinhalt passt nicht zum gewählten unterstützten Format."),
            }
        }
    }
}
