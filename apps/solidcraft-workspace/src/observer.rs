//! Read-only observer: one stream, one displayed image and one decoding buffer.
//! No model commands, persisted credentials or unbounded frame queue.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use serde_json::Value;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Document, Element, Event, EventSource, HtmlDialogElement, HtmlElement, HtmlImageElement, MessageEvent};

thread_local! {
    static CLIENT: RefCell<Option<Rc<RefCell<Client>>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Client {
    connection: Option<Connection>,
    generation: u64,
    handle: String,
    return_handle: String,
    retries: u8,
    reconnecting: bool,
    front: usize,
    pending: Option<Frame>,
    loaded_revision: Option<u64>,
    desired_status: String,
}

struct Frame {
    generation: u64,
    index: usize,
    revision: u64,
    url: String,
    metadata: String,
}

type Listener = (&'static str, Closure<dyn FnMut(Event)>);

struct Connection {
    source: EventSource,
    listeners: Vec<Listener>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.source.close();
        for (name, listener) in &self.listeners {
            let _ = self.source.remove_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
    }
}

fn retire(connection: Option<Connection>) {
    if let Some(connection) = connection {
        connection.source.close();
        // A terminal event can close its own connection. Keep its active Wasm
        // closure alive until that callback has returned, then detach and drop.
        spawn_local(async move { drop(connection) });
    }
}

fn document() -> Option<Document> {
    web_sys::window()?.document()
}

fn element(id: &str) -> Option<Element> {
    document()?.get_element_by_id(id)
}

fn dialog() -> Option<HtmlDialogElement> {
    element("observer-dialog")?.dyn_into().ok()
}

fn label(state: &str, text: &str) {
    for id in ["observer-dialog", "observer-status"] {
        if let Some(element) = element(id) {
            let _ = element.set_attribute("data-state", state);
        }
    }
    if let Some(element) = element("observer-status-text") {
        element.set_text_content(Some(text));
    }
}

fn clear_images() {
    for id in ["observer-image", "observer-next"] {
        if let Some(image) = element(id) {
            let _ = image.set_attribute("hidden", "");
            let _ = image.remove_attribute("src");
        }
    }
    if let Some(placeholder) = element("observer-placeholder") {
        let _ = placeholder.remove_attribute("hidden");
    }
    for id in ["observer-meta", "observer-warning"] {
        if let Some(meta) = element(id) {
            meta.set_text_content(None);
        }
    }
}

fn close(client: &Rc<RefCell<Client>>) {
    let connection = {
        let mut state = client.borrow_mut();
        state.desired_status.clear();
        state.generation = state.generation.wrapping_add(1);
        state.handle.clear();
        state.pending = None;
        state.loaded_revision = None;
        state.reconnecting = false;
        state.connection.take()
    };
    retire(connection);
    clear_images();
}

pub(crate) fn stop() {
    CLIENT.with(|client| {
        if let Some(client) = client.borrow().as_ref() {
            close(client);
        }
    });
    if let Some(dialog) = dialog() {
        dialog.close();
    }
}

fn ended(client: &Rc<RefCell<Client>>) {
    close(client);
    label("ended", "Sitzung beendet · diese Live-Vorschau ist geschlossen.");
    if let Some(placeholder) = element("observer-placeholder") {
        placeholder.set_text_content(Some("Die Sitzung ist nicht mehr verfügbar. Es wird kein veraltetes Modell als live angezeigt."));
    }
}

fn image_id(index: usize) -> &'static str {
    if index == 0 { "observer-image" } else { "observer-next" }
}

fn update(client: &Rc<RefCell<Client>>, data: &str) {
    if data.len() > 4096 {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else { return };
    let status = value.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "ended" {
        ended(client);
        return;
    }
    let text = match status {
        "waiting" => "Wartet auf einen darstellbaren Modellstand …",
        "rendering" => "Vorschaubild wird berechnet · der vorherige Stand bleibt sichtbar.",
        "busy" => "Berechnung läuft · Bild zeigt den letzten abgeschlossenen Stand.",
        "ready" => "Live verbunden · aktueller abgeschlossener Modellstand",
        "unavailable" => "Vorschau vorübergehend nicht verfügbar · das letzte Bild kann veraltet sein.",
        _ => return,
    };
    label(status, text);
    if status == "waiting" {
        {
            let mut state = client.borrow_mut();
            state.pending = None;
            state.loaded_revision = None;
        }
        clear_images();
    }
    if let Some(warning) = element("observer-warning") {
        let message = if value.get("renderer").and_then(Value::as_str) == Some("cpu") {
            Some("CPU-Vorschau: Analysefarben, Hervorhebungen und Legenden können fehlen oder von der Desktop-Ansicht abweichen.")
        } else {
            value.get("warning").and_then(Value::as_str).filter(|text| text.len() <= 1024)
        };
        warning.set_text_content(message);
    }
    let mut state = client.borrow_mut();
    state.desired_status = status.into();
    let Some(revision) = value.get("revision").and_then(Value::as_u64) else { return };
    if state.loaded_revision.is_some_and(|loaded| loaded >= revision) {
        return;
    }
    if state.pending.as_ref().is_some_and(|pending| pending.revision >= revision) {
        if status == "ready" {
            label("rendering", "Neuer abgeschlossener Stand · Bild wird geladen …");
        }
        return;
    }
    // Reconstruct a same-origin URL from the handle and bounded integer. Never
    // accept arbitrary payload URLs, MCP session IDs, data URLs or credentials.
    let url = format!("/workspace/observe/{}/image?revision={revision}", state.handle);
    if value.get("image_url").and_then(Value::as_str) != Some(url.as_str()) {
        return;
    }
    let renderer = value.get("renderer").and_then(Value::as_str).filter(|value| value.len() <= 80).unwrap_or("CPU");
    let captured = value.get("captured_at").and_then(Value::as_str).filter(|value| value.len() <= 80).unwrap_or("Zeit unbekannt");
    let index = 1 - state.front;
    state.pending = Some(Frame {
        generation: state.generation,
        index,
        revision,
        url: url.clone(),
        metadata: format!("Bildrevision {revision} · {captured} · Renderer: {renderer}"),
    });
    if status == "ready" {
        label("rendering", "Neuer abgeschlossener Stand · Bild wird geladen …");
    }
    if let Some(image) = element(image_id(index)) {
        // Only a decoded matching back buffer can replace image and metadata.
        let _ = image.set_attribute("src", &url);
    }
}

fn callback(connection: &mut Connection, name: &'static str, action: impl FnMut(Event) + 'static) -> Result<(), JsValue> {
    let listener = Closure::<dyn FnMut(Event)>::new(action);
    connection.source.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref())?;
    connection.listeners.push((name, listener));
    Ok(())
}

fn connect(client: &Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let (handle, generation) = {
        let mut state = client.borrow_mut();
        // Invalidate queued callbacks from the replaced stream as well as any
        // image decode started under it. A restored stream will announce its
        // current cached frame again; the displayed front buffer remains valid.
        state.generation = state.generation.wrapping_add(1);
        state.pending = None;
        (state.handle.clone(), state.generation)
    };
    let mut connection = Connection { source: EventSource::new(&format!("/workspace/observe/{handle}/events"))?, listeners: Vec::new() };
    let weak = Rc::downgrade(client);
    callback(&mut connection, "observer", move |event| {
        let Some(client) = current(&weak, generation) else { return };
        let Some(event) = event.dyn_ref::<MessageEvent>() else { return };
        if let Some(data) = event.data().as_string() {
            update(&client, &data);
        }
    })?;
    let weak = Rc::downgrade(client);
    callback(&mut connection, "open", move |_| {
        if let Some(client) = current(&weak, generation) {
            let mut state = client.borrow_mut();
            state.retries = 0;
            state.reconnecting = false;
        }
    })?;
    let weak = Rc::downgrade(client);
    callback(&mut connection, "expired", move |_| {
        if current(&weak, generation).is_some() {
            crate::browser::expired();
        }
    })?;
    let weak = Rc::downgrade(client);
    callback(&mut connection, "error", move |_| {
        let Some(client) = current(&weak, generation) else { return };
        client.borrow_mut().desired_status = "unavailable".into();
        label("unavailable", "Verbindung unterbrochen · das letzte Bild ist möglicherweise veraltet.");
        let terminal = client.borrow().connection.as_ref().is_some_and(|connection| connection.source.ready_state() == EventSource::CLOSED);
        if terminal && !client.borrow().reconnecting {
            client.borrow_mut().reconnecting = true;
            check_reconnect(Rc::downgrade(&client), generation);
        }
    })?;
    let previous = client.borrow_mut().connection.replace(connection);
    retire(previous);
    Ok(())
}

fn current(weak: &Weak<RefCell<Client>>, generation: u64) -> Option<Rc<RefCell<Client>>> {
    let client = weak.upgrade()?;
    let valid = client.borrow().generation == generation && !client.borrow().handle.is_empty();
    valid.then_some(client)
}

fn check_reconnect(weak: Weak<RefCell<Client>>, generation: u64) {
    spawn_local(async move {
        let Some(window) = web_sys::window() else { return };
        // Authentication only: never ask the worker to render via this probe.
        let response = JsFuture::from(window.fetch_with_str("/workspace/api")).await.and_then(|value| value.dyn_into::<web_sys::Response>());
        let Some(client) = current(&weak, generation) else { return };
        if response.as_ref().is_ok_and(|response| response.status() == 401) {
            crate::browser::expired();
            return;
        }
        if client.borrow().retries >= 3 {
            label("unavailable", "Live-Vorschau nicht erreichbar · schließen und erneut öffnen.");
            return;
        }
        {
            let mut state = client.borrow_mut();
            state.retries = state.retries.saturating_add(1);
        }
        let retry = Closure::once_into_js(move || {
            if let Some(client) = current(&weak, generation)
                && connect(&client).is_err()
            {
                client.borrow_mut().reconnecting = false;
                label("unavailable", "Live-Vorschau nicht erreichbar · schließen und erneut öffnen.");
            }
        });
        if window.set_timeout_with_callback_and_timeout_and_arguments_0(retry.unchecked_ref(), 3000).is_err() {
            client.borrow_mut().reconnecting = false;
            label("unavailable", "Live-Vorschau nicht erreichbar · schließen und erneut öffnen.");
        }
    });
}

fn listen(target: &web_sys::EventTarget, name: &str, action: impl FnMut(Event) + 'static) -> Result<(), JsValue> {
    let listener = Closure::<dyn FnMut(Event)>::new(action);
    target.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref())?;
    // Exactly one set of page listeners; each captures only a weak client.
    listener.forget();
    Ok(())
}

fn restore_focus(handle: &str) {
    let Some(doc) = document() else { return };
    let selector = format!("button[data-observe=\"{handle}\"]");
    let target = doc.query_selector(&selector).ok().flatten().or_else(|| doc.query_selector("#workspace-status a").ok().flatten());
    if let Some(target) = target.and_then(|element| element.dyn_into::<HtmlElement>().ok()) {
        let _ = target.focus();
    }
}

pub(crate) fn install(doc: &Document) -> Result<(), JsValue> {
    let client = Rc::new(RefCell::new(Client::default()));
    CLIENT.with(|slot| *slot.borrow_mut() = Some(client.clone()));
    let weak = Rc::downgrade(&client);
    listen(doc.as_ref(), "click", move |event| {
        let Some(target) = event.target().and_then(|target| target.dyn_into::<Element>().ok()) else { return };
        let Some(button) = target.closest("button[data-observe]").ok().flatten() else { return };
        let Some(handle) = button.get_attribute("data-observe").filter(|handle| {
            handle.len() == 36
                && handle.bytes().enumerate().all(
                    |(index, byte)| {
                        if matches!(index, 8 | 13 | 18 | 23) { byte == b'-' } else { byte.is_ascii_hexdigit() }
                    },
                )
        }) else {
            return;
        };
        let (Some(client), Some(dialog)) = (weak.upgrade(), dialog()) else { return };
        close(&client);
        if dialog.show_modal().is_err() {
            return;
        }
        event.prevent_default();
        {
            let mut state = client.borrow_mut();
            state.handle = handle.clone();
            state.return_handle = handle;
            state.retries = 0;
        }
        if let Some(title) = element("observer-title") {
            let session = button
                .closest(".session")
                .ok()
                .flatten()
                .and_then(|section| section.query_selector(".sessionname").ok().flatten())
                .and_then(|name| name.text_content())
                .filter(|name| name.len() <= 160);
            title.set_text_content(Some(
                &session.map_or_else(|| "Live-Vorschau der MCP-Sitzung".into(), |session| format!("Live-Vorschau · {session}")),
            ));
        }
        if let Some(placeholder) = element("observer-placeholder") {
            placeholder.set_text_content(Some("Das erste Modellbild wird vorbereitet."));
        }
        label("waiting", "Vorschau wird verbunden …");
        if connect(&client).is_err() {
            label("unavailable", "Die Live-Vorschau konnte nicht verbunden werden.");
        }
    })?;
    if let Some(dialog) = dialog() {
        let weak = Rc::downgrade(&client);
        listen(dialog.as_ref(), "close", move |_| {
            if let Some(client) = weak.upgrade() {
                let handle = client.borrow().return_handle.clone();
                close(&client);
                restore_focus(&handle);
            }
        })?;
    }
    if let Some(close_button) = element("observer-close") {
        listen(close_button.as_ref(), "click", |_| {
            if let Some(dialog) = dialog() {
                dialog.close();
            }
        })?;
    }
    let weak = Rc::downgrade(&client);
    listen(doc.as_ref(), "submit", move |event| {
        let is_logout = event
            .target()
            .and_then(|target| target.dyn_into::<Element>().ok())
            .is_some_and(|target| target.get_attribute("action").as_deref() == Some("/workspace/logout"));
        if is_logout && let Some(client) = weak.upgrade() {
            close(&client);
        }
    })?;
    if let Some(window) = web_sys::window() {
        listen(window.as_ref(), "pagehide", |_| stop())?;
    }
    for index in 0..2 {
        let Some(image) = element(image_id(index)) else { continue };
        let weak = Rc::downgrade(&client);
        listen(image.as_ref(), "load", move |_| {
            let Some(client) = weak.upgrade() else { return };
            let mut state = client.borrow_mut();
            if !matching_frame(&state, index, false) {
                return;
            }
            let Some(frame) = state.pending.take() else { return };
            if let Some(front) = element(image_id(state.front)) {
                let _ = front.set_attribute("hidden", "");
                let _ = front.remove_attribute("src");
            }
            if let Some(back) = element(image_id(index)) {
                let _ = back.remove_attribute("hidden");
            }
            if let Some(placeholder) = element("observer-placeholder") {
                let _ = placeholder.set_attribute("hidden", "");
            }
            if let Some(meta) = element("observer-meta") {
                meta.set_text_content(Some(&frame.metadata));
            }
            state.front = index;
            state.loaded_revision = Some(frame.revision);
            if state.desired_status == "ready" {
                label("ready", "Live verbunden · aktueller abgeschlossener Modellstand");
            }
        })?;
        let weak = Rc::downgrade(&client);
        listen(image.as_ref(), "error", move |_| {
            let Some(client) = weak.upgrade() else { return };
            let mut state = client.borrow_mut();
            if matching_frame(&state, index, true) {
                state.pending = None;
                state.desired_status = "unavailable".into();
                label("unavailable", "Bild nicht verfügbar · der nächste Modellstand wird automatisch übernommen.");
            }
        })?;
    }
    Ok(())
}

fn matching_frame(state: &Client, index: usize, failed: bool) -> bool {
    let Some(frame) = state.pending.as_ref() else { return false };
    if state.handle.is_empty() || frame.generation != state.generation || frame.index != index || index == state.front {
        return false;
    }
    let Some(image) = element(image_id(index)).and_then(|image| image.dyn_into::<HtmlImageElement>().ok()) else { return false };
    if image.get_attribute("src").as_deref() != Some(frame.url.as_str()) || !image.complete() {
        return false;
    }
    if failed {
        return image.natural_width() == 0;
    }
    let Some(origin) = web_sys::window().and_then(|window| window.location().origin().ok()) else { return false };
    image.natural_width() > 0 && image.natural_height() > 0 && image.current_src() == format!("{origin}{}", frame.url)
}
