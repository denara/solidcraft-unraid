//! Server-rendered, read-only overview of persistent files and independent MCP sessions.

use std::fmt::Write as _;

use serde_json::Value;

const CSS: &str = r#"
:root{color-scheme:dark;--bg:#10151b;--panel:#171e26;--raised:#1e2731;--line:#2b3541;--text:#ecf1f5;--dim:#9eafbf;--green:#6cddb4;--amber:#f1c36b;--blue:#9bc8fa}
*{box-sizing:border-box}body{margin:0;background:var(--bg);font-family:Segoe UI,system-ui,sans-serif;color:var(--text);font-size:14px;line-height:1.5}button,input,select{font:inherit}button,a,select{touch-action:manipulation}button{color:var(--text);cursor:pointer}a{color:inherit}button:focus-visible,a:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--green);outline-offset:3px}
.top{min-height:76px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between;gap:20px;padding:16px 40px}.brand{display:flex;align-items:center;gap:12px;font-weight:700;font-size:20px;letter-spacing:-.4px;text-decoration:none}.mark{height:34px;width:34px;border:2px solid var(--green);border-radius:9px;display:grid;place-items:center;color:var(--green);font-size:22px}.brandnote{font-size:12px;color:var(--dim);font-weight:400;margin-left:12px;border-left:1px solid var(--line);padding-left:18px}.topright{display:flex;align-items:center;gap:20px;color:var(--dim);font-size:12px}.topright form{margin:0}.logout{border:0;background:transparent;color:var(--dim);font-size:12px;padding:6px 0}
.shell{max-width:1480px;margin:auto;padding:30px 40px 28px}.heading{display:flex;align-items:flex-end;justify-content:space-between;gap:20px;margin-bottom:25px}.eyebrow{text-transform:uppercase;font-size:10px;letter-spacing:1.8px;color:var(--green);font-weight:700}h1{font-size:29px;letter-spacing:-.7px;margin:4px 0 5px;line-height:1.25}p{margin:0;color:var(--dim)}.primary,.secondary{display:inline-block;text-decoration:none;padding:9px 14px;border-radius:7px;border:1px solid var(--line);background:var(--raised);white-space:nowrap;font-size:12px}.primary{color:var(--green);border-color:#3c645d;background:#1c332f}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:25px}.stat{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:16px 18px}.statlabel{font-size:12px;color:var(--dim);display:flex;gap:7px;align-items:center}.statnumber{font-size:26px;line-height:1.2;font-weight:650;margin:9px 0 5px;letter-spacing:-.6px}.statnote{font-size:11px;color:var(--dim)}.statdenom{font-size:15px;color:var(--dim)}.dot{height:7px;width:7px;border-radius:50%;background:var(--green);display:inline-block}.dot.amber{background:var(--amber)}.dot.blue{background:var(--blue)}
.work{display:grid;grid-template-columns:minmax(0,1fr) 320px;gap:20px}.panel{background:var(--panel);border:1px solid var(--line);border-radius:11px;overflow:hidden}.panelhead{padding:20px 20px 16px;display:flex;justify-content:space-between;align-items:center;gap:10px}h2{margin:0;font-size:16px;font-weight:650}.sub{font-size:12px;color:var(--dim);margin-top:3px}.counter{color:var(--dim);font-weight:400;margin-left:6px}.toolbar{margin:0 20px 16px;display:flex;gap:10px;flex-wrap:wrap}.search{min-width:120px;padding:9px 12px;border-radius:7px;border:1px solid var(--line);background:#111820;color:var(--text);flex:1;font-size:12px}.search::placeholder{color:var(--dim)}.filter{color:var(--text);background:var(--raised);border:1px solid var(--line);border-radius:7px;padding:7px 12px;font-size:12px}.tablewrap{overflow:auto}table{border-collapse:collapse;width:100%;text-align:left}th{font-size:10px;text-transform:uppercase;letter-spacing:1px;color:var(--dim);font-weight:600;background:#131b23;padding:11px 15px;border-top:1px solid var(--line);border-bottom:1px solid var(--line);white-space:nowrap}td{padding:16px 15px;border-bottom:1px solid var(--line);font-size:12px;vertical-align:top}th:first-child,td:first-child{padding-left:20px}.file{font-weight:600;font-size:12px;min-width:170px;max-width:300px;overflow-wrap:anywhere}.filetype{font-size:10px;color:var(--dim);margin-top:5px}.fileicon{display:inline-grid;place-items:center;width:31px;height:31px;border:1px solid #3b5063;border-radius:4px;margin-right:9px;float:left;color:var(--blue);font-size:8px}.pill{display:inline-block;padding:3px 8px;border-radius:5px;font-size:10px;font-weight:600;background:#203a32;color:var(--green);border:1px solid #345549}.pill.amber{color:var(--amber);background:#393123;border-color:#665334}.pill.blue{background:#25374a;color:var(--blue);border-color:#3b5067}.pill.gray{background:#232c35;color:#aebbc7;border-color:#35424e}.small{font-size:10px;color:var(--dim);margin-top:5px;overflow-wrap:anywhere}.action{color:var(--blue);font-size:11px;text-decoration:none;white-space:nowrap}.action:hover{text-decoration:underline}.nowrap{white-space:nowrap}
.note{margin:18px 20px;padding:11px 13px;border:1px solid #3b4c5b;border-radius:7px;background:#1a2632;color:#b8c9d9;font-size:11px;line-height:1.6}.note strong{color:var(--text);font-weight:600}.session{padding:16px 20px;border-top:1px solid var(--line)}.sessiontitle{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px}.sessionname{font-size:12px;font-weight:600}.mono{font-family:ui-monospace,Consolas,monospace;color:var(--dim);font-size:10px;font-weight:400;overflow-wrap:anywhere}.sessionfile{font-size:12px;margin-top:8px;overflow-wrap:anywhere}.sessiondetail{font-size:10px;color:var(--dim);margin-top:5px;line-height:1.7}.capacity{display:flex;gap:5px;margin:0 20px 16px}.slot{height:5px;flex:1;border-radius:3px;background:#344554}.slot.active{background:var(--green)}.slot.busy{background:var(--amber)}.unsaved{padding:14px 18px;border-top:1px solid var(--line);background:#211f1c;font-size:11px;color:var(--amber)}.empty{padding:32px 20px;text-align:center;color:var(--dim);font-size:12px}.empty strong{display:block;color:var(--text);font-size:14px;margin-bottom:7px}
.bottom{display:grid;grid-template-columns:1.2fr 1fr;gap:20px;margin-top:20px}.explain{padding:17px 20px}.explain h3{font-size:12px;margin:0 0 7px}.explain p{font-size:11px;line-height:1.7}.legend{display:flex;gap:16px;flex-wrap:wrap;margin-top:10px;font-size:10px;color:var(--dim)}footer{display:flex;justify-content:space-between;gap:16px;color:#8193a4;font-size:10px;margin-top:19px}.loginpanel{max-width:460px;margin:70px auto 30px;padding:28px}.loginpanel h1{font-size:25px;margin:7px 0 12px}.loginpanel p{font-size:12px}.loginform{display:flex;flex-direction:column;gap:10px;margin-top:24px}.loginform label{font-size:12px}.loginform input{width:100%;padding:11px 12px;background:#111820;border:1px solid var(--line);border-radius:7px;color:var(--text)}.loginform button{margin-top:6px}.loginhint{margin-top:20px!important;font-size:11px!important}.error{margin-top:18px;padding:10px 12px;border:1px solid #815253;border-radius:7px;color:#ffb6b6;background:#352327;font-size:12px}
@media(max-width:1150px){.work{grid-template-columns:1fr}.sessionslist{display:grid;grid-template-columns:repeat(3,minmax(0,1fr))}.session{border-right:1px solid var(--line)}.capacity{max-width:240px}.bottom{grid-template-columns:1fr 1fr}}@media(max-width:760px){.top{padding:14px 20px}.brandnote{display:none}.shell{padding:25px 18px}.topright{gap:12px}.topright>span{display:none}.cards{grid-template-columns:1fr 1fr}.heading{align-items:flex-start;flex-direction:column}.sessionslist{display:block}.bottom{grid-template-columns:1fr}h1{font-size:25px}.toolbar .secondary{flex:1}.loginpanel{margin-top:30px}footer{flex-direction:column;gap:4px}.stat{padding:14px}.statlabel{font-size:11px}}@media(max-width:400px){.cards{grid-template-columns:1fr}.panelhead{flex-wrap:wrap}}
"#;

fn escape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(character),
        }
    }
    result
}

fn download_url(name: &str) -> String {
    let mut result = String::from("/workspace/files/");
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            result.push(char::from(byte));
        } else {
            let _ = write!(result, "%{byte:02X}");
        }
    }
    result
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn entries<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

fn start(title: &str, csrf: Option<&str>) -> String {
    let mut html = format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>SolidCraft · {title}</title><style>{CSS}</style></head><body><header class=\"top\"><a class=\"brand\" href=\"/workspace\"><span class=\"mark\" aria-hidden=\"true\">◇</span>SolidCraft<span class=\"brandnote\">Dateien &amp; Sitzungen</span></a><div class=\"topright\"><span>Arbeitsbereich</span>"
    );
    if let Some(csrf) = csrf {
        let _ = write!(
            html,
            "<form method=\"post\" action=\"/workspace/logout\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"logout\" type=\"submit\">Abmelden</button></form>",
            escape(csrf)
        );
    }
    html.push_str("</div></header><main class=\"shell\">");
    html
}

/// Login never reflects a submitted token, including when authentication fails.
pub fn login(error: bool, csrf: &str) -> String {
    let mut html = start("Arbeitsbereich anmelden", None);
    html.push_str("<section class=\"panel loginpanel\"><div class=\"eyebrow\">Dein CAD-Arbeitsbereich</div><h1>Dateien &amp; Sitzungen</h1><p>Melde dich an, um gespeicherte Dateien und die aktuellen MCP-Sitzungen anzusehen.</p>");
    if error {
        html.push_str("<div class=\"error\" role=\"alert\">Anmeldung nicht möglich. Prüfe deinen Token und versuche es erneut. Nach mehreren Versuchen bitte kurz warten.</div>");
    }
    let _ = write!(
        html,
        "<form class=\"loginform\" method=\"post\" action=\"/workspace/login\" autocomplete=\"off\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label for=\"token\">MCP-Token</label><input id=\"token\" name=\"token\" type=\"password\" required minlength=\"32\" maxlength=\"512\" autocomplete=\"off\" spellcheck=\"false\" autocapitalize=\"none\"><button class=\"primary\" type=\"submit\">Arbeitsbereich öffnen</button></form><p class=\"loginhint\">Den Speicherort des Tokens findest du im Container-Log unter „token.location“. Verwende denselben Token wie für deinen MCP-Client.</p><p class=\"loginhint\">Die Anmeldung gilt für diese Übersicht. Die <a href=\"/\">Browser-CAD-App</a> bleibt unabhängig.</p></section></main></body></html>",
        escape(csrf)
    );
    html
}

/// Fixed explanations only; rejected requests never contribute content to HTML.
pub fn login_rejected(reason: &str) -> String {
    let explanation = match reason {
        "origin_missing" => {
            "Der Browser hat keine gültige Herkunft übermittelt. Öffne die Anmeldung direkt in einem neuen Tab. Prüfe gegebenenfalls Browser-Erweiterungen und die Referrer-Policy des Proxys."
        }
        "origin_not_allowed" => {
            "Diese Browser-Adresse ist nicht freigegeben. In der Container-Einstellung SOLVECRAFT_ALLOWED_ORIGINS muss die verwendete Adresse mit http:// oder https:// und gegebenenfalls Port stehen, ohne Pfad oder abschließenden Schrägstrich."
        }
        "host_mismatch" => {
            "Die vom Proxy übermittelte Host-Adresse passt nicht zur Browser-Adresse. Der Reverse Proxy muss den ursprünglichen Host-Header weiterreichen."
        }
        "csrf_cookie_missing" => "Das Anmelde-Cookie fehlt. Erlaube Cookies für diese Seite und verwende HTTPS. Lade die Anmeldung anschließend neu.",
        "csrf_mismatch" | "csrf_expired" => {
            "Diese Anmeldeseite ist abgelaufen oder wurde bereits verwendet. Lade die Anmeldung neu und gib den Token erneut ein."
        }
        "rate_limited" => "Zu viele fehlgeschlagene Anmeldungen. Warte eine Minute und lade die Anmeldung erneut.",
        "browser_capacity" => {
            "Die maximale Anzahl an Browser-Anmeldungen ist erreicht. Melde eine bestehende Sitzung ab oder warte, bis sie abgelaufen ist."
        }
        _ => "Das Anmeldeformular konnte nicht verarbeitet werden. Lade die Anmeldung neu und versuche es erneut.",
    };
    let mut html = start("Anmeldung nicht möglich", None);
    let _ = write!(
        html,
        "<section class=\"panel loginpanel\"><h1>Anmeldung nicht möglich</h1><div class=\"error\" role=\"alert\">{explanation}</div><a class=\"primary\" href=\"/workspace/login\">Anmeldung neu laden</a><p class=\"loginhint\">Der Token wurde bei diesem Versuch noch nicht geprüft. Der Container-Log enthält den Ablehnungsgrund.</p></section></main></body></html>"
    );
    html
}

fn elapsed(seconds: u64) -> String {
    match seconds {
        0..=1 => "gerade eben".into(),
        2..=59 => format!("vor {seconds} Sekunden"),
        60..=119 => "vor 1 Minute".into(),
        120..=3599 => format!("vor {} Minuten", seconds / 60),
        3600..=7199 => "vor 1 Stunde".into(),
        7200..=86399 => format!("vor {} Stunden", seconds / 3600),
        86400..=172799 => "vor 1 Tag".into(),
        _ => format!("vor {} Tagen", seconds / 86400),
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.1} KiB", bytes as f64 / 1024.0),
        1_048_576..=1_073_741_823 => format!("{:.1} MiB", bytes as f64 / 1_048_576.0),
        _ => format!("{:.1} GiB", bytes as f64 / 1_073_741_824.0),
    }
}

fn file_row(html: &mut String, file: &Value) {
    let name = text(file, "name");
    let kind = text(file, "kind");
    let opened = entries(file, "opened_by");
    let (icon, label) = match kind {
        "model" => ("CAD", "SolidCraft-Modell"),
        "export" => ("OUT", "Exportdatei"),
        _ => ("FILE", "Datei"),
    };
    let _ = write!(
        html,
        "<tr><td><span class=\"fileicon\" aria-hidden=\"true\">{icon}</span><div class=\"file\">{}</div><div class=\"filetype\">{label}</div></td><td>",
        escape(name)
    );
    if opened.is_empty() {
        let status = if kind == "export" { "Exportdatei" } else { "Nicht geöffnet" };
        let _ = write!(html, "<span class=\"pill gray\">{status}</span><div class=\"small\">Keine aktive Sitzung zugeordnet</div>");
    } else {
        let (class, label) = match file.get("dirty").and_then(Value::as_bool) {
            Some(true) => ("amber", "Ungespeicherte Änderungen"),
            Some(false) => ("", "Geöffnet · unverändert"),
            None => ("gray", "Änderungsstatus unbekannt"),
        };
        let _ = write!(
            html,
            "<span class=\"pill {class}\">{label}</span><div class=\"small\">In {} Sitzung{} geöffnet</div><div class=\"small mono\">",
            opened.len(),
            if opened.len() == 1 { "" } else { "en" }
        );
        for (index, display_id) in opened.iter().take(32).enumerate() {
            if index > 0 {
                html.push_str(" · ");
            }
            html.push_str(&escape(display_id.as_str().unwrap_or("?")));
        }
        html.push_str("</div>");
    }
    let modified = text(file, "modified_at");
    let date = chrono::DateTime::parse_from_rfc3339(modified)
        .map(|date| date.with_timezone(&chrono::Utc).format("%d.%m.%Y, %H:%M UTC").to_string())
        .unwrap_or_else(|_| "Unbekannt".into());
    let _ = write!(
        html,
        "</td><td class=\"nowrap\"><time datetime=\"{}\">{}</time></td><td class=\"nowrap\">{}</td><td><a class=\"action\" href=\"{}\" download>Herunterladen ↓</a></td></tr>",
        escape(modified),
        escape(&date),
        size(number(file, "size_bytes")),
        download_url(name)
    );
}

fn session_row(html: &mut String, session: &Value) {
    let current = session.get("current_file").and_then(Value::as_str);
    let busy = flag(session, "request_in_flight");
    let has_model = flag(session, "has_model") || flag(session, "dirty");
    let (class, status) = if busy {
        ("amber", "Führt aus")
    } else if current.is_none() && has_model {
        ("blue", "Neuer Entwurf")
    } else {
        ("gray", "Wartet")
    };
    let file_label = current.unwrap_or(if has_model { "Ohne Dateinamen" } else { "Noch kein Modell" });
    let _ = write!(
        html,
        "<section class=\"session\"><div class=\"sessiontitle\"><span class=\"sessionname\">Sitzung <span class=\"mono\">{}</span></span><span class=\"pill {class}\">{status}</span></div><div class=\"sessionfile\">{}</div><div class=\"sessiondetail\">",
        escape(text(session, "display_id")),
        escape(file_label)
    );
    let change_label = match session.get("dirty").and_then(Value::as_bool) {
        None => "Änderungsstatus unbekannt",
        _ if !has_model && current.is_none() => "Noch kein Modell angelegt",
        Some(true) if current.is_none() => "Noch nie gespeichert",
        Some(true) => "Ungespeicherte Änderungen",
        Some(false) if current.is_some() => "Unveränderter geöffneter Stand",
        Some(false) => "Noch nie gespeichert",
    };
    html.push_str(change_label);
    if let Some(seconds) = session.get("last_model_change_seconds").and_then(Value::as_u64) {
        let _ = write!(html, "<br>Letzte Modelländerung {}", elapsed(seconds));
    }
    let _ = write!(html, "<br>Letzte Anfrage {}", elapsed(number(session, "last_request_seconds")));
    if busy {
        html.push_str("<br>Eine Anfrage wird gerade ausgeführt");
    } else if current.is_some() {
        html.push_str("<br>Eigenes Modell · keine Dateisperre");
    } else if has_model {
        html.push_str("<br>Modell ist nur in dieser Sitzung vorhanden");
    }
    html.push_str("</div></section>");
}

/// Render an authenticated snapshot. The caller applies query/type filtering.
pub fn page(snapshot: &Value, query: &str, kind: &str) -> String {
    let files = entries(snapshot, "files");
    let sessions = entries(snapshot, "sessions");
    let total = number(snapshot, "slots_total");
    let used = number(snapshot, "slots_used");
    let busy = sessions.iter().filter(|session| flag(session, "request_in_flight")).count();
    let dirty_files = files.iter().filter(|file| flag(file, "dirty")).count();
    let models = files.iter().filter(|file| text(file, "kind") == "model").count();
    let exports = files.iter().filter(|file| text(file, "kind") == "export").count();
    let unsaved = sessions
        .iter()
        .filter(|session| session.get("current_file").and_then(Value::as_str).is_none() && (flag(session, "has_model") || flag(session, "dirty")))
        .count();
    let shared = files.iter().filter(|file| entries(file, "opened_by").len() > 1).count();
    let filtered = !query.is_empty() || !kind.is_empty() && kind != "all";
    let mut html = start("Dateien &amp; Sitzungen", Some(text(snapshot, "csrf")));
    html.push_str("<div class=\"heading\"><div><div class=\"eyebrow\">Dein CAD-Arbeitsbereich</div><h1>Was liegt vor. Was entsteht gerade.</h1><p>Gespeicherte Modelle, Exporte und die aktuellen MCP-Sitzungen an einem Ort.</p></div><a class=\"primary\" href=\"/\">Browser-CAD öffnen ↗</a></div>");
    let _ = write!(
        html,
        "<section class=\"cards\" aria-label=\"Übersicht\"><div class=\"stat\"><div class=\"statlabel\">{}</div><div class=\"statnumber\">{}</div><div class=\"statnote\">{models} Modelle · {exports} Exporte{}</div></div><div class=\"stat\"><div class=\"statlabel\"><span class=\"dot amber\" aria-hidden=\"true\"></span>Dateien in Bearbeitung</div><div class=\"statnumber\">{dirty_files}</div><div class=\"statnote\">{} mit bestätigten Änderungen</div></div><div class=\"stat\"><div class=\"statlabel\"><span class=\"dot\" aria-hidden=\"true\"></span>MCP-Sitzungen</div><div class=\"statnumber\">{used} <span class=\"statdenom\">/ {total}</span></div><div class=\"statnote\">{busy} beschäftigt · {} bereit</div></div><div class=\"stat\"><div class=\"statlabel\"><span class=\"dot blue\" aria-hidden=\"true\"></span>Neue Entwürfe</div><div class=\"statnumber\">{unsaved}</div><div class=\"statnote\">In Sitzungen · noch nie gespeichert</div></div></section>",
        if filtered { "Angezeigte Dateien" } else { "Gespeicherte Dateien" },
        files.len(),
        if files.len() > models + exports { " · weitere Dateien" } else { "" },
        if filtered { "Angezeigte Dateien" } else { "Gespeicherte Dateien" },
        sessions.len().saturating_sub(busy)
    );
    html.push_str("<div class=\"work\"><section class=\"panel\"><div class=\"panelhead\"><div><h2>Dateien <span class=\"counter\">");
    let _ = write!(
        html,
        "{}</span></h2><div class=\"sub\">Im gemeinsamen Serverspeicher</div></div><a class=\"secondary\" href=\"/workspace\">Aktualisieren ↻</a></div><form class=\"toolbar\" method=\"get\" action=\"/workspace\"><input class=\"search\" type=\"search\" name=\"q\" value=\"{}\" placeholder=\"Dateiname suchen …\" aria-label=\"Dateiname suchen\" maxlength=\"256\"><select class=\"filter\" name=\"kind\" aria-label=\"Dateityp\">",
        files.len(),
        escape(query)
    );
    for (value, label) in [("all", "Alle Dateitypen"), ("model", "SolidCraft-Modelle"), ("export", "Exporte"), ("other", "Weitere Dateien")] {
        let selected = kind == value || value == "all" && !matches!(kind, "model" | "export" | "other");
        let _ = write!(html, "<option value=\"{value}\"{}>{label}</option>", if selected { " selected" } else { "" });
    }
    html.push_str("</select><button class=\"secondary\" type=\"submit\">Anzeigen</button></form><div class=\"tablewrap\"><table><thead><tr><th scope=\"col\">Name</th><th scope=\"col\">Bearbeitung</th><th scope=\"col\">Zuletzt gespeichert</th><th scope=\"col\">Größe</th><th scope=\"col\"><span aria-label=\"Aktionen\">Download</span></th></tr></thead><tbody>");
    for file in files.iter().take(2000) {
        file_row(&mut html, file);
    }
    if files.is_empty() {
        html.push_str(if filtered {
            "<tr><td colspan=\"5\" class=\"empty\"><strong>Keine passenden Dateien</strong>Ändere den Suchbegriff oder den Dateityp. <a href=\"/workspace\">Alle Dateien anzeigen</a></td></tr>"
        } else {
            "<tr><td colspan=\"5\" class=\"empty\"><strong>Noch keine gespeicherten Dateien</strong>Speichert ein MCP-Client ein Modell oder einen Export im gemeinsamen Serverspeicher, erscheint die Datei hier nach dem Aktualisieren.</td></tr>"
        });
    }
    html.push_str("</tbody></table></div>");
    if flag(snapshot, "truncated") || files.len() > 2000 {
        html.push_str("<div class=\"note\"><strong>Die Dateiliste ist begrenzt.</strong> Es werden nicht alle gespeicherten Dateien angezeigt. Die angezeigten Zähler beziehen sich auf diesen Ausschnitt.</div>");
    }
    if shared > 0 {
        let _ = write!(
            html,
            "<div class=\"note\"><strong>{shared} Datei{} in mehreren Sitzungen geöffnet.</strong> Jede Sitzung hat ihr eigenes Modell im Arbeitsspeicher. „Geöffnet“ bedeutet keine Dateisperre. Prüfe beim Speichern mögliche Konflikte mit anderen Sitzungen.</div>",
            if shared == 1 { " ist" } else { "en sind" }
        );
    }
    let _ = write!(
        html,
        "</section><aside class=\"panel\"><div class=\"panelhead\"><div><h2>MCP-Sitzungen <span class=\"counter\">{used} / {total}</span></h2><div class=\"sub\">Freie Sitzungsplätze: {}</div></div></div><div class=\"capacity\" role=\"img\" aria-label=\"{used} von {total} Sitzungsplätzen belegt\">",
        total.saturating_sub(used)
    );
    for index in 0..total.min(32) {
        let class = if index < (busy as u64).min(used) {
            " busy"
        } else if index < used {
            " active"
        } else {
            ""
        };
        let _ = write!(html, "<span class=\"slot{class}\"></span>");
    }
    html.push_str("</div>");
    let closing = used.saturating_sub(sessions.len() as u64);
    if closing > 0 {
        let _ = write!(
            html,
            "<div class=\"note\">{closing} beendete Sitzung(en) schließen noch eine laufende Anfrage ab. Ihr Platz wird anschließend frei.</div>"
        );
    }
    html.push_str("<div class=\"sessionslist\">");
    for session in sessions.iter().take(32) {
        session_row(&mut html, session);
    }
    if sessions.is_empty() && used == 0 {
        html.push_str("<div class=\"empty\"><strong>Alle Sitzungen frei</strong>Sobald sich ein MCP-Client verbindet, erscheint seine Sitzung hier nach dem Aktualisieren.</div>");
    }
    html.push_str("</div><div class=\"unsaved\">Ungespeicherte Änderungen gehen beim Sitzungsende oder Container-Neustart verloren.</div></aside></div><div class=\"bottom\"><section class=\"panel explain\"><h3>Was bedeutet „in Bearbeitung“?</h3><p>Eine Sitzung enthält bestätigte Änderungen, die noch nicht in einer Datei gespeichert sind. „Geöffnet“ bedeutet nur, dass eine Sitzung eine Datei geladen hat. „Führt aus“ beschreibt eine gerade laufende Anfrage. Ein unbekannter Änderungsstatus zählt nicht als bestätigte Bearbeitung.</p><div class=\"legend\"><span><span class=\"dot amber\" aria-hidden=\"true\"></span> Ungespeichert / führt aus</span><span><span class=\"dot blue\" aria-hidden=\"true\"></span> Neuer Entwurf</span><span><span class=\"dot\" aria-hidden=\"true\"></span> Sitzungsplatz belegt</span></div></section><section class=\"panel explain\"><h3>Browser-CAD bleibt unabhängig</h3><p>Diese Übersicht zeigt Dateien und Sitzungen des MCP-Dienstes. Deine Browsermodelle leben weiterhin im Browser. Lade einen gespeicherten Stand herunter und importiere ihn in der CAD-App, wenn du ihn dort ansehen möchtest.</p><div class=\"small\">Lesende Übersicht · Sitzungen werden hier nicht beendet · Aktualisierung per Schaltfläche</div></section></div><footer><span>SolidCraft · Inoffizielle Unraid-Integration</span><span>Momentaufnahme · Zeiten in UTC · manuell aktualisieren</span></footer></main></body></html>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_only_unsaved_design_is_a_draft() {
        let snapshot = serde_json::json!({"files":[],"sessions":[{"display_id":"s-parameters","current_file":null,"has_model":false,"dirty":true}],"slots_total":4,"slots_used":1});
        let html = page(&snapshot, "", "");
        assert!(html.contains("Neuer Entwurf"));
        assert!(!html.contains("Noch kein Modell angelegt"));
    }
    use serde_json::json;

    #[test]
    fn untrusted_names_queries_and_csrf_are_escaped() {
        let hostile = "\"><script>alert('x')</script>&";
        let snapshot = json!({
            "csrf": hostile,
            "files": [{"name": format!("parts/{hostile}.solidcraft"), "kind":"model", "opened_by":[hostile], "dirty":null, "modified_at":hostile}],
            "sessions":[{"display_id":hostile,"current_file":hostile,"has_model":true,"dirty":null}],
            "slots_total":4,"slots_used":1
        });
        let html = page(&snapshot, hostile, hostile);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("alert('x')"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("/workspace/files/parts/%22%3E%3Cscript%3E"));
        assert!(html.contains("Änderungsstatus unbekannt"));
        assert!(!html.contains("<script "));
        assert!(!html.contains("http-equiv"));
    }

    #[test]
    fn urls_encode_each_utf8_segment_and_query_characters() {
        assert_eq!(download_url("öffentlich/teil #1?.solidcraft"), "/workspace/files/%C3%B6ffentlich/teil%20%231%3F.solidcraft");
        assert_eq!(download_url("x%2Fy&z.solidcraft"), "/workspace/files/x%252Fy%26z.solidcraft");
    }

    #[test]
    fn saved_dirty_files_and_unsaved_sessions_remain_distinct() {
        let snapshot = json!({
            "files":[{"name":"saved.solidcraft","kind":"model","dirty":true,"opened_by":["s-one","s-two"],"size_bytes":2048,"modified_at":"2026-01-01T01:02:03Z"}],
            "sessions":[
                {"display_id":"s-one","current_file":"saved.solidcraft","has_model":true,"dirty":true,"request_in_flight":true},
                {"display_id":"s-two","current_file":"saved.solidcraft","has_model":true,"dirty":false},
                {"display_id":"s-new","current_file":null,"has_model":true,"dirty":true},
                {"display_id":"s-empty","current_file":null,"has_model":false,"dirty":false}
            ], "slots_total":4,"slots_used":4
        });
        let html = page(&snapshot, "", "all");
        assert!(html.contains("Dateien in Bearbeitung</div><div class=\"statnumber\">1</div>"));
        assert!(html.contains("Neue Entwürfe</div><div class=\"statnumber\">1</div>"));
        assert!(html.contains("1 Datei ist in mehreren Sitzungen geöffnet"));
        assert!(html.contains("In 2 Sitzungen geöffnet"));
        assert!(html.contains("Noch kein Modell"));
        assert!(html.contains("01.01.2026, 01:02 UTC"));
        assert!(html.contains("2.0 KiB"));
        assert!(html.contains("Führt aus"));
        assert!(html.contains("Unveränderter geöffneter Stand"));
    }

    #[test]
    fn empty_filtered_and_truncated_states_are_explicit() {
        let html = page(&json!({"slots_total":4}), "", "all");
        assert!(html.contains("Noch keine gespeicherten Dateien"));
        assert!(html.contains("Alle Sitzungen frei"));
        let html = page(&json!({"truncated":true}), "part", "export");
        assert!(html.contains("Keine passenden Dateien"));
        assert!(html.contains("Angezeigte Dateien"));
        assert!(html.contains("value=\"export\" selected"));
        assert!(html.contains("Die Dateiliste ist begrenzt"));
    }

    #[test]
    fn login_has_no_token_value_or_browser_storage() {
        let html = login(true, "\"><script>bad</script>");
        assert!(html.contains("type=\"password\" required minlength"));
        assert!(html.contains("autocomplete=\"off\""));
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("localStorage"));
        assert!(!html.contains("name=\"token\" value="));
        assert!(!login(false, "csrf").contains("role=\"alert\""));
    }
}
