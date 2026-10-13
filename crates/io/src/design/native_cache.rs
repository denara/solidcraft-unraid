//! Optional native cache. Its SolidCraft schema is independent of the producing kernel.
use super::*;
const PRODUCER: &str = "solidcraft-truck-adapter/2";
/// Provisional evaluated geometry. Never an editable document state.
#[derive(Clone)]
pub struct DesignGeometry {
    pub name: String,
    pub body: solidcraft_kernel::Body,
}
pub(super) fn encode_geometry(bodies: &[DesignGeometry], document: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    if bodies.is_empty() || bodies.len() > 4096 {
        return Err(invalid("native body count"));
    }
    let mut data = Vec::new();
    let mut entries = Vec::new();
    let mut faces = 0usize;
    for entry in bodies {
        faces = faces.saturating_add(entry.body.face_count());
        if faces > 100_000 || entry.name.len() > 1024 {
            return Err(invalid("native geometry budget"));
        }
        let bytes = solidcraft_kernel::encode_native_body(&entry.body).map_err(invalid)?;
        let offset = data.len();
        if offset.saturating_add(bytes.len()) > 64 << 20 {
            return Err(invalid("native cache size"));
        }
        entries.push(json!({"name":entry.name,"offset":offset,"length":bytes.len()}));
        data.extend(bytes);
    }
    let manifest = serde_json::to_vec(&json!({
        "format":"solidcraft-brep-cache/1", "producer":PRODUCER,
        "app":env!("CARGO_PKG_VERSION"), "document":digest(document), "payload":digest(&data), "bodies":entries
    }))
    .map_err(invalid)?;
    if manifest.len() > 1 << 20 {
        return Err(invalid("native manifest size"));
    }
    Ok((manifest, data))
}

/// Read only metadata and the native cache. Original blobs are not read/decompressed
/// until the authoritative document reader needs them for background reconstruction.
/// Any optional geometry failure is a miss, never a document error.
pub fn read_design_geometry(bytes: &[u8]) -> Option<Vec<DesignGeometry>> {
    if bytes.len() > MAX_DESIGN_BYTES || !is_design_container(bytes) {
        return None;
    }
    DesignContainerReader::new(Cursor::new(bytes)).ok()?.geometry()
}
pub(super) fn read_geometry_with<R: Read + Seek>(reader: &mut PackageReader<R>, document: &[u8]) -> Result<Vec<DesignGeometry>> {
    let container: Value = serde_json::from_slice(&reader.limited_part("container.json", 4096)?).map_err(invalid)?;
    if container.get("format").and_then(Value::as_str) != Some(VERSION) {
        return Err(invalid("container version"));
    }
    let manifest: Value = serde_json::from_slice(&reader.limited_part("cache/key.json", 1 << 20)?).map_err(invalid)?;
    if manifest.get("format").and_then(Value::as_str) != Some("solidcraft-brep-cache/1")
        || manifest.get("producer").and_then(Value::as_str) != Some(PRODUCER)
        || manifest.get("app").and_then(Value::as_str) != Some(env!("CARGO_PKG_VERSION"))
    {
        return Err(invalid("native cache version"));
    }

    if manifest.get("document").and_then(Value::as_str) != Some(digest(document).as_str()) {
        return Err(invalid("native document key"));
    }
    // Validate the small parametric metadata before showing a provisional model.
    Document::from_json(std::str::from_utf8(document).map_err(invalid)?)?;
    let data = reader.limited_part("cache/brep.bin", 64 << 20)?;
    if manifest.get("payload").and_then(Value::as_str) != Some(digest(&data).as_str()) {
        return Err(invalid("native payload key"));
    }
    let entries =
        manifest.get("bodies").and_then(Value::as_array).filter(|b| !b.is_empty() && b.len() <= 4096).ok_or_else(|| invalid("native bodies"))?;
    let mut bodies = Vec::new();
    let mut end = 0usize;
    let mut faces = 0usize;
    for entry in entries {
        let n = |key| entry.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| invalid("native slice"));
        let offset = n("offset")?;
        let length = n("length")?;
        if offset != end {
            return Err(invalid("overlapping native bodies"));
        }
        end = end.checked_add(length).ok_or_else(|| invalid("native length overflow"))?;
        let name = entry.get("name").and_then(Value::as_str).filter(|n| n.len() <= 1024).ok_or_else(|| invalid("native body name"))?.to_owned();
        let body = solidcraft_kernel::decode_native_body(data.get(offset..end).ok_or_else(|| invalid("truncated native body"))?).map_err(invalid)?;
        faces = faces.saturating_add(body.face_count());
        if faces > 100_000 {
            return Err(invalid("native face budget"));
        }
        bodies.push(DesignGeometry { name, body });
    }
    if end != data.len() {
        return Err(invalid("trailing native data"));
    }
    Ok(bodies)
}
pub(super) fn geometry_preview(bodies: Vec<DesignGeometry>) -> Result<DesignPreview> {
    let mut preview = DesignPreview::default();
    let mut triangles = 0usize;
    let mut display_bytes = 0usize;
    let faces: usize = bodies.iter().map(|b| b.body.face_count()).sum();
    let detail = if faces > 20_000 {
        1
    } else if faces > 4_000 {
        4
    } else {
        8
    };
    let mut work = 1_000_000usize;
    for entry in bodies {
        let mesh = solidcraft_kernel::native_preview_mesh(&entry.body, detail, &mut work).map_err(invalid)?;
        triangles = triangles.saturating_add(mesh.triangles.len());
        display_bytes = display_bytes.saturating_add(mesh.positions.len().saturating_mul(24)).saturating_add(mesh.triangles.len().saturating_mul(12));
        if triangles > 1_000_000 || display_bytes > 128 << 20 {
            return Err(invalid("regenerated display budget"));
        }
        let displayed = MeshData {
            name: entry.name,
            positions: mesh.positions.iter().flat_map(|p| [p.x, p.y, p.z]).collect(),
            triangles: mesh.triangles.iter().flatten().copied().collect(),
            color: entry.body.color(),
        };
        validate_mesh(&displayed)?;
        preview.meshes.push(displayed);
    }
    Ok(preview)
}
