//! Explicit cookie/CSRF-bound project creation and bounded atomic file uploads.
use crate::sessions::State as AppState;
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
#[path = "workspace_upload_formats.rs"]
pub(crate) mod formats;
const FILE_LIMIT: u64 = 16 << 20;
const BATCH_LIMIT: u64 = 128 << 20;
const FILE_COUNT: usize = 100;
#[derive(Clone)]
struct Item {
    path: String,
    name: String,
    size: u64,
    revision: Option<String>,
    used: bool,
}
struct Batch {
    cancelled: Arc<AtomicBool>,
    owner: String,
    expires: Instant,
    items: Vec<Item>,
}
pub(crate) struct Uploads {
    batches: AsyncMutex<HashMap<String, Batch>>,
    slots: Arc<Semaphore>,
    preflights: Arc<Semaphore>,
    pub(crate) staging: PathBuf,
    staging_ready: Mutex<bool>,
    pub(crate) storage: Arc<crate::workspace_storage::Storage>,
}
impl Uploads {
    pub(crate) fn new(config: &crate::config::Config) -> Self {
        Self {
            batches: Default::default(),
            slots: Arc::new(Semaphore::new(2)),
            preflights: Arc::new(Semaphore::new(2)),
            staging: config
                .files
                .parent()
                .unwrap_or(&config.files)
                .join(format!(".workspace-upload-{}", config.files.file_name().and_then(|v| v.to_str()).unwrap_or("files"))),
            staging_ready: Mutex::new(false),
            storage: Default::default(),
        }
    }
    fn reserve(&self, size: u64) -> Result<PathBuf, &'static str> {
        let mut ready = self.staging_ready.lock().map_err(|_| "Upload-Speicher nicht verfügbar.")?;
        if !*ready {
            initialize_staging(&self.staging)?;
            *ready = true;
        }
        let mut total = 0u64;
        for (count, e) in std::fs::read_dir(&self.staging).map_err(|_| "Upload-Speicher nicht verfügbar.")?.enumerate() {
            if count > 64 {
                return Err("Temporärer Upload-Speicher enthält zu viele Einträge.");
            }
            let e = e.map_err(|_| "Upload-Speicher nicht verfügbar.")?;
            let m = e.metadata().map_err(|_| "Upload-Speicher nicht verfügbar.")?;
            total = total.saturating_add(m.len());
        }
        if total.saturating_add(size) > 2 * FILE_LIMIT + 1024 {
            return Err("Temporärer Upload-Speicher ist belegt.");
        }
        let path = self.staging.join(format!("{}.part", uuid::Uuid::new_v4()));
        let file =
            std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_| "Upload-Speicher kann nicht reserviert werden.")?;
        if file.set_len(size).is_err() {
            let _ = std::fs::remove_file(&path);
            return Err("Upload-Speicher kann nicht reserviert werden.");
        }
        Ok(path)
    }
    pub(crate) fn limits() -> Value {
        json!({"file_bytes":FILE_LIMIT,"batch_bytes":BATCH_LIMIT,"files":FILE_COUNT,"parallel":2,"extensions":formats::EXTENSIONS})
    }
}
fn failure(code: StatusCode, message: &str) -> Response {
    crate::workspace::protect((code, Json(json!({"error":message}))).into_response())
}
fn success(value: Value) -> Response {
    crate::workspace::protect(Json(value).into_response())
}
async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<String, Response> {
    let owner = crate::workspace::browser_session(state, headers).await.ok_or_else(|| failure(StatusCode::UNAUTHORIZED, "Bitte erneut anmelden."))?;
    if crate::workspace::check_origin(state, headers).is_err()
        || headers.get("x-workspace-csrf").and_then(|v| v.to_str().ok()) != Some(owner.as_str())
    {
        return Err(failure(StatusCode::FORBIDDEN, "Herkunft oder Formularbestätigung ist ungültig."));
    }
    Ok(owner)
}
fn json_form(bytes: &[u8], allowed: &[&str]) -> Result<Value, &'static str> {
    let data: Value = serde_json::from_slice(bytes).map_err(|_| "Ungültiges Formular.")?;
    if !data.as_object().is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str()))) {
        return Err("Ungültige Formularfelder.");
    }
    Ok(data)
}
fn real_directory(root: &Path, name: &str) -> Result<PathBuf, String> {
    let project = name.split('/').next().ok_or("Projekt fehlt.")?;
    crate::projects::validate(project)?;
    let path = crate::paths::resolve(root, name)?;
    let meta = std::fs::symlink_metadata(&path).map_err(|_| "Projektordner ist nicht vorhanden.")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("Ziel ist kein echter Projektordner.".into());
    }
    Ok(path)
}
fn file_name(name: &str) -> Result<(), &'static str> {
    if name.len() > 240 || name.contains('/') || crate::projects::validate(name).is_err() {
        return Err("Dateiname ist nicht zulässig.");
    }
    formats::extension(name)?;
    Ok(())
}
fn spelling(directory: &Path, name: &str) -> Result<(), &'static str> {
    for (n, entry) in std::fs::read_dir(directory).map_err(|_| "Zielordner ist nicht lesbar.")?.enumerate() {
        if n >= 4096 {
            return Err("Zielordner enthält zu viele Einträge.");
        }
        let entry = entry.map_err(|_| "Zielordner ist nicht lesbar.")?;
        let existing = entry.file_name();
        let existing = existing.to_string_lossy();
        if existing != name && existing.eq_ignore_ascii_case(name) {
            return Err("Eine Datei mit abweichender Groß-/Kleinschreibung ist bereits vorhanden.");
        }
    }
    Ok(())
}
pub(crate) async fn create_project(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    let owner = match authorize(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let data = match json_form(&bytes, &["name"]) {
        Ok(v) => v,
        Err(r) => return failure(StatusCode::BAD_REQUEST, r),
    };
    let Some(name) = data["name"].as_str().map(str::to_owned) else { return failure(StatusCode::BAD_REQUEST, "Projektname fehlt.") };
    if let Err(e) = crate::projects::validate(&name) {
        return failure(StatusCode::BAD_REQUEST, &e);
    }
    let chosen = name.clone();
    let result = tokio::spawn(async move {
        let _gate = state.file_gate.lock().await;
        if crate::workspace::browser_session(&state, &headers).await.as_deref() != Some(owner.as_str()) {
            return Err("Bitte erneut anmelden.".into());
        }
        let root = state.config.files.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let projects = crate::projects::list(&root)?;
            if projects["truncated"] == true || projects["projects"].as_array().is_none_or(|p| p.len() >= 256) {
                return Err("Projektgrenze erreicht.".into());
            }
            spelling(&root, &chosen).map_err(str::to_owned)?;
            let target = crate::paths::resolve(&root, &chosen)?;
            std::fs::create_dir(target).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    "Projektname ist bereits vorhanden.".into()
                } else {
                    "Projekt konnte nicht angelegt werden.".into()
                }
            })
        })
        .await
        .map_err(|_| "Projektanlage nicht verfügbar.".to_owned())?;
        state.browser.uploads.storage.invalidate();
        result
    })
    .await;
    match result {
        Ok(Ok(())) => success(json!({"project":name,"created":true})),
        Ok(Err(e)) => failure(StatusCode::CONFLICT, &e),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "Projektanlage nicht verfügbar."),
    }
}
pub(crate) async fn cancel(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    let owner = match authorize(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let data = match json_form(&bytes, &["batch"]) {
        Ok(v) => v,
        Err(r) => return failure(StatusCode::BAD_REQUEST, r),
    };
    let Some(id) = data["batch"].as_str() else { return failure(StatusCode::BAD_REQUEST, "Upload-Bestätigung fehlt.") };
    let mut batches = state.browser.uploads.batches.lock().await;
    if batches.get(id).is_some_and(|b| b.owner == owner)
        && let Some(batch) = batches.remove(id)
    {
        batch.cancelled.store(true, Ordering::Release);
    }
    success(json!({"cancelled":true}))
}
pub(crate) async fn prepare(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    let owner = match authorize(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Ok(_slot) = Arc::clone(&state.browser.uploads.preflights).try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Eine Upload-Prüfung läuft bereits.");
    };
    let preflight = Arc::new(_slot);
    let data = match json_form(&bytes, &["project", "files"]) {
        Ok(v) => v,
        Err(r) => return failure(StatusCode::BAD_REQUEST, r),
    };
    let Some(project) = data["project"].as_str().map(str::to_owned) else {
        return failure(StatusCode::BAD_REQUEST, "Bitte einen Projektordner auswählen.");
    };
    let Some(files) = data["files"].as_array().filter(|a| !a.is_empty() && a.len() <= FILE_COUNT) else {
        return failure(StatusCode::BAD_REQUEST, "Bitte 1 bis 100 Dateien auswählen.");
    };
    let mut names = HashSet::new();
    let mut total = 0u64;
    let mut items = Vec::new();
    for file in files {
        let Some(name) = file["name"].as_str() else { return failure(StatusCode::BAD_REQUEST, "Dateiname fehlt.") };
        if let Err(e) = file_name(name) {
            return failure(StatusCode::BAD_REQUEST, e);
        }
        let Some(size) = file["size"].as_u64().filter(|n| *n > 0 && *n <= FILE_LIMIT) else {
            return failure(StatusCode::PAYLOAD_TOO_LARGE, "Dateien dürfen höchstens 16 MiB groß und nicht leer sein.");
        };
        total = total.saturating_add(size);
        if total > BATCH_LIMIT || !names.insert(name.to_ascii_lowercase()) {
            return failure(StatusCode::BAD_REQUEST, "Ein Upload darf höchstens 128 MiB und keine doppelten Namen enthalten.");
        }
        items.push(Item { path: format!("{project}/{name}"), name: name.into(), size, revision: None, used: false });
    }
    let _gate = state.file_gate.lock().await;
    if let Err(e) = crate::workspace_actions::check_sessions(&state, &project).await {
        return failure(StatusCode::CONFLICT, e);
    }
    let root = state.config.files.clone();
    let chosen = project.clone();
    let checked = tokio::task::spawn_blocking(move || -> Result<Vec<Item>, String> {
        let _slot = preflight;
        let directory = real_directory(&root, &chosen)?;
        let mut old_total = 0u64;
        for item in &mut items {
            spelling(&directory, &item.name).map_err(str::to_owned)?;
            let path = crate::paths::resolve(&root, &item.path)?;
            match std::fs::symlink_metadata(path) {
                Ok(m) => {
                    old_total = old_total.saturating_add(m.len());
                    if !m.is_file() || m.len() > FILE_LIMIT || old_total > BATCH_LIMIT {
                        return Err("Bestehende Zieldateien überschreiten die Upload-Prüfgrenze.".into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("Zieldatei kann nicht geprüft werden.".into()),
            }
            item.revision = crate::workspace_files::revision(&root, &item.path)?;
        }
        Ok(items)
    })
    .await;
    let items = match checked {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return failure(StatusCode::CONFLICT, &e),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Upload-Prüfung nicht verfügbar."),
    };
    let mut batches = state.browser.uploads.batches.lock().await;
    batches.retain(|_, b| b.expires > Instant::now());
    if batches.len() >= 32 || batches.values().filter(|b| b.owner == owner).count() >= 4 {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Zu viele offene Uploads. Bitte kurz warten.");
    }
    let id = uuid::Uuid::new_v4().to_string();
    let rows: Vec<_> = items.iter().enumerate().map(|(index, i)| json!({"index":index,"name":i.name,"size":i.size,"revision":i.revision})).collect();
    batches
        .insert(id.clone(), Batch { cancelled: Arc::new(AtomicBool::new(false)), owner, expires: Instant::now() + Duration::from_secs(600), items });
    success(json!({"batch":id,"project":project,"files":rows,"limits":Uploads::limits()}))
}
struct Lease {
    transfers: crate::workspace_actions::Transfers,
    id: String,
    staged: PathBuf,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut map) = self.transfers.lock() {
            map.remove(&self.id);
        }
        let _ = std::fs::remove_file(&self.staged);
    }
}
fn initialize_staging(root: &Path) -> Result<(), &'static str> {
    match std::fs::create_dir(root) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Temporärer Upload-Speicher nicht verfügbar."),
    }
    let m = std::fs::symlink_metadata(root).map_err(|_| "Temporärer Upload-Speicher nicht verfügbar.")?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err("Temporärer Upload-Speicher nicht verfügbar.");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).map_err(|_| "Upload-Speicher kann nicht geschützt werden.")?;
    }
    let marker = root.join(".solidcraft-upload-owner");
    if marker.exists() {
        if std::fs::symlink_metadata(&marker).map_err(|_| "Upload-Marker nicht verfügbar.")?.file_type().is_symlink()
            || std::fs::read(&marker).ok().as_deref() != Some(b"solidcraft-upload-staging/1")
        {
            return Err("Upload-Speicher ist nicht eindeutig zugeordnet.");
        }
    } else {
        if std::fs::read_dir(root).map_err(|_| "Upload-Speicher nicht verfügbar.")?.next().is_some() {
            return Err("Vorhandener Upload-Ordner ist nicht zugeordnet.");
        }
        std::fs::write(&marker, b"solidcraft-upload-staging/1").map_err(|_| "Upload-Speicher kann nicht markiert werden.")?;
    }
    // Exactly one gateway owns a data volume. After restart only its marked UUID parts are discarded.
    for (count, e) in std::fs::read_dir(root).map_err(|_| "Upload-Speicher nicht verfügbar.")?.enumerate() {
        if count > 64 {
            return Err("Zu viele temporäre Upload-Dateien.");
        }
        let e = e.map_err(|_| "Upload-Speicher nicht verfügbar.")?;
        if e.file_name() == ".solidcraft-upload-owner" {
            continue;
        }
        let name = e.file_name();
        let name = name.to_str().ok_or("Unbekannter Upload-Eintrag.")?;
        if name.strip_suffix(".part").is_none_or(|s| uuid::Uuid::parse_str(s).is_err())
            || !e.file_type().map_err(|_| "Upload-Speicher nicht verfügbar.")?.is_file()
        {
            return Err("Unbekannter Upload-Eintrag wird nicht bereinigt.");
        }
        std::fs::remove_file(e.path()).map_err(|_| "Alter Upload konnte nicht bereinigt werden.")?;
    }
    Ok(())
}
async fn receive(mut body: Body, path: &Path, expected: u64) -> Result<(), &'static str> {
    let mut file = tokio::fs::OpenOptions::new().write(true).open(path).await.map_err(|_| "Upload kann nicht zwischengespeichert werden.")?;
    let mut size = 0u64;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| "Upload-Verbindung abgebrochen.")?;
        if let Ok(data) = frame.into_data() {
            size = size.saturating_add(data.len() as u64);
            if size > expected || size > FILE_LIMIT {
                return Err("Upload überschreitet die angekündigte Größe.");
            }
            file.write_all(&data).await.map_err(|_| "Upload fehlgeschlagen: Datenspeicher möglicherweise voll.")?;
        }
    }
    if size != expected {
        return Err("Upload ist unvollständig.");
    }
    file.sync_all().await.map_err(|_| "Upload konnte nicht gespeichert werden: Datenspeicher möglicherweise voll.")?;
    Ok(())
}
pub(crate) async fn upload(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let owner = match authorize(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) != Some("application/octet-stream") {
        return failure(StatusCode::BAD_REQUEST, "Ungültiger Upload-Inhaltstyp.");
    }
    let get = |k: &str| headers.get(k).and_then(|v| v.to_str().ok());
    let Some(batch) = get("x-upload-batch") else { return failure(StatusCode::BAD_REQUEST, "Upload-Bestätigung fehlt.") };
    let Some(index) = get("x-upload-index").and_then(|s| s.parse::<usize>().ok()) else {
        return failure(StatusCode::BAD_REQUEST, "Upload-Index fehlt.");
    };
    let Ok(_slot) = Arc::clone(&state.browser.uploads.slots).try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Zwei Uploads laufen bereits. Bitte gleich erneut versuchen.");
    };
    let mut batches = state.browser.uploads.batches.lock().await;
    let Some(b) = batches.get_mut(batch).filter(|b| b.owner == owner && b.expires > Instant::now()) else {
        return failure(StatusCode::CONFLICT, "Upload-Bestätigung abgelaufen. Bitte erneut auswählen.");
    };
    let cancelled = Arc::clone(&b.cancelled);
    let expires = b.expires;
    let Some(item) = b.items.get_mut(index).filter(|i| !i.used) else {
        return failure(StatusCode::CONFLICT, "Datei wurde bereits übertragen oder ist nicht Teil dieses Uploads.");
    };
    if let Some(revision) = item.revision.as_deref() {
        if get("x-upload-overwrite") != Some("true") || get("if-match") != Some(revision) {
            return failure(StatusCode::CONFLICT, "Überschreiben muss ausdrücklich mit der aktuellen Dateirevision bestätigt werden.");
        }
    } else if get("if-none-match") != Some("*") {
        return failure(StatusCode::PRECONDITION_REQUIRED, "Neue Datei benötigt eine Erstellungsvorbedingung.");
    }
    if get("content-length").is_some_and(|s| s.parse::<u64>().ok() != Some(item.size)) {
        return failure(StatusCode::BAD_REQUEST, "Upload-Größe stimmt nicht überein.");
    }
    item.used = true;
    let item = item.clone();
    drop(batches);
    let permit = Arc::new(_slot);
    let temp = match state.browser.uploads.reserve(item.size) {
        Ok(v) => v,
        Err(e) => return failure(StatusCode::SERVICE_UNAVAILABLE, e),
    };
    let transfers = Arc::clone(&state.browser.actions.transfers);
    let id = uuid::Uuid::new_v4().to_string();
    let lease = Lease { transfers: Arc::clone(&transfers), id: id.clone(), staged: temp.clone() };
    {
        let _gate = state.file_gate.lock().await;
        if let Err(e) = crate::workspace_actions::check_sessions(&state, &item.path).await {
            return failure(StatusCode::CONFLICT, e);
        }
        let Ok(mut map) = transfers.lock() else { return failure(StatusCode::SERVICE_UNAVAILABLE, "Dateizugriff nicht verfügbar.") };
        map.insert(id.clone(), item.path.clone());
    }
    match tokio::time::timeout(Duration::from_secs(120), receive(body, &temp, item.size)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return failure(StatusCode::BAD_REQUEST, e),
        Err(_) => return failure(StatusCode::REQUEST_TIMEOUT, "Upload hat das Zeitlimit überschritten."),
    }
    let path = temp.clone();
    let name = item.name.clone();
    let validating = Arc::clone(&permit);
    match tokio::task::spawn_blocking(move || {
        let _permit = validating;
        let bytes = std::fs::read(path).map_err(|_| "Upload ist nicht lesbar.")?;
        formats::validate(&name, &bytes)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return failure(StatusCode::UNPROCESSABLE_ENTITY, e),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Dateiprüfung fehlgeschlagen."),
    }
    // Detach only the short final transaction. The lease follows it if the client disconnects.
    let task = Arc::clone(&state);
    let auth_headers = headers.clone();
    let result = tokio::spawn(async move {
        let _permit = permit;
        let _gate = task.file_gate.lock().await;
        if cancelled.load(Ordering::Acquire) || expires <= Instant::now() {
            return Err("Upload abgebrochen oder abgelaufen; Datei wurde nicht veröffentlicht.".to_owned());
        }
        if crate::workspace::browser_session(&task, &auth_headers).await.as_deref() != Some(owner.as_str()) {
            return Err("Anmeldung beendet; Upload wurde nicht veröffentlicht.".to_owned());
        }
        if let Ok(mut map) = lease.transfers.lock() {
            map.remove(&lease.id);
        } else {
            return Err("Dateizugriff nicht verfügbar.".to_owned());
        }
        crate::workspace_actions::check_sessions(&task, &item.path).await.map_err(str::to_owned)?;
        let root = task.config.files.clone();
        let staged = lease.staged.clone();
        let name = item.path.clone();
        let expected = item.revision.clone();
        let revision = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let parent = name.rsplit_once('/').map(|(p, _)| p).ok_or("Projekt fehlt.")?;
            let dir = real_directory(&root, parent)?;
            let filename = name.rsplit('/').next().ok_or("Dateiname fehlt.")?;
            spelling(&dir, filename).map_err(str::to_owned)?;
            if crate::workspace_files::revision(&root, &name)? != expected {
                return Err("Datei wurde inzwischen geändert. Bitte erneut auswählen und Überschreiben neu bestätigen.".into());
            }
            let target = crate::paths::resolve(&root, &name)?;
            if expected.is_some() {
                std::fs::rename(&staged, &target).map_err(|_| "Datei konnte nicht atomar ersetzt werden.")?;
            } else {
                std::fs::hard_link(&staged, &target).map_err(|_| "Ziel wurde inzwischen angelegt oder ist nicht verfügbar.")?;
            }
            crate::workspace_files::revision(&root, &name)?.ok_or_else(|| "Neue Dateirevision fehlt.".into())
        })
        .await
        .map_err(|_| "Upload-Abschluss nicht verfügbar.".to_owned())??;
        task.browser.uploads.storage.invalidate();
        drop(lease);
        Ok((item.path, revision))
    })
    .await;
    match result {
        Ok(Ok((name, revision))) => success(json!({"name":name,"revision":revision,"uploaded":true})),
        Ok(Err(e)) => failure(StatusCode::CONFLICT, &e),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "Upload-Abschluss fehlgeschlagen. Bitte Übersicht prüfen."),
    }
}

#[cfg(test)]
mod staging_tests {
    use super::*;
    #[test]
    fn restart_cleans_only_owned_uuid_parts_and_reservation_is_bounded() {
        let dir = std::env::temp_dir().join(format!("solidcraft-upload-staging-{}", uuid::Uuid::new_v4()));
        initialize_staging(&dir).unwrap();
        let old = dir.join(format!("{}.part", uuid::Uuid::new_v4()));
        std::fs::write(&old, b"interrupted").unwrap();
        initialize_staging(&dir).unwrap();
        assert!(!old.exists());
        std::fs::write(dir.join("unrelated"), b"preserve").unwrap();
        assert!(initialize_staging(&dir).is_err());
        assert_eq!(std::fs::read(dir.join("unrelated")).unwrap(), b"preserve");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn simulated_full_device_reports_failure_without_publication() {
        let result = receive(Body::from("x"), Path::new("/dev/full"), 1).await;
        assert!(result.unwrap_err().contains("voll"));
    }
}
