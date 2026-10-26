//! Accessible multi-file uploads. All authorization, type and write decisions are server-side.
use futures_channel::oneshot;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    AbortController, Document, Element, Event, File, FileList, HtmlDetailsElement, HtmlDialogElement, HtmlElement, HtmlInputElement,
    HtmlProgressElement, HtmlSelectElement,
};
#[derive(Default)]
struct Ui {
    running: Cell<bool>,
    cancelled: Cell<bool>,
    abort: RefCell<Option<AbortController>>,
    project: RefCell<String>,
}
#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Overwrite,
    Skip,
    Cancel,
}
fn doc() -> Option<Document> {
    web_sys::window()?.document()
}
fn element(id: &str) -> Option<Element> {
    doc()?.get_element_by_id(id)
}
fn label(id: &str, text: &str) {
    if let Some(el) = element(id) {
        el.set_text_content(Some(text));
    }
}
fn hidden(id: &str, yes: bool) {
    if let Some(el) = element(id) {
        if yes {
            let _ = el.set_attribute("hidden", "");
        } else {
            let _ = el.remove_attribute("hidden");
        }
    }
}
fn busy(ui: &Ui, yes: bool) {
    ui.running.set(yes);
    for id in ["upload-project", "upload-files", "project-name", "project-create-button"] {
        if let Some(el) = element(id) {
            if yes {
                let _ = el.set_attribute("disabled", "");
            } else {
                let _ = el.remove_attribute("disabled");
            }
        }
    }
    hidden("upload-cancel", !yes);
    hidden("upload-progress", !yes);
}
fn csrf() -> Result<String, String> {
    doc()
        .and_then(|d| d.query_selector("form[action='/workspace/logout'] input[name='csrf']").ok().flatten())
        .and_then(|e| e.get_attribute("value"))
        .filter(|s| s.len() == 64)
        .ok_or_else(|| "Bitte erneut anmelden.".into())
}
fn project() -> String {
    element("upload-project").and_then(|e| e.dyn_into::<HtmlSelectElement>().ok()).map(|s| s.value()).unwrap_or_default()
}
fn target_label() {
    let target = project();
    label(
        "upload-target",
        &if target.is_empty() { "Bitte zuerst einen echten Projektordner auswählen.".into() } else { format!("Upload-Ziel: {target}") },
    );
}
fn open_panel() {
    if let Some(e) = element("upload-panel").and_then(|e| e.dyn_into::<HtmlDetailsElement>().ok()) {
        e.set_open(true);
        e.scroll_into_view_with_bool(true);
    }
}
fn select_project(value: &str) {
    if let Some(select) = element("upload-project").and_then(|e| e.dyn_into::<HtmlSelectElement>().ok()) {
        select.set_value(value);
        if select.value() != value
            && let Some(d) = doc()
            && let Ok(option) = d.create_element("option")
        {
            let _ = option.set_attribute("value", value);
            option.set_text_content(Some(value));
            let _ = select.append_child(&option);
            select.set_value(value);
        }
    }
    target_label();
}
pub(crate) fn refresh(directories: &Value) {
    let Some(select) = element("upload-project").and_then(|e| e.dyn_into::<HtmlSelectElement>().ok()) else { return };
    let selected = select.value();
    let Some(d) = doc() else { return };
    select.set_text_content(None);
    for value in std::iter::once("").chain(directories.as_array().into_iter().flatten().filter_map(Value::as_str).take(256)) {
        if let Ok(option) = d.create_element("option") {
            let _ = option.set_attribute("value", value);
            option.set_text_content(Some(if value.is_empty() { "Projekt auswählen …" } else { value }));
            let _ = select.append_child(&option);
        }
    }
    select.set_value(&selected);
    target_label();
}
async fn response(response: JsValue) -> Result<Value, String> {
    let response = response.dyn_into::<web_sys::Response>().map_err(|_| "Antwort nicht verfügbar.".to_owned())?;
    if response.status() == 401 {
        crate::browser::expired();
        return Err("Bitte erneut anmelden.".into());
    }
    let ok = response.ok();
    let code = response.status();
    let text = JsFuture::from(response.text().map_err(|_| "Antwort nicht lesbar.")?)
        .await
        .map_err(|_| "Antwort nicht lesbar.")?
        .as_string()
        .filter(|s| s.len() < 128 * 1024)
        .ok_or("Ungültige Antwort.")?;
    let value = serde_json::from_str::<Value>(&text).map_err(|_| format!("Anfrage fehlgeschlagen (HTTP {code})."))?;
    if ok { Ok(value) } else { Err(value["error"].as_str().unwrap_or("Upload konnte nicht abgeschlossen werden.").chars().take(500).collect()) }
}
async fn api(path: &str, body: &Value) -> Result<Value, String> {
    let headers = web_sys::Headers::new().map_err(|_| "Browser-Anfrage nicht verfügbar.")?;
    headers.set("Content-Type", "application/json").map_err(|_| "Browser-Anfrage nicht verfügbar.")?;
    headers.set("X-Workspace-CSRF", &csrf()?).map_err(|_| "Browser-Anfrage nicht verfügbar.")?;
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_headers(headers.as_ref());
    init.set_body(&JsValue::from_str(&body.to_string()));
    let w = web_sys::window().ok_or("Browser nicht verfügbar.")?;
    response(JsFuture::from(w.fetch_with_str_and_init(path, &init)).await.map_err(|_| "Verbindung unterbrochen. Bitte erneut versuchen.")?).await
}
async fn send_file(ui: &Ui, batch: &str, row: &Value, file: &File, overwrite: bool) -> Result<Value, String> {
    let headers = web_sys::Headers::new().map_err(|_| "Browser-Anfrage nicht verfügbar.")?;
    let csrf = csrf()?;
    let index = row["index"].to_string();
    for (key, value) in [
        ("Content-Type", "application/octet-stream"),
        ("X-Workspace-CSRF", csrf.as_str()),
        ("X-Upload-Batch", batch),
        ("X-Upload-Index", index.as_str()),
    ] {
        headers.set(key, value).map_err(|_| "Browser-Anfrage nicht verfügbar.")?;
    }
    if let Some(revision) = row["revision"].as_str() {
        headers.set("If-Match", revision).map_err(|_| "Ungültige Dateirevision.")?;
        if overwrite {
            headers.set("X-Upload-Overwrite", "true").map_err(|_| "Ungültige Bestätigung.")?;
        }
    } else {
        headers.set("If-None-Match", "*").map_err(|_| "Browser-Anfrage nicht verfügbar.")?;
    }
    let abort = AbortController::new().map_err(|_| "Upload-Abbruch nicht verfügbar.")?;
    ui.abort.replace(Some(abort.clone()));
    let init = web_sys::RequestInit::new();
    init.set_method("PUT");
    init.set_headers(headers.as_ref());
    init.set_body(file.as_ref());
    init.set_signal(Some(&abort.signal()));
    let w = web_sys::window().ok_or("Browser nicht verfügbar.")?;
    let answer = JsFuture::from(w.fetch_with_str_and_init("/workspace/upload", &init)).await;
    ui.abort.replace(None);
    response(answer.map_err(|_| if ui.cancelled.get() { "Upload abgebrochen." } else { "Verbindung unterbrochen. Bitte Übersicht prüfen." })?).await
}
fn result(name: &str, message: &str, class: &str) {
    if let Some(d) = doc()
        && let Some(list) = d.get_element_by_id("upload-results")
        && let Ok(li) = d.create_element("li")
    {
        li.set_text_content(Some(&format!("{name}: {message}")));
        let _ = li.set_attribute("class", class);
        let _ = list.append_child(&li);
    }
}
async fn ask(name: &str, target: &str) -> Result<(Choice, bool), String> {
    let dialog = element("upload-overwrite").and_then(|e| e.dyn_into::<HtmlDialogElement>().ok()).ok_or("Bestätigung nicht verfügbar.")?;
    label("overwrite-name", &format!("{target}/{name}"));
    if let Some(check) = element("overwrite-all").and_then(|e| e.dyn_into::<HtmlInputElement>().ok()) {
        check.set_checked(false);
    }
    let (tx, rx) = oneshot::channel();
    let sender = Rc::new(RefCell::new(Some(tx)));
    let mut listeners = Vec::new();
    for (id, choice) in [
        ("overwrite-yes", Choice::Overwrite),
        ("overwrite-skip", Choice::Skip),
        ("overwrite-cancel", Choice::Cancel),
        ("upload-overwrite", Choice::Cancel),
    ] {
        let node = element(id).ok_or("Bestätigung nicht verfügbar.")?;
        let sender = Rc::clone(&sender);
        let event = if id == "upload-overwrite" { "cancel" } else { "click" };
        let listener = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            event.prevent_default();
            let all = element("overwrite-all").and_then(|e| e.dyn_into::<HtmlInputElement>().ok()).is_some_and(|c| c.checked());
            if let Some(tx) = sender.borrow_mut().take() {
                let _ = tx.send((choice, all));
            }
        });
        node.add_event_listener_with_callback(event, listener.as_ref().unchecked_ref()).map_err(|_| "Bestätigung nicht verfügbar.")?;
        listeners.push((node, event, listener));
    }
    let shown = dialog.show_modal().is_ok();
    let selected = if shown { rx.await.unwrap_or((Choice::Cancel, false)) } else { (Choice::Cancel, false) };
    for (node, event, listener) in listeners {
        let _ = node.remove_event_listener_with_callback(event, listener.as_ref().unchecked_ref());
    }
    dialog.close();
    Ok(selected)
}
async fn run_files(ui: Rc<Ui>, files: Vec<File>, target: String) -> Result<(), String> {
    let descriptors: Vec<_> = files.iter().map(|f| json!({"name":f.name(),"size":f.size() as u64})).collect();
    let prepared = api("/workspace/uploads/prepare", &json!({"project":target,"files":descriptors})).await?;
    let batch = prepared["batch"].as_str().ok_or("Upload-Bestätigung fehlt.")?.to_owned();
    let rows = prepared["files"].as_array().filter(|r| r.len() == files.len()).ok_or("Upload-Bestätigung unvollständig.")?;
    let mut complete = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut apply = None;
    let outcome = async {
        for (index, (file, row)) in files.iter().zip(rows).enumerate() {
            if ui.cancelled.get() {
                break;
            }
            label("upload-status", &format!("{} / {} · {} → {}", index + 1, files.len(), file.name(), target));
            let mut overwrite = false;
            if row["revision"].is_string() {
                let choice = if let Some(choice) = apply {
                    choice
                } else {
                    let (choice, all) = ask(&file.name(), &target).await?;
                    if all && choice != Choice::Cancel {
                        apply = Some(choice);
                    }
                    choice
                };
                match choice {
                    Choice::Cancel => {
                        ui.cancelled.set(true);
                        break;
                    }
                    Choice::Skip => {
                        skipped += 1;
                        result(&file.name(), "übersprungen", "");
                        continue;
                    }
                    Choice::Overwrite => overwrite = true,
                }
            }
            match send_file(&ui, &batch, row, file, overwrite).await {
                Ok(_) => {
                    complete += 1;
                    result(&file.name(), "hochgeladen", "done");
                }
                Err(e) => {
                    if ui.cancelled.get() {
                        break;
                    }
                    failed += 1;
                    result(&file.name(), &e, "failed");
                }
            }
            if let Some(progress) = element("upload-progress").and_then(|e| e.dyn_into::<HtmlProgressElement>().ok()) {
                progress.set_value((index + 1) as f64);
            }
        }
        Ok::<(), String>(())
    }
    .await;
    let _ = api("/workspace/uploads/cancel", &json!({"batch":batch})).await;
    label(
        "upload-status",
        &format!(
            "{} · {complete} hochgeladen, {skipped} übersprungen, {failed} fehlgeschlagen. Bereits gespeicherte Dateien bleiben erhalten.",
            if ui.cancelled.get() { "Upload abgebrochen" } else { "Upload abgeschlossen" }
        ),
    );
    outcome
}
fn start_files(ui: Rc<Ui>, list: FileList) {
    if ui.running.get() {
        label("upload-status", "Ein Upload läuft bereits. Das Ziel dieses Uploads bleibt unverändert.");
        return;
    }
    let target = project();
    if target.is_empty() {
        open_panel();
        label("upload-status", "Bitte zuerst einen echten Projektordner auswählen.");
        return;
    }
    if list.length() == 0 {
        return;
    }
    if list.length() > 100 {
        label("upload-status", "Maximal 100 Dateien je Upload.");
        return;
    }
    let files: Vec<_> = (0..list.length()).filter_map(|i| list.get(i)).collect();
    if files.iter().any(|f| f.size() == 0. || f.size() > 16777216.) || files.iter().map(|f| f.size()).sum::<f64>() > 134217728. {
        label("upload-status", "Dateien dürfen nicht leer sein und höchstens 16 MiB je Datei / 128 MiB je Upload umfassen.");
        return;
    }
    ui.cancelled.set(false);
    ui.project.replace(target.clone());
    busy(&ui, true);
    open_panel();
    if let Some(list) = element("upload-results") {
        list.set_text_content(None);
    }
    if let Some(progress) = element("upload-progress").and_then(|e| e.dyn_into::<HtmlProgressElement>().ok()) {
        progress.set_max(files.len() as f64);
        progress.set_value(0.);
    }
    let state = Rc::clone(&ui);
    spawn_local(async move {
        let result = run_files(Rc::clone(&state), files, target).await;
        busy(&state, false);
        state.abort.replace(None);
        if let Some(input) = element("upload-files").and_then(|e| e.dyn_into::<HtmlInputElement>().ok()) {
            input.set_value("");
        }
        if let Err(e) = result {
            label("upload-status", &e);
        }
    });
}
pub(crate) fn install(document: &Document) -> Result<(), JsValue> {
    let ui = Rc::new(Ui::default());
    if let Some(input) = element("upload-files") {
        let state = Rc::clone(&ui);
        let handler = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            if let Some(input) = event.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
                && let Some(files) = input.files()
            {
                start_files(Rc::clone(&state), files);
            }
        });
        input.add_event_listener_with_callback("change", handler.as_ref().unchecked_ref())?;
        handler.forget();
    }
    let state = Rc::clone(&ui);
    let drop = Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |event: web_sys::DragEvent| {
        event.prevent_default();
        if let Some(zone) = element("upload-dropzone") {
            let _ = zone.class_list().remove_1("dragover");
        }
        let group = event.target().and_then(|t| t.dyn_into::<Element>().ok()).and_then(|e| e.closest("[data-upload-project]").ok().flatten());
        if group.as_ref().is_some_and(|g| g.get_attribute("data-upload-project").is_none_or(|p| p.is_empty())) {
            open_panel();
            label(
                "upload-status",
                "Virtuelle Gruppen sind keine Upload-Ziele. Bitte einen echten Projektordner auswählen und die Drop-Zone verwenden.",
            );
            return;
        }
        if !state.running.get()
            && let Some(target) = event
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .and_then(|e| e.closest("[data-upload-project]").ok().flatten())
                .and_then(|e| e.get_attribute("data-upload-project"))
                .filter(|s| !s.is_empty())
        {
            select_project(&target);
        }
        if let Some(files) = event.data_transfer().and_then(|d| d.files()) {
            start_files(Rc::clone(&state), files);
        }
    });
    document.add_event_listener_with_callback("drop", drop.as_ref().unchecked_ref())?;
    drop.forget();
    let over = Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |event: web_sys::DragEvent| {
        event.prevent_default();
        if let Some(zone) = element("upload-dropzone") {
            let _ = zone.class_list().add_1("dragover");
        }
    });
    document.add_event_listener_with_callback("dragover", over.as_ref().unchecked_ref())?;
    over.forget();
    let leave = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(zone) = element("upload-dropzone") {
            let _ = zone.class_list().remove_1("dragover");
        }
    });
    document.add_event_listener_with_callback("dragleave", leave.as_ref().unchecked_ref())?;
    leave.forget();
    let state = Rc::clone(&ui);
    let click = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |event: web_sys::MouseEvent| {
        let target = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|e| e.closest("[data-upload-target]").ok().flatten())
            .and_then(|e| e.get_attribute("data-upload-target"));
        if let Some(target) = target {
            if state.running.get() {
                label("upload-status", "Ein Upload läuft bereits.");
            } else {
                select_project(&target);
                open_panel();
                if let Some(e) = element("upload-files").and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
                    let _ = e.focus();
                }
            }
        }
    });
    document.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
    click.forget();
    if let Some(select) = element("upload-project") {
        let state = Rc::clone(&ui);
        let change = Closure::<dyn FnMut(Event)>::new(move |_| {
            if state.running.get() {
                select_project(&state.project.borrow());
            }
            target_label();
        });
        select.add_event_listener_with_callback("change", change.as_ref().unchecked_ref())?;
        change.forget();
    }
    if let Some(cancel) = element("upload-cancel") {
        let state = Rc::clone(&ui);
        let handler = Closure::<dyn FnMut(Event)>::new(move |_| {
            state.cancelled.set(true);
            if let Some(abort) = state.abort.borrow().as_ref() {
                abort.abort();
            }
            label("upload-status", "Upload wird abgebrochen. Bereits gespeicherte Dateien bleiben erhalten.");
        });
        cancel.add_event_listener_with_callback("click", handler.as_ref().unchecked_ref())?;
        handler.forget();
    }
    if let Some(form) = element("project-create") {
        let state = Rc::clone(&ui);
        let handler = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            event.prevent_default();
            if state.running.get() {
                return;
            }
            let name = element("project-name").and_then(|e| e.dyn_into::<HtmlInputElement>().ok()).map(|e| e.value()).unwrap_or_default();
            if name.is_empty() {
                return;
            }
            busy(&state, true);
            hidden("upload-cancel", true);
            label("upload-status", "Projekt wird angelegt …");
            let state = Rc::clone(&state);
            spawn_local(async move {
                let answer = api("/workspace/projects", &json!({"name":name})).await;
                busy(&state, false);
                match answer {
                    Ok(_) => {
                        select_project(&name);
                        label("upload-status", &format!("Projekt „{name}“ angelegt. Jetzt Dateien auswählen oder hierher ziehen."));
                        if let Some(input) = element("project-name").and_then(|e| e.dyn_into::<HtmlInputElement>().ok()) {
                            input.set_value("");
                        }
                    }
                    Err(e) => label("upload-status", &e),
                }
            });
        });
        form.add_event_listener_with_callback("submit", handler.as_ref().unchecked_ref())?;
        handler.forget();
    }
    target_label();
    Ok(())
}
