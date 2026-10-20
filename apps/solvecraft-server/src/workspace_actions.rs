//! Explicitly confirmed file management; browser CAD and model observation stay independent.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

use crate::sessions::State as AppState;

#[path = "workspace_zip.rs"]
mod archive;
#[path = "workspace_manifest.rs"]
pub(crate) mod manifest;
pub(crate) use archive::download;

pub(crate) type Transfers = Arc<Mutex<HashMap<String, String>>>;
struct Confirmation {
    owner: String,
    path: String,
    revision: String,
    expires: Instant,
}
pub(crate) struct Actions {
    confirmations: AsyncMutex<HashMap<String, Confirmation>>,
    pub(crate) transfers: Transfers,
    pub(crate) slots: Arc<Semaphore>,
    checks: Arc<Semaphore>,
}
impl Default for Actions {
    fn default() -> Self {
        Self {
            confirmations: Default::default(),
            transfers: Default::default(),
            slots: Arc::new(Semaphore::new(2)),
            checks: Arc::new(Semaphore::new(2)),
        }
    }
}

#[cfg(test)]
impl Actions {
    pub(crate) async fn expire_confirmations(&self) {
        for confirmation in self.confirmations.lock().await.values_mut() {
            confirmation.expires = Instant::now();
        }
    }
}

fn overlaps(a: &str, b: &str) -> bool {
    a == b || a.strip_prefix(b).is_some_and(|tail| tail.starts_with('/')) || b.strip_prefix(a).is_some_and(|tail| tail.starts_with('/'))
}

pub(crate) fn check_transfers(transfers: &Transfers, name: &str) -> Result<(), &'static str> {
    let active = transfers.lock().map_err(|_| "Dateizugriff vorübergehend nicht verfügbar.")?;
    if active.values().any(|path| overlaps(path, name)) {
        return Err("Datei oder Ordner wird gerade als ZIP heruntergeladen. Bitte nach dem Download erneut versuchen.");
    }
    Ok(())
}

async fn check_sessions(state: &AppState, name: &str) -> Result<(), &'static str> {
    for entry in state.sessions.lock().await.values() {
        let model = entry.model.lock().map_err(|_| "Sitzungsstatus nicht verfügbar.")?;
        if model.project.as_deref().is_some_and(|project| overlaps(project, name))
            || model.current_file.as_deref().is_some_and(|file| overlaps(file, name))
        {
            return Err("Datei oder Projekt wird von einer aktiven MCP-Sitzung genutzt. Erst die Sitzung regulär beenden, dann erneut versuchen.");
        }
    }
    check_transfers(&state.browser.actions.transfers, name)
}

pub(crate) fn failure(status: StatusCode, message: &str) -> Response {
    crate::workspace::protect((status, Html(crate::workspace_view::action_failure(message))).into_response())
}

pub(crate) async fn preview(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(path): Path<String>) -> Response {
    let Some(owner) = crate::workspace::browser_session(&state, &headers).await else {
        return failure(StatusCode::UNAUTHORIZED, "Bitte erneut anmelden.");
    };
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return failure(StatusCode::FORBIDDEN, "Diese Herkunft ist nicht freigegeben.");
    }
    let Ok(_check) = Arc::clone(&state.browser.actions.checks).try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Eine Dateiprüfung läuft bereits. Bitte gleich erneut versuchen.");
    };
    let _gate = state.file_gate.lock().await;
    if let Err(error) = check_sessions(&state, &path).await {
        return failure(StatusCode::CONFLICT, error);
    }
    let root = state.config.files.clone();
    let target = path.clone();
    let manifest = match tokio::task::spawn_blocking(move || manifest::inspect(&root, &target, true)).await {
        Ok(Ok(manifest)) => manifest,
        Ok(Err(error)) => return failure(StatusCode::CONFLICT, error),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Dateiprüfung nicht verfügbar."),
    };
    let mut confirmations = state.browser.actions.confirmations.lock().await;
    confirmations.retain(|_, confirmation| confirmation.expires > Instant::now());
    if confirmations.len() >= 64 {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Zu viele offene Bestätigungen. Bitte kurz warten.");
    }
    let ticket = uuid::Uuid::new_v4().to_string();
    let html = crate::workspace_view::delete_confirmation(&path, &owner, &ticket, &manifest);
    confirmations.insert(ticket, Confirmation { owner, path, revision: manifest.revision, expires: Instant::now() + Duration::from_secs(300) });
    crate::workspace::protect(Html(html).into_response())
}

pub(crate) async fn delete(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    let Some(owner) = crate::workspace::browser_session(&state, &headers).await else {
        return failure(StatusCode::UNAUTHORIZED, "Bitte erneut anmelden.");
    };
    if crate::workspace::check_origin(&state, &headers).is_err() {
        return failure(StatusCode::FORBIDDEN, "Diese Herkunft ist nicht freigegeben.");
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v.split(';').next() != Some("application/x-www-form-urlencoded"))
    {
        return failure(StatusCode::BAD_REQUEST, "Ungültiges Bestätigungsformular.");
    }
    let Some(form) = std::str::from_utf8(&bytes).ok().and_then(crate::workspace::fields) else {
        return failure(StatusCode::BAD_REQUEST, "Ungültiges Bestätigungsformular.");
    };
    if form.get("csrf") != Some(&owner) || form.get("confirm").map(String::as_str) != Some("delete") {
        return failure(StatusCode::FORBIDDEN, "Löschbestätigung fehlt oder ist abgelaufen.");
    }
    let Some(ticket) = form.get("ticket") else { return failure(StatusCode::BAD_REQUEST, "Löschbestätigung fehlt.") };
    let mut confirmations = state.browser.actions.confirmations.lock().await;
    if confirmations.get(ticket).is_none_or(|confirmation| {
        confirmation.owner != owner || confirmation.expires <= Instant::now() || form.get("path") != Some(&confirmation.path)
    }) {
        return failure(StatusCode::CONFLICT, "Löschbestätigung ist abgelaufen. Bitte Inhalt erneut prüfen.");
    }
    let Some(confirmation) = confirmations.remove(ticket) else { return failure(StatusCode::CONFLICT, "Löschbestätigung fehlt.") };
    drop(confirmations);
    let Ok(_check) = Arc::clone(&state.browser.actions.checks).try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Eine Dateiprüfung läuft bereits. Bitte erneut bestätigen.");
    };
    // The detached transaction completes atomically with respect to all gateway writes,
    // even if the browser disconnects after explicitly confirming deletion.
    let task = Arc::clone(&state);
    match tokio::spawn(async move {
        let _gate = task.file_gate.lock().await;
        check_sessions(&task, &confirmation.path).await?;
        let root = task.config.files.clone();
        tokio::task::spawn_blocking(move || {
            let current = manifest::inspect(&root, &confirmation.path, true)?;
            if current.revision != confirmation.revision {
                return Err("Inhalt wurde seit der Bestätigung verändert. Bitte erneut prüfen und bestätigen.");
            }
            let path = crate::paths::resolve(&root, &confirmation.path).map_err(|_| "Ungültiger Pfad.")?;
            // All descendants were validated, including reserved files and symlinks.
            // remove_dir_all never follows symlinks; the shared gate excludes CAD/HTTP writes.
            if current.entries.first().is_some_and(|entry| entry.directory) {
                std::fs::remove_dir_all(path).map_err(|_| "Ordner konnte nicht vollständig gelöscht werden. Bitte Übersicht prüfen.")
            } else {
                std::fs::remove_file(path).map_err(|_| "Datei konnte nicht gelöscht werden.")
            }
        })
        .await
        .map_err(|_| "Löschen nicht verfügbar.")?
    })
    .await
    {
        Ok(Ok(())) => crate::workspace::protect((StatusCode::SEE_OTHER, [(header::LOCATION, "/workspace")]).into_response()),
        Ok(Err(error)) => failure(StatusCode::CONFLICT, error),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "Löschen nicht verfügbar. Bitte Übersicht prüfen."),
    }
}
