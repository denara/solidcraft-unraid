//! The model cache: the evaluated bodies of a design's timeline, saved with it so opening need
//! not rebuild them. Its SolidCraft schema is independent of the producing kernel; its key ties it
//! to the exact document, build and platform that computed it (docs/design-container.md).
use super::*;
use solidcraft_doc::{ModelBody, ModelState};

const FORMAT: &str = "solidcraft-model-cache/1";
const PRODUCER: &str = "solidcraft-truck-adapter/3";
const MANIFEST: &str = "cache/model.json";
const PAYLOAD: &str = "cache/bodies.bin";
/// Body records (a placed copy is one record; its part is stored once).
const MAX_RECORDS: usize = 200_000;
/// Distinct stored bodies.
const MAX_UNIQUE: usize = 100_000;
const MAX_MANIFEST: usize = 32 << 20;

/// The build that computed a cache: the same source, compiled for the same platform, computes the
/// same numbers, so only its own caches are its results.
fn build() -> String {
    format!("{}/{}-{}", env!("SOLIDCRAFT_SOURCE_COMMIT"), std::env::consts::ARCH, std::env::consts::OS)
}

/// A 4 x 4 matrix as its 16 numbers, in storage order.
fn matrix(t: &[[f64; 4]; 4]) -> Vec<f64> {
    t.iter().flatten().copied().collect()
}

fn look(look: &solidcraft_doc::appearance::ImportedLook) -> Value {
    let faces: Vec<Value> = look.faces.iter().map(|(name, (color, opacity))| json!([name, color, opacity])).collect();
    json!({"color": look.color, "opacity": look.opacity, "faces": faces})
}

fn load_look(v: &Value) -> Result<solidcraft_doc::appearance::ImportedLook> {
    let unit = |x: f32| x.is_finite() && (0.0..=1.0).contains(&x);
    let color = |v: &Value| -> Result<Option<[f32; 3]>> {
        let c: Option<[f32; 3]> = serde_json::from_value(v.clone()).map_err(invalid)?;
        if c.is_some_and(|c| !c.iter().copied().all(unit)) {
            return Err(invalid("cached colour"));
        }
        Ok(c)
    };
    let opacity: Option<f32> = serde_json::from_value(v.get("opacity").cloned().unwrap_or(Value::Null)).map_err(invalid)?;
    if opacity.is_some_and(|o| !unit(o)) {
        return Err(invalid("cached opacity"));
    }
    let mut faces = BTreeMap::new();
    for f in v.get("faces").and_then(Value::as_array).ok_or_else(|| invalid("cached face looks"))? {
        let name = f.get(0).and_then(Value::as_str).filter(|n| n.len() <= 1024).ok_or_else(|| invalid("cached face name"))?;
        let c = color(f.get(1).unwrap_or(&Value::Null))?.ok_or_else(|| invalid("cached face colour"))?;
        let o = f.get(2).and_then(Value::as_f64).map(|o| o as f32).filter(|o| unit(*o)).ok_or_else(|| invalid("cached face opacity"))?;
        faces.insert(name.to_owned(), (c, o));
    }
    Ok(solidcraft_doc::appearance::ImportedLook { color: color(v.get("color").unwrap_or(&Value::Null))?, opacity, faces })
}

/// Encode a model state's bodies. Identical bodies (the same part placed many times, a body a
/// feature passed through) are stored once; a placed copy of a part records the part and its
/// matrix, so it shows that part's display as it did before saving.
pub(super) fn encode_model(state: &ModelState, document: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    if state.bodies.is_empty() || state.bodies.len() > MAX_RECORDS {
        return Err(invalid("cached body count"));
    }
    // What to encode: every body, and the part every placed copy shows.
    let mut shares: Vec<solidcraft_kernel::DisplayShare> = Vec::new();
    let mut share_index = std::collections::HashMap::new();
    for b in &state.bodies {
        if let Some((share, _)) = b.body.display_share() {
            share_index.entry(share.id()).or_insert_with(|| {
                shares.push(share.clone());
                shares.len() - 1
            });
        }
    }
    let bodies: Vec<&solidcraft_kernel::Body> = state.bodies.iter().map(|b| &b.body).chain(shares.iter().map(|s| s.original())).collect();
    let encoded = solidcraft_kernel::map_on_threads(&bodies, 16, |b| solidcraft_kernel::encode_native_body(b).ok());
    let mut data = Vec::new();
    let mut slices = Vec::new();
    let mut by_digest = std::collections::HashMap::new();
    let mut index = Vec::with_capacity(encoded.len());
    for bytes in encoded {
        let bytes = bytes.flatten().ok_or_else(|| invalid("body not stored exactly"))?;
        let next = slices.len();
        let i = *by_digest.entry(Sha256::digest(&bytes)).or_insert(next);
        if i == next {
            if data.len().saturating_add(bytes.len()) > MAX_DESIGN_BYTES || slices.len() >= MAX_UNIQUE {
                return Err(invalid("model cache size"));
            }
            slices.push(json!([data.len(), bytes.len()]));
            data.extend(bytes);
        }
        index.push(i);
    }
    let (body_index, share_body) = index.split_at(state.bodies.len());
    let mut records = Vec::with_capacity(state.bodies.len());
    for (b, i) in state.bodies.iter().zip(body_index) {
        if b.name.len() > 1024 {
            return Err(invalid("cached body name"));
        }
        let mut r = json!({"name": b.name, "body": i, "feature": b.feature, "component": b.component});
        if let Some(source) = &b.source_name {
            r["source_name"] = json!(source);
        }
        if b.source_transform != solidcraft_doc::IDENTITY {
            r["source_transform"] = json!(matrix(&b.source_transform));
        }
        if b.transferred {
            r["transferred"] = json!(true);
        }
        if let Some(l) = &b.imported {
            r["imported"] = look(l);
        }
        if let Some((share, t)) = b.body.display_share() {
            let s = share_index.get(&share.id()).ok_or_else(|| invalid("display share"))?;
            r["share"] = json!([s, matrix(&t)]);
        }
        records.push(r);
    }
    let manifest = serde_json::to_vec(&json!({
        "format": FORMAT, "producer": PRODUCER, "app": env!("CARGO_PKG_VERSION"), "build": build(),
        "document": digest(document), "payload": digest(&data),
        "bodies": slices, "shares": share_body, "body_counter": state.body_counter, "state": records,
    }))
    .map_err(invalid)?;
    if manifest.len() > MAX_MANIFEST {
        return Err(invalid("model cache manifest size"));
    }
    Ok((manifest, data))
}

/// The cached model state of a design, when it holds one this build computed for exactly this
/// document. Any failure is a miss, never a document error.
pub fn read_design_geometry(bytes: &[u8]) -> Option<ModelState> {
    if bytes.len() > MAX_DESIGN_BYTES || !is_design_container(bytes) {
        return None;
    }
    DesignContainerReader::new(Cursor::new(bytes)).ok()?.geometry()
}

/// Whether `manifest` is a model cache this build wrote for `document`.
fn current(manifest: &Value, document: &[u8]) -> bool {
    let is = |key: &str, value: &str| manifest.get(key).and_then(Value::as_str) == Some(value);
    is("format", FORMAT)
        && is("producer", PRODUCER)
        && is("app", env!("CARGO_PKG_VERSION"))
        && is("build", &build())
        && is("document", &digest(document))
        // A development build of unknown source never trusts a cache.
        && env!("SOLIDCRAFT_SOURCE_COMMIT") != "unknown"
}

pub(super) fn read_geometry_with<R: Read + Seek>(reader: &mut PackageReader<R>, document: &[u8]) -> Result<ModelState> {
    let container: Value = serde_json::from_slice(&reader.limited_part("container.json", 4096)?).map_err(invalid)?;
    if container.get("format").and_then(Value::as_str) != Some(VERSION) {
        return Err(invalid("container version"));
    }
    let manifest: Value = serde_json::from_slice(&reader.limited_part(MANIFEST, MAX_MANIFEST)?).map_err(invalid)?;
    if !current(&manifest, document) {
        return Err(invalid("model cache key"));
    }
    // Validate the small parametric metadata before decoding any geometry.
    Document::from_json(std::str::from_utf8(document).map_err(invalid)?)?;
    let data = reader.limited_part(PAYLOAD, MAX_DESIGN_BYTES)?;
    if manifest.get("payload").and_then(Value::as_str) != Some(digest(&data).as_str()) {
        return Err(invalid("model cache payload key"));
    }
    let count = |v: &Value| v.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| invalid("model cache number"));
    let slices = manifest
        .get("bodies")
        .and_then(Value::as_array)
        .filter(|b| !b.is_empty() && b.len() <= MAX_UNIQUE)
        .ok_or_else(|| invalid("cached bodies"))?;
    let mut end = 0usize;
    let mut parts = Vec::with_capacity(slices.len());
    for s in slices {
        let (offset, length) = (count(s.get(0).unwrap_or(&Value::Null))?, count(s.get(1).unwrap_or(&Value::Null))?);
        if offset != end {
            return Err(invalid("overlapping cached bodies"));
        }
        end = end.checked_add(length).ok_or_else(|| invalid("cached body length"))?;
        parts.push(data.get(offset..end).ok_or_else(|| invalid("truncated cached body"))?);
    }
    if end != data.len() {
        return Err(invalid("trailing cached data"));
    }
    let decoded = solidcraft_kernel::map_on_threads(&parts, 16, |bytes| solidcraft_kernel::decode_native_body(bytes).ok());
    let bodies = decoded.into_iter().map(Option::flatten).collect::<Option<Vec<_>>>().ok_or_else(|| invalid("cached body"))?;
    let body = |v: &Value| bodies.get(count(v)?).ok_or_else(|| invalid("cached body index"));
    let shares = manifest
        .get("shares")
        .and_then(Value::as_array)
        .filter(|s| s.len() <= MAX_UNIQUE)
        .ok_or_else(|| invalid("cached shares"))?
        .iter()
        .map(|i| Ok(solidcraft_kernel::DisplayShare::new(body(i)?.clone())))
        .collect::<Result<Vec<_>>>()?;
    let records =
        manifest.get("state").and_then(Value::as_array).filter(|s| !s.is_empty() && s.len() <= MAX_RECORDS).ok_or_else(|| invalid("cached state"))?;
    let mut state = ModelState::default();
    state.body_counter = count(manifest.get("body_counter").unwrap_or(&Value::Null))?;
    for r in records {
        let name = r.get("name").and_then(Value::as_str).filter(|n| n.len() <= 1024).ok_or_else(|| invalid("cached body name"))?;
        let mut kernel_body = body(r.get("body").unwrap_or(&Value::Null))?.clone();
        if let Some(share) = r.get("share") {
            let s = shares.get(count(share.get(0).unwrap_or(&Value::Null))?).ok_or_else(|| invalid("cached share index"))?;
            let numbers: Vec<f64> = serde_json::from_value(share.get(1).cloned().unwrap_or(Value::Null)).map_err(invalid)?;
            let t = matrix_of(&numbers)?;
            kernel_body = kernel_body.with_display_share(s, t);
        }
        let feature = r.get("feature").and_then(Value::as_u64).ok_or_else(|| invalid("cached body feature"))?;
        let mut b = ModelBody::new(name.to_owned(), kernel_body, feature);
        b.component = r.get("component").and_then(Value::as_u64).ok_or_else(|| invalid("cached body component"))?;
        b.source_name = match r.get("source_name") {
            None => None,
            Some(v) => Some(v.as_str().filter(|n| n.len() <= 1024).ok_or_else(|| invalid("cached source name"))?.to_owned()),
        };
        if let Some(v) = r.get("source_transform") {
            let numbers: Vec<f64> = serde_json::from_value(v.clone()).map_err(invalid)?;
            b.source_transform = matrix_of(&numbers)?;
        }
        b.transferred = r.get("transferred").and_then(Value::as_bool).unwrap_or(false);
        if let Some(l) = r.get("imported") {
            b.imported = Some(Arc::new(load_look(l)?));
        }
        state.bodies.push(b);
    }
    Ok(state)
}

/// A 4 x 4 matrix from its 16 numbers in storage order, finite.
fn matrix_of(numbers: &[f64]) -> Result<[[f64; 4]; 4]> {
    if numbers.len() != 16 || numbers.iter().any(|x| !x.is_finite() || x.abs() > 1e12) {
        return Err(invalid("cached matrix"));
    }
    let mut t = [[0.0; 4]; 4];
    for (slot, x) in t.iter_mut().flatten().zip(numbers) {
        *slot = *x;
    }
    Ok(t)
}

pub(super) fn geometry_preview(document: &[u8], state: ModelState) -> Result<DesignPreview> {
    let doc = Document::from_json(std::str::from_utf8(document).map_err(invalid)?)?;
    let world = solidcraft_doc::world_state(&doc, &state);
    let mut preview = DesignPreview::default();
    let mut triangles = 0usize;
    let mut display_bytes = 0usize;
    let faces: usize = world.bodies.iter().map(|b| b.body.face_count()).sum();
    let detail = if faces > 20_000 {
        1
    } else if faces > 4_000 {
        4
    } else {
        8
    };
    let mut work = 1_000_000usize;
    for entry in &world.bodies {
        let mesh = solidcraft_kernel::native_preview_mesh(&entry.body, detail, &mut work).map_err(invalid)?;
        triangles = triangles.saturating_add(mesh.triangles.len());
        display_bytes = display_bytes.saturating_add(mesh.positions.len().saturating_mul(24)).saturating_add(mesh.triangles.len().saturating_mul(12));
        if triangles > 1_000_000 || display_bytes > 128 << 20 {
            return Err(invalid("regenerated display budget"));
        }
        let displayed = MeshData {
            name: entry.name.clone(),
            positions: mesh.positions.iter().flat_map(|p| [p.x, p.y, p.z]).collect(),
            triangles: mesh.triangles.iter().flatten().copied().collect(),
            color: entry.body.color(),
        };
        validate_mesh(&displayed)?;
        preview.meshes.push(displayed);
    }
    Ok(preview)
}

pub(super) const ENTRIES: [&str; 2] = [MANIFEST, PAYLOAD];
