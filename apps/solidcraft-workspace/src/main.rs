//! Browser-only plumbing for the server-rendered workspace. All model, grouping,
//! metadata and authentication decisions remain on the server.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast as _, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{Document, Element, Event, EventSource, HtmlDetailsElement, HtmlDialogElement, HtmlElement, MessageEvent, MouseEvent};

    fn document() -> Option<Document> {
        web_sys::window()?.document()
    }

    fn status(state: &str, label: &str) {
        let Some(document) = document() else { return };
        if let Some(bar) = document.get_element_by_id("workspace-status") {
            let _ = bar.set_attribute("data-state", state);
        }
        if let Some(text) = document.get_element_by_id("workspace-status-text")
            && text.text_content().as_deref() != Some(label)
        {
            text.set_text_content(Some(label));
        }
    }

    fn dialog() -> Option<HtmlDialogElement> {
        document()?.get_element_by_id("file-dialog")?.dyn_into().ok()
    }

    fn expired() {
        if let Some(dialog) = dialog() {
            dialog.close();
        }
        status("expired", "Anmeldung abgelaufen. Bitte erneut anmelden.");
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/workspace/login");
        }
    }

    fn apply(html: &str) {
        let Some(document) = document() else { return };
        let Some(live) = document.get_element_by_id("workspace-live") else { return };
        let mut projects = HashMap::new();
        if let Ok(nodes) = live.query_selector_all("[data-project]") {
            for index in 0..nodes.length().min(2000) {
                if let Some(node) = nodes.item(index)
                    && let Ok(details) = node.dyn_into::<HtmlDetailsElement>()
                    && let Some(id) = details.get_attribute("data-project")
                {
                    projects.insert(id, details.open());
                }
            }
        }
        let focused = document.active_element().filter(|element| live.contains(Some(element)));
        let focused_href = focused.as_ref().and_then(|element| element.get_attribute("href"));
        let focused_project =
            focused.and_then(|element| element.closest("[data-project]").ok().flatten()).and_then(|element| element.get_attribute("data-project"));
        live.set_inner_html(html);
        if let Ok(nodes) = live.query_selector_all("[data-project]") {
            for index in 0..nodes.length().min(2000) {
                if let Some(node) = nodes.item(index)
                    && let Ok(details) = node.dyn_into::<HtmlDetailsElement>()
                    && let Some(open) = details.get_attribute("data-project").and_then(|id| projects.get(&id))
                {
                    details.set_open(*open);
                    if focused_href.is_none()
                        && details.get_attribute("data-project") == focused_project
                        && let Ok(Some(summary)) = details.query_selector("summary")
                        && let Ok(summary) = summary.dyn_into::<HtmlElement>()
                    {
                        let _ = summary.focus();
                    }
                }
            }
        }
        if let Some(href) = focused_href
            && let Ok(nodes) = live.query_selector_all("a[href]")
        {
            for index in 0..nodes.length().min(6000) {
                if let Some(node) = nodes.item(index)
                    && let Ok(element) = node.dyn_into::<HtmlElement>()
                    && element.get_attribute("href").as_deref() == Some(href.as_str())
                {
                    let _ = element.focus();
                    break;
                }
            }
        }
    }

    async fn load_details(url: &str) -> Result<Option<String>, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("browser unavailable"))?;
        let response = JsFuture::from(window.fetch_with_str(url)).await?.dyn_into::<web_sys::Response>()?;
        if response.status() == 401 || response.redirected() && response.url().ends_with("/workspace/login") {
            expired();
            return Ok(None);
        }
        if !response.ok() {
            return Err(JsValue::from_str("file unavailable"));
        }
        let html = JsFuture::from(response.text()?).await?.as_string().ok_or_else(|| JsValue::from_str("invalid response"))?;
        if html.len() > 2 * 1024 * 1024 {
            return Err(JsValue::from_str("response too large"));
        }
        let parsed = web_sys::DomParser::new()?.parse_from_string(&html, web_sys::SupportedType::TextHtml)?;
        Ok(parsed.get_element_by_id("file-details").map(|element| element.outer_html()))
    }

    fn install_details(doc: &Document) -> Result<(), JsValue> {
        let request = Rc::new(Cell::new(0_u64));
        let active_request = request.clone();
        let listener = Closure::<dyn FnMut(MouseEvent)>::new(move |event: MouseEvent| {
            if event.button() != 0 || event.ctrl_key() || event.meta_key() || event.shift_key() || event.alt_key() {
                return;
            }
            let Some(element) = event.target().and_then(|target| target.dyn_into::<Element>().ok()) else { return };
            let Some(link) = element.closest("a[data-file-details]").ok().flatten() else { return };
            let Some(url) = link.get_attribute("data-file-details").filter(|url| url.starts_with("/workspace/details/")) else { return };
            let Some(dialog) = dialog() else { return };
            // Preserve a normal accessible navigation when the browser cannot
            // display the native dialog, rather than swallowing the link.
            if dialog.show_modal().is_err() {
                return;
            }
            event.prevent_default();
            let generation = active_request.get().wrapping_add(1);
            active_request.set(generation);
            let current = active_request.clone();
            if let Some(content) = document().and_then(|doc| doc.get_element_by_id("file-dialog-content")) {
                content.set_text_content(Some("Dateidetails werden geladen …"));
            }
            spawn_local(async move {
                let result = load_details(&url).await;
                if current.get() != generation {
                    return;
                }
                let Some(content) = document().and_then(|doc| doc.get_element_by_id("file-dialog-content")) else { return };
                match result {
                    Ok(Some(html)) => content.set_inner_html(&html),
                    Ok(None) => content.set_text_content(Some("Dateidetails nicht verfügbar. Lade den Arbeitsbereich neu.")),
                    Err(_) => {
                        content.set_text_content(Some("Dateidetails konnten nicht geladen werden. Prüfe die Verbindung und versuche es erneut."))
                    }
                }
            });
        });
        doc.add_event_listener_with_callback("click", listener.as_ref().unchecked_ref())?;
        listener.forget();
        if let Some(close) = doc.get_element_by_id("file-dialog-close") {
            let listener = Closure::<dyn FnMut(Event)>::new(move |_| {
                request.set(request.get().wrapping_add(1));
                if let Some(dialog) = dialog() {
                    dialog.close();
                }
            });
            close.add_event_listener_with_callback("click", listener.as_ref().unchecked_ref())?;
            listener.forget();
        }
        Ok(())
    }

    pub fn start() -> Result<(), JsValue> {
        let document = document().ok_or_else(|| JsValue::from_str("document unavailable"))?;
        if document.get_element_by_id("workspace-live").is_none() {
            return Ok(());
        }
        install_details(&document)?;
        let query = web_sys::window().and_then(|window| window.location().search().ok()).unwrap_or_default();
        connect(query, Rc::new(Cell::new(0)))
    }

    // Native EventSource retries while CONNECTING. A server restart can instead
    // terminate the stream as CLOSED; recover that state separately, bounded to
    // three consecutive replacements, without reinstalling any page listeners.
    fn connect(query: String, terminal_retries: Rc<Cell<u8>>) -> Result<(), JsValue> {
        let source = EventSource::new(&format!("/workspace/events{query}"))?;
        let opened_retries = terminal_retries.clone();
        let open = Closure::<dyn FnMut(Event)>::new(move |_| {
            opened_retries.set(0);
            status("connected", "Live verbunden · Änderungen erscheinen automatisch");
        });
        source.set_onopen(Some(open.as_ref().unchecked_ref()));
        open.forget();
        let keep_alive = source.clone();
        let checked_terminal = Rc::new(Cell::new(false));
        let error = Closure::<dyn FnMut(Event)>::new(move |_| {
            if keep_alive.ready_state() != EventSource::CLOSED {
                status("reconnecting", "Verbindung unterbrochen · wird automatisch wiederhergestellt");
                return;
            }
            if checked_terminal.replace(true) {
                return;
            }
            status("reconnecting", "Live-Verbindung beendet · Anmeldung wird geprüft");
            let query = query.clone();
            let retries = terminal_retries.clone();
            // Probe authentication once per terminal connection. A valid
            // remembered login can survive the restart that closed its stream.
            spawn_local(async move {
                let Some(window) = web_sys::window() else { return };
                let response =
                    JsFuture::from(window.fetch_with_str("/workspace/api")).await.and_then(|response| response.dyn_into::<web_sys::Response>());
                match response {
                    Ok(response) if response.status() == 401 => expired(),
                    response
                        if retries.get() < 3
                            && match &response {
                                Ok(response) => matches!(response.status(), 200 | 429 | 502 | 503 | 504),
                                Err(_) => true,
                            } =>
                    {
                        retries.set(retries.get().saturating_add(1));
                        status("reconnecting", "Verbindung unterbrochen · wird automatisch wiederhergestellt");
                        let retry = Closure::once_into_js(move || {
                            if connect(query, retries).is_err() {
                                status("unavailable", "Live-Verbindung nicht verfügbar · bitte den Arbeitsbereich neu laden");
                            }
                        });
                        if window.set_timeout_with_callback_and_timeout_and_arguments_0(retry.unchecked_ref(), 3000).is_err() {
                            status("unavailable", "Live-Verbindung nicht verfügbar · bitte den Arbeitsbereich neu laden");
                        }
                    }
                    _ => status("unavailable", "Live-Verbindung beendet · bitte den Arbeitsbereich neu laden"),
                }
            });
        });
        source.set_onerror(Some(error.as_ref().unchecked_ref()));
        error.forget();
        let update = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(data) = event.data().as_string().filter(|data| data.len() <= 8 * 1024 * 1024) else { return };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else { return };
            if let Some(html) = value.get("html").and_then(serde_json::Value::as_str) {
                apply(html);
                status("connected", "Live verbunden · Änderungen erscheinen automatisch");
            }
        });
        source.add_event_listener_with_callback("workspace", update.as_ref().unchecked_ref())?;
        update.forget();
        let invalid = source.clone();
        let end = Closure::<dyn FnMut(Event)>::new(move |_| {
            invalid.close();
            expired();
        });
        source.add_event_listener_with_callback("expired", end.as_ref().unchecked_ref())?;
        end.forget();
        let unavailable = Closure::<dyn FnMut(Event)>::new(move |_| {
            status("unavailable", "Aktualisierung vorübergehend nicht verfügbar · Verbindung bleibt bestehen")
        });
        source.add_event_listener_with_callback("unavailable", unavailable.as_ref().unchecked_ref())?;
        unavailable.forget();
        Ok(())
    }
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    if browser::start().is_err() {
        // All links, filters and the manual refresh remain usable if the tiny
        // optional browser client cannot start.
        if let Some(document) = web_sys::window().and_then(|window| window.document())
            && let Some(status) = document.get_element_by_id("workspace-status-text")
        {
            status.set_text_content(Some("Live-Verbindung nicht verfügbar · bitte manuell aktualisieren"));
        }
    }
}
