//! Workspace-only locale selection and rendering. The preference never leaves the browser.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Document, Element, Event, HtmlSelectElement, MouseEvent};

type Catalog = BTreeMap<String, BTreeMap<String, String>>;
const MESSAGES: &str = include_str!("../../../assets/locales/pending/workspace/messages.json");
const STORAGE_KEY: &str = "solidcraft.workspace.language";
const DEFAULT: &str = "en-US";

thread_local! {
    static LANGUAGE: Cell<&'static str> = const { Cell::new(DEFAULT) };
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(MESSAGES).unwrap_or_default())
}

fn supported(value: &str) -> Option<&'static str> {
    match value {
        "de" => Some("de"),
        "en-US" => Some(DEFAULT),
        "en-GB" => Some("en-GB"),
        _ => None,
    }
}

fn browser_match(value: &str) -> Option<&'static str> {
    let tag = value.to_ascii_lowercase();
    if tag == "en-gb" || tag.starts_with("en-gb-") {
        return Some("en-GB");
    }
    match tag.split('-').next() {
        Some("de") => Some("de"),
        Some("en") => Some(DEFAULT),
        _ => None,
    }
}

fn key(source: &str) -> Option<&'static str> {
    if source
        == "project name must be one visible folder name (1–96 UTF-8 bytes), without surrounding spaces, trailing dots, reserved names or path separators"
    {
        return Some("error_invalid_project_name");
    }
    catalog().get("de")?.iter().find_map(|(key, value)| (value == source).then_some(key.as_str()))
}

fn template(id: &str) -> Option<&'static str> {
    let language = LANGUAGE.get();
    catalog().get(language).and_then(|messages| messages.get(id)).or_else(|| catalog().get(DEFAULT)?.get(id)).map(String::as_str)
}

fn interpolate(template: &str, args: &BTreeMap<&str, &str>) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some((prefix, suffix)) = rest.split_once('{') {
        result.push_str(prefix);
        let Some((key, remaining)) = suffix.split_once('}') else {
            result.push('{');
            result.push_str(suffix);
            return result;
        };
        if let Some(value) = args.get(key) {
            result.push_str(value);
        }
        rest = remaining;
    }
    result.push_str(rest);
    result
}

pub(crate) fn format(id: &str, args: &[(&str, &str)]) -> String {
    template(id).map_or_else(String::new, |pattern| interpolate(pattern, &args.iter().copied().collect()))
}

pub(crate) fn translate(source: &str) -> &str {
    key(source).and_then(template).unwrap_or(source)
}

fn render_element(element: &Element) {
    let args = element.get_attribute("data-ws-args");
    let args = args.as_deref().and_then(|args| serde_json::from_str::<BTreeMap<String, String>>(args).ok());
    let args = args.as_ref().map(|args| args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<BTreeMap<_, _>>());
    if let Some(id) = element.get_attribute("data-ws-text")
        && let Some(text) = template(&id)
    {
        let text = args.as_ref().map_or_else(|| text.to_owned(), |args| interpolate(text, args));
        if element.text_content().as_deref() != Some(text.as_str()) {
            element.set_text_content(Some(&text));
        }
    }
    for (key, attribute) in
        [("data-ws-aria-label", "aria-label"), ("data-ws-alt", "alt"), ("data-ws-placeholder", "placeholder"), ("data-ws-title", "title")]
    {
        if let Some(key) = element.get_attribute(key)
            && let Some(text) = template(&key)
        {
            let value = args.as_ref().map_or_else(|| text.to_owned(), |args| interpolate(text, args));
            let _ = element.set_attribute(attribute, &value);
        }
    }
}

pub(crate) fn apply(root: &Element) {
    const SELECTOR: &str = "[data-ws-text],[data-ws-aria-label],[data-ws-alt],[data-ws-placeholder],[data-ws-title]";
    render_element(root);
    if let Ok(elements) = root.query_selector_all(SELECTOR) {
        for index in 0..elements.length() {
            if let Some(element) = elements.item(index).and_then(|node| node.dyn_into::<Element>().ok()) {
                render_element(&element);
            }
        }
    }
}

pub(crate) fn message(element: &Element, source: &str) {
    if let Some(id) = key(source) {
        set(element, id, &[]);
    } else {
        let _ = element.remove_attribute("data-ws-text");
        let _ = element.remove_attribute("data-ws-args");
        element.set_text_content(Some(source));
    }
}

pub(crate) fn set(element: &Element, id: &str, args: &[(&str, &str)]) {
    let _ = element.set_attribute("data-ws-text", id);
    if args.is_empty() {
        let _ = element.remove_attribute("data-ws-args");
    } else if let Ok(value) = serde_json::to_string(&args.iter().copied().collect::<BTreeMap<_, _>>()) {
        let _ = element.set_attribute("data-ws-args", &value);
    }
    render_element(element);
}

pub(crate) fn clear(element: &Element) {
    let _ = element.remove_attribute("data-ws-text");
    let _ = element.remove_attribute("data-ws-args");
    element.set_text_content(None);
}

fn apply_document(doc: &Document, code: &'static str) {
    LANGUAGE.set(code);
    if let Some(root) = doc.document_element() {
        let _ = root.set_attribute("lang", code);
        apply(&root);
    }
    if let Ok(Some(title)) = doc.query_selector("title[data-ws-page]")
        && let Some(label) = title.get_attribute("data-ws-page").as_deref().and_then(template)
    {
        doc.set_title(&format!("SolidCraft · {label}"));
    }
}

pub(crate) fn install(doc: &Document) -> Result<(), JsValue> {
    let Some(window) = web_sys::window() else { return Ok(()) };
    let storage = window.local_storage().ok().flatten();
    let navigator = window.navigator();
    let language = storage
        .as_ref()
        .and_then(|storage| storage.get_item(STORAGE_KEY).ok().flatten())
        .as_deref()
        .and_then(supported)
        .or_else(|| navigator.languages().iter().find_map(|value| value.as_string().as_deref().and_then(browser_match)))
        .or_else(|| navigator.language().as_deref().and_then(browser_match))
        .unwrap_or(DEFAULT);
    apply_document(doc, language);

    if let Some(select) = doc.get_element_by_id("workspace-language").and_then(|node| node.dyn_into::<HtmlSelectElement>().ok()) {
        select.set_value(language);
        if let Some(label) = select.parent_element() {
            let _ = label.remove_attribute("hidden");
        }
        let field = select.clone();
        let document = doc.clone();
        let change = Closure::<dyn FnMut(Event)>::new(move |_| {
            if let Some(language) = supported(&field.value()) {
                if let Some(storage) = storage.as_ref() {
                    let _ = storage.set_item(STORAGE_KEY, language);
                }
                apply_document(&document, language);
                crate::uploads::target_label();
            }
        });
        select.add_event_listener_with_callback("change", change.as_ref().unchecked_ref())?;
        change.forget();
    }

    if let Some(link) = doc.get_element_by_id("open-browser-cad") {
        let click = Closure::<dyn FnMut(MouseEvent)>::new(move |event: MouseEvent| {
            if event.button() == 0
                && !event.ctrl_key()
                && !event.meta_key()
                && !event.shift_key()
                && !event.alt_key()
                && window.open_with_url_and_target_and_features("/", "_blank", "popup,noopener").is_ok()
            {
                event.prevent_default();
            }
        });
        link.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
        click.forget();
    }
    Ok(())
}
