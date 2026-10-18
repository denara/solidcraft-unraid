//! One-use workspace handoff. The fragment holds no MCP credential and is removed
//! before fetching a fixed same-origin, cookie-authenticated import endpoint.

pub const PREFIX: &str = "#workspace-import=";
pub const MAX_BYTES: usize = solidcraft_engine::io::MAX_DESIGN_BYTES;

/// The original doc.open UI path already preserves the active design in a new
/// tab. Its prepare_open reports full capacity without stopping doc.open, so
/// reject that case before calling it on an externally queued browser copy.
pub fn open_copy(app: &mut solidcraft_ui_egui::SolveApp, path: &str) -> bool {
    if !solidcraft_ui_egui::documents::active_is_blank(app) && app.docs.count() >= solidcraft_ui_egui::documents::MAX_OPEN {
        app.set_status("Die Modellkopie wurde nicht geöffnet: Zu viele Modelle sind bereits offen. Schließe ein Modell und öffne die Datei erneut im Arbeitsbereich.", true);
        return false;
    }
    app.open_path(path);
    app.session.path.as_deref() == Some(path)
}

pub fn key(fragment: &str) -> Result<Option<&str>, &'static str> {
    let Some(value) = fragment.strip_prefix(PREFIX) else { return Ok(None) };
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("Der Link zum gespeicherten Modell ist ungültig. Öffne es erneut im Arbeitsbereich.");
    }
    Ok(Some(value))
}

pub fn filename(encoded: &str) -> Result<String, &'static str> {
    if encoded.len() > 3072 {
        return Err("Der Dateiname der Modellkopie ist ungültig.");
    }
    let mut decoded = Vec::new();
    let mut bytes = encoded.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let first = char::from(bytes.next().ok_or("Ungültiger Dateiname.")?).to_digit(16).ok_or("Ungültiger Dateiname.")?;
            let second = char::from(bytes.next().ok_or("Ungültiger Dateiname.")?).to_digit(16).ok_or("Ungültiger Dateiname.")?;
            decoded.push((first * 16 + second) as u8);
        } else {
            decoded.push(byte);
        }
    }
    let name = String::from_utf8(decoded).map_err(|_| "Ungültiger Dateiname.")?;
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', ':']) || name.chars().any(char::is_control) {
        return Err("Der Dateiname der Modellkopie ist ungültig.");
    }
    let Some((stem, extension)) = name.rsplit_once('.') else { return Err("Die Datei ist kein SolidCraft-Modell.") };
    // Existing explicit native-extension compatibility is retained on open.
    if stem.is_empty() || !matches!(extension.to_ascii_lowercase().as_str(), "solidcraft" | "solvecraft") {
        return Err("Die Datei ist kein SolidCraft-Modell.");
    }
    if name.len() <= 200 {
        return Ok(name);
    }
    let mut short = String::new();
    for ch in stem.chars() {
        if short.len() + ch.len_utf8() > 180 {
            break;
        }
        short.push(ch);
    }
    Ok(format!("{short}.solidcraft"))
}

pub fn payload(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("Die Modellkopie überschreitet die Größenbegrenzung von 256 MiB.");
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.iter().copied().find(|byte| !byte.is_ascii_whitespace()) == Some(b'{') {
        return Ok(());
    }
    Err("Die Datei enthält kein lesbares SolidCraft-Modell.")
}

/// Each handoff gets its own browser filename even when two tabs finish fetching
/// at the same instant with independently loaded OPFS inventories.
pub fn copy_name(name: &str, entropy: [u8; 16]) -> String {
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or("Modell");
    let random: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
    let suffix = format!(" (Browserkopie-{random}).solidcraft");
    let mut short = String::new();
    for ch in stem.chars() {
        if short.len() + ch.len_utf8() + suffix.len() > 200 {
            break;
        }
        short.push(ch);
    }
    format!("{short}{suffix}")
}

#[cfg(target_arch = "wasm32")]
pub struct Imported {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
pub type Slot = std::rc::Rc<std::cell::RefCell<Option<Result<Imported, String>>>>;

#[cfg(target_arch = "wasm32")]
pub fn start(ctx: &egui::Context) -> (Slot, bool) {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::JsFuture;

    let slot: Slot = Default::default();
    let Some(window) = web_sys::window() else { return (slot, false) };
    let fragment = window.location().hash().unwrap_or_default();
    let request = match key(&fragment) {
        Ok(None) => return (slot, false),
        Ok(Some(key)) => Ok(key.to_owned()),
        Err(error) => Err(error.to_owned()),
    };
    // Drop only this handoff fragment, preserving the path and every query flag.
    // A reload must never consume the same one-use request again.
    let clean = window.location().pathname().and_then(|path| window.location().search().map(|query| format!("{path}{query}")));
    let consumed = clean.and_then(|clean| window.history()?.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&clean)));
    if consumed.is_err() {
        *slot.borrow_mut() = Some(Err("Der Modelllink konnte nicht sicher übernommen werden. Öffne ihn erneut im Arbeitsbereich.".into()));
        return (slot, true);
    }
    let key = match request {
        Ok(key) => key,
        Err(error) => {
            *slot.borrow_mut() = Some(Err(error));
            return (slot, true);
        }
    };
    let result = slot.clone();
    let ctx = ctx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let fetched = async {
            let url = format!("/workspace/import/{key}");
            // The browser attaches its HttpOnly cookie only to /workspace. Neither
            // that cookie nor the MCP token is read by application code.
            let response =
                JsFuture::from(window.fetch_with_str(&url)).await.map_err(|_| "Die Modellkopie konnte nicht geladen werden.".to_owned())?;
            let response: web_sys::Response = response.dyn_into().map_err(|_| "Ungültige Antwort für die Modellkopie.".to_owned())?;
            if !response.ok() || response.redirected() {
                return Err(match response.status() {
                    401 => "Bitte melde dich im Arbeitsbereich an und öffne das Modell erneut.",
                    404 | 410 => "Dieser Modelllink ist abgelaufen oder wurde bereits verwendet. Öffne das Modell erneut im Arbeitsbereich.",
                    409 | 412 => "Die gespeicherte Datei wurde inzwischen geändert. Öffne das Modell erneut im Arbeitsbereich.",
                    _ => "Die Modellkopie konnte nicht geladen werden. Öffne sie erneut im Arbeitsbereich.",
                }
                .to_owned());
            }
            let size = response
                .headers()
                .get("content-length")
                .ok()
                .flatten()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|size| *size <= MAX_BYTES)
                .ok_or("Die Modellkopie hat keine gültige Dateigröße.".to_owned())?;
            let encoded = response.headers().get("x-solidcraft-filename").ok().flatten().ok_or("Der Dateiname der Modellkopie fehlt.".to_owned())?;
            let name = filename(&encoded).map_err(str::to_owned)?;
            let buffer = response.array_buffer().map_err(|_| "Die Modellkopie konnte nicht gelesen werden.".to_owned())?;
            let buffer = JsFuture::from(buffer).await.map_err(|_| "Die Modellkopie konnte nicht gelesen werden.".to_owned())?;
            let array = js_sys::Uint8Array::new(&buffer);
            if array.length() as usize != size || array.length() as usize > MAX_BYTES {
                return Err("Die Größe der Modellkopie stimmt nicht mit der Antwort überein.".to_owned());
            }
            let bytes = array.to_vec();
            payload(&bytes).map_err(str::to_owned)?;
            let mut entropy = [0u8; 16];
            window
                .crypto()
                .and_then(|crypto| crypto.get_random_values_with_u8_array(&mut entropy))
                .map_err(|_| "Eine sichere Browserkopie konnte nicht angelegt werden. Öffne das Modell erneut im Arbeitsbereich.".to_owned())?;
            let name = copy_name(&name, entropy);
            Ok(Imported { name, bytes })
        }
        .await;
        *result.borrow_mut() = Some(fetched);
        ctx.request_repaint();
    });
    (slot, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_accepts_only_one_fixed_shape_opaque_handoff() {
        let valid = format!("{PREFIX}{}", "a1".repeat(32));
        assert_eq!(key(&valid), Ok(valid.strip_prefix(PREFIX)));
        for fragment in ["", "#part", "#workspace-import", "#other=abc"] {
            assert_eq!(key(fragment), Ok(None));
        }
        for value in ["", "../private", "https://other.example", &"a".repeat(63), &"A".repeat(64), &format!("{}&next=evil", "a".repeat(64))] {
            assert!(key(&format!("{PREFIX}{value}")).is_err());
        }
    }

    #[test]
    fn filenames_are_decoded_confined_and_shortened_without_breaking_utf8() {
        assert_eq!(filename("bracket%20v2.solidcraft"), Ok("bracket v2.solidcraft".into()));
        assert_eq!(filename("H%C3%A4uschen.SOLIDCRAFT"), Ok("Häuschen.SOLIDCRAFT".into()));
        assert_eq!(filename("recovery.solidcraft"), Ok("recovery.solidcraft".into()));
        for value in [
            "",
            "../part.solidcraft",
            "folder%2Fpart.solidcraft",
            "%2Fetc%2Fpasswd",
            "part%5Cname.solidcraft",
            "C%3Apart.solidcraft",
            "%00.solidcraft",
            ".private.solidcraft",
            "part.step",
            "broken%xx.solidcraft",
            "%FF.solidcraft",
        ] {
            assert!(filename(value).is_err(), "{value}");
        }
        let long = filename(&format!("{}.solidcraft", "ä".repeat(150)));
        assert!(long.is_ok_and(|name| name.len() <= 200 && name.ends_with(".solidcraft")));
    }

    #[test]
    fn design_envelope_accepts_native_zip_and_json_but_not_html_or_empty() {
        assert!(payload(b"PK\x03\x04native").is_ok());
        assert!(payload(b" \n {\"format\":\"solidcraft/1\"}").is_ok());
        for bytes in [b"".as_slice(), b"<!doctype html>", b"[]", b"STEP DATA"] {
            assert!(payload(bytes).is_err());
        }
    }

    #[test]
    fn simultaneous_browser_copies_use_distinct_bounded_names() {
        let first = copy_name("Häuschen.solidcraft", [1; 16]);
        let second = copy_name("Häuschen.solidcraft", [2; 16]);
        assert_ne!(first, second);
        assert!(first.starts_with("Häuschen (Browserkopie-"));
        assert!(first.ends_with(".solidcraft"));
        assert!(copy_name(&format!("{}.solidcraft", "ä".repeat(150)), [0; 16]).len() <= 200);
        assert!(!copy_name("recovery.solidcraft", [1; 16]).eq_ignore_ascii_case("recovery.solidcraft"));
    }

    #[test]
    fn original_open_command_reads_json_and_zip_without_replacing_existing_work() {
        use solidcraft_engine::{Session, doc::Document, io};
        use solidcraft_ui_egui::{Services, SolveApp, documents};

        let document = Document::new("Workspace copy");
        let variants = [io::write_design(&document), io::write_design_container(&document).unwrap_or_default()];
        for (index, bytes) in variants.iter().enumerate() {
            let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let directory = std::env::temp_dir().join(format!("solidcraft-browser-import-{}-{unique}-{index}", std::process::id()));
            assert!(std::fs::create_dir(&directory).is_ok());
            let path = directory.join("workspace-copy.solidcraft");
            assert!(std::fs::write(&path, bytes).is_ok());
            let mut app = SolveApp::new(Session::default(), Services::default());
            app.session.doc_mut().name = "Existing browser work".into();
            app.session.mark_unsaved();
            let Some(path) = path.to_str() else { return };
            assert!(open_copy(&mut app, path));
            assert_eq!(app.session.doc.name, "Workspace copy");
            assert_eq!(app.docs.count(), 2);
            documents::switch(&mut app, 0);
            assert_eq!(app.session.doc.name, "Existing browser work");
            assert!(app.session.is_dirty());
            let _ = std::fs::remove_dir_all(directory);
        }
    }

    #[test]
    fn full_tab_strip_refuses_import_before_original_open_can_replace_a_design() {
        use solidcraft_engine::Session;
        use solidcraft_ui_egui::{Services, SolveApp, documents};

        let mut app = SolveApp::new(Session::default(), Services::default());
        for _ in 1..documents::MAX_OPEN {
            assert!(documents::open_tab(&mut app, Session::default()).is_ok());
        }
        app.session.doc_mut().name = "Keep this design".into();
        app.session.mark_unsaved();
        let revision = app.session.revision;
        assert!(!open_copy(&mut app, "/never-open-this.solidcraft"));
        assert_eq!(app.docs.count(), documents::MAX_OPEN);
        assert_eq!(app.session.doc.name, "Keep this design");
        assert_eq!(app.session.revision, revision);
        assert!(app.session.is_dirty());
    }
}
