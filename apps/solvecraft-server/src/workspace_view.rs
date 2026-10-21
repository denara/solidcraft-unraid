//! Server-rendered file management and overview of persistent files and independent MCP sessions.

use std::fmt::Write as _;

use serde_json::Value;

const ICON_FOLDER: &str = include_str!("../../../assets/icons/ui/design/svg/model/folder.svg");
const ICON_MODEL: &str = include_str!("../../../assets/icons/ui/design/svg/model/box.svg");
const ICON_EXPORT: &str = include_str!("../../../assets/icons/ui/design/svg/model/export.svg");
const ICON_FILE: &str = include_str!("../../../assets/icons/ui/design/svg/model/library.svg");
const ICON_PALETTE: &str = include_str!("../../../assets/icons/ui/design/palette.toml");

// Reuse the native application's original masters and its single palette authority.
// Convert the export master's GAP paint into real transparency, matching the
// native renderer. The editable master and all its geometry stay unchanged.
fn icon(source: &str) -> String {
    let mut svg = if source == ICON_EXPORT {
        static NEXT_MASK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_MASK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut before = String::new();
        let mut cut = String::new();
        let mut after = String::new();
        let mut passed = false;
        for line in source.lines().skip(1).filter(|line| !line.contains("</svg>")) {
            if line.contains("\"#ffffff\"") {
                cut.push_str(&line.replace("#ffffff", "black"));
                passed = true;
            } else if passed {
                after.push_str(line);
            } else {
                before.push_str(line);
            }
        }
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><defs><mask id=\"workspace-export-{id}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"64\" height=\"64\"><path d=\"M0 0L64 0L64 64L0 64Z\" fill=\"white\"/>{cut}</mask></defs><g mask=\"url(#workspace-export-{id})\">{before}</g>{after}</svg>"
        )
    } else {
        source.into()
    };
    svg = svg.replace("<svg ", "<svg class=\"nativeicon\" aria-hidden=\"true\" focusable=\"false\" ");
    // Parse only the two fixed quoted colour fields in role sections, leaving
    // accent ramps untouched. No user-provided SVG or palette is ever accepted.
    for section in ICON_PALETTE.split("[roles.").skip(1) {
        let mut source = None;
        for line in section.lines().skip(1).take_while(|line| !line.starts_with('[')) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim().split('"').nth(1).unwrap_or_default();
            match key.trim() {
                "source" => source = Some(value),
                "dark" => {
                    if let Some(source) = source {
                        svg = svg.replace(source, value);
                    }
                }
                _ => {}
            }
        }
    }
    svg
}

const CSS: &str = r#"
.folderactions{display:flex;align-items:center;justify-content:flex-end;flex-wrap:wrap;gap:14px;padding:6px 20px;border-top:1px solid var(--line);color:var(--dim);font-size:11px;line-height:1.4}.folderactions>span{margin-right:auto;overflow-wrap:anywhere;min-width:0}.folderactions a{white-space:nowrap}.danger{color:#ffaeae}.deletewarning{border:1px solid #805052;background:#332125;padding:16px;border-radius:8px;margin:20px 0}.deleteform{display:flex;gap:16px;align-items:center;flex-wrap:wrap}.deleteform .primary{background:#a13e48;color:white}.deleteform label{width:100%}

:root{color-scheme:dark;--bg:#10151b;--panel:#171e26;--raised:#1e2731;--line:#2b3541;--text:#ecf1f5;--dim:#9eafbf;--green:#6cddb4;--amber:#f1c36b;--blue:#9bc8fa}
*{box-sizing:border-box}body{margin:0;background:var(--bg);font-family:Segoe UI,system-ui,sans-serif;color:var(--text);font-size:14px;line-height:1.5}button,input,select{font:inherit}button,a,select{touch-action:manipulation}button{color:var(--text);cursor:pointer}a{color:inherit}button:focus-visible,a:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--green);outline-offset:3px}
.top{min-height:76px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between;gap:20px;padding:16px 40px}.brand{display:flex;align-items:center;gap:12px;font-weight:700;font-size:20px;letter-spacing:-.4px;text-decoration:none}.mark{height:34px;width:34px;border:2px solid var(--green);border-radius:9px;display:grid;place-items:center;color:var(--green);font-size:22px}.brandnote{font-size:12px;color:var(--dim);font-weight:400;margin-left:12px;border-left:1px solid var(--line);padding-left:18px}.topright{display:flex;align-items:center;gap:20px;color:var(--dim);font-size:12px}.topright form{margin:0}.logout{border:0;background:transparent;color:var(--dim);font-size:12px;padding:6px 0}
.shell{max-width:1480px;margin:auto;padding:30px 40px 28px}.heading{display:flex;align-items:flex-end;justify-content:space-between;gap:20px;margin-bottom:25px}.eyebrow{text-transform:uppercase;font-size:10px;letter-spacing:1.8px;color:var(--green);font-weight:700}h1{font-size:29px;letter-spacing:-.7px;margin:4px 0 5px;line-height:1.25}p{margin:0;color:var(--dim)}.primary,.secondary{display:inline-block;text-decoration:none;padding:9px 14px;border-radius:7px;border:1px solid var(--line);background:var(--raised);white-space:nowrap;font-size:12px}.primary{color:var(--green);border-color:#3c645d;background:#1c332f}
.cards{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:25px}.stat{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:16px 18px}.statlabel{font-size:12px;color:var(--dim);display:flex;gap:7px;align-items:center}.statnumber{font-size:26px;line-height:1.2;font-weight:650;margin:9px 0 5px;letter-spacing:-.6px}.statnote{font-size:11px;color:var(--dim)}.statdenom{font-size:15px;color:var(--dim)}.dot{height:7px;width:7px;border-radius:50%;background:var(--green);display:inline-block}.dot.amber{background:var(--amber)}.dot.blue{background:var(--blue)}
.work{display:grid;grid-template-columns:minmax(0,1fr) 320px;gap:20px}.panel{background:var(--panel);border:1px solid var(--line);border-radius:11px;overflow:hidden}.panelhead{padding:20px 20px 16px;display:flex;justify-content:space-between;align-items:center;gap:10px}h2{margin:0;font-size:16px;font-weight:650}.sub{font-size:12px;color:var(--dim);margin-top:3px}.counter{color:var(--dim);font-weight:400;margin-left:6px}.toolbar{margin:0 20px 16px;display:flex;gap:10px;flex-wrap:wrap}.search{min-width:120px;padding:9px 12px;border-radius:7px;border:1px solid var(--line);background:#111820;color:var(--text);flex:1;font-size:12px}.search::placeholder{color:var(--dim)}.filter{color:var(--text);background:var(--raised);border:1px solid var(--line);border-radius:7px;padding:7px 12px;font-size:12px}.tablewrap{overflow:auto}table{border-collapse:collapse;width:100%;text-align:left}th{font-size:10px;text-transform:uppercase;letter-spacing:1px;color:var(--dim);font-weight:600;background:#131b23;padding:11px 15px;border-top:1px solid var(--line);border-bottom:1px solid var(--line);white-space:nowrap}td{padding:16px 15px;border-bottom:1px solid var(--line);font-size:12px;vertical-align:top}th:first-child,td:first-child{padding-left:20px}.file{font-weight:600;font-size:12px;min-width:170px;max-width:300px;overflow-wrap:anywhere}.filetype{font-size:10px;color:var(--dim);margin-top:5px}.fileicon{display:inline-grid;place-items:center;width:31px;height:31px;border:1px solid #3b5063;border-radius:4px;margin-right:9px;float:left;color:var(--blue);font-size:8px}.pill{display:inline-block;padding:3px 8px;border-radius:5px;font-size:10px;font-weight:600;background:#203a32;color:var(--green);border:1px solid #345549}.pill.amber{color:var(--amber);background:#393123;border-color:#665334}.pill.blue{background:#25374a;color:var(--blue);border-color:#3b5067}.pill.gray{background:#232c35;color:#aebbc7;border-color:#35424e}.small{font-size:10px;color:var(--dim);margin-top:5px;overflow-wrap:anywhere}.action{color:var(--blue);font-size:11px;text-decoration:none;white-space:nowrap}.action:hover{text-decoration:underline}.nowrap{white-space:nowrap}
.note{margin:18px 20px;padding:11px 13px;border:1px solid #3b4c5b;border-radius:7px;background:#1a2632;color:#b8c9d9;font-size:11px;line-height:1.6}.note strong{color:var(--text);font-weight:600}.session{padding:16px 20px;border-top:1px solid var(--line)}.sessiontitle{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px}.sessionname{font-size:12px;font-weight:600}.mono{font-family:ui-monospace,Consolas,monospace;color:var(--dim);font-size:10px;font-weight:400;overflow-wrap:anywhere}.sessionfile{font-size:12px;margin-top:8px;overflow-wrap:anywhere}.sessiondetail{font-size:10px;color:var(--dim);margin-top:5px;line-height:1.7}.capacity{display:flex;gap:5px;margin:0 20px 16px}.slot{height:5px;flex:1;border-radius:3px;background:#344554}.slot.active{background:var(--green)}.slot.busy{background:var(--amber)}.unsaved{padding:14px 18px;border-top:1px solid var(--line);background:#211f1c;font-size:11px;color:var(--amber)}.empty{padding:32px 20px;text-align:center;color:var(--dim);font-size:12px}.empty strong{display:block;color:var(--text);font-size:14px;margin-bottom:7px}
.bottom{display:grid;grid-template-columns:1.2fr 1fr;gap:20px;margin-top:20px}.explain{padding:17px 20px}.explain h3{font-size:12px;margin:0 0 7px}.explain p{font-size:11px;line-height:1.7}.legend{display:flex;gap:16px;flex-wrap:wrap;margin-top:10px;font-size:10px;color:var(--dim)}footer{display:flex;justify-content:space-between;gap:16px;color:#8193a4;font-size:10px;margin-top:19px}.loginpanel{max-width:460px;margin:70px auto 30px;padding:28px}.loginpanel h1{font-size:25px;margin:7px 0 12px}.loginpanel p{font-size:12px}.loginform{display:flex;flex-direction:column;gap:10px;margin-top:24px}.loginform label{font-size:12px}.loginform input{width:100%;padding:11px 12px;background:#111820;border:1px solid var(--line);border-radius:7px;color:var(--text)}.loginform button{margin-top:6px}.loginhint{margin-top:20px!important;font-size:11px!important}.error{margin-top:18px;padding:10px 12px;border:1px solid #815253;border-radius:7px;color:#ffb6b6;background:#352327;font-size:12px}
@media(max-width:1150px){.work{grid-template-columns:1fr}.sessionslist{display:grid;grid-template-columns:repeat(3,minmax(0,1fr))}.session{border-right:1px solid var(--line)}.capacity{max-width:240px}.bottom{grid-template-columns:1fr 1fr}}@media(max-width:760px){.top{padding:14px 20px}.brandnote{display:none}.shell{padding:25px 18px}.topright{gap:12px}.topright>span{display:none}.cards{grid-template-columns:1fr 1fr}.heading{align-items:flex-start;flex-direction:column}.sessionslist{display:block}.bottom{grid-template-columns:1fr}h1{font-size:25px}.toolbar .secondary{flex:1}.loginpanel{margin-top:30px}footer{flex-direction:column;gap:4px}.stat{padding:14px}.statlabel{font-size:11px}}@media(max-width:400px){.cards{grid-template-columns:1fr}.panelhead{flex-wrap:wrap}}
.nativeicon{width:30px;height:30px;display:block;flex-shrink:0}.fileicon{border:0;background:none;padding:0;height:34px;width:34px}.fileicon .nativeicon{width:34px;height:34px}.project{border-top:1px solid var(--line)}.project>summary{padding:14px 20px;display:flex;align-items:center;gap:10px;cursor:pointer;list-style:none;background:#1b2530}.project>summary::-webkit-details-marker{display:none}.project>summary:before{content:'›';font-size:22px;color:var(--dim);width:12px;transition:transform .15s}.project[open]>summary:before{transform:rotate(90deg)}.project>summary:focus-visible{outline:2px solid var(--green);outline-offset:-3px}.projectname{font-weight:650;overflow-wrap:anywhere}.projectmeta{font-size:10px;color:var(--dim);margin-top:2px}.projectcount{margin-left:auto;color:var(--dim);font-size:11px;white-space:nowrap}.project table{table-layout:auto}.actions{display:flex;gap:12px;align-items:center;flex-wrap:wrap}.detailslink{color:var(--text);text-decoration:none}.detailslink:hover{text-decoration:underline;color:var(--blue)}.livebar{display:flex;align-items:center;gap:8px;color:var(--dim);font-size:11px;margin-bottom:14px;min-height:20px}.livebar .dot{background:var(--amber)}.livebar[data-state=connected] .dot{background:var(--green)}.livebar a{margin-left:auto;text-decoration:none;color:var(--blue)}.filters{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:14px 16px;margin:0 0 18px}.filters .toolbar{margin:0}.filesintro{padding:0 20px 16px;font-size:11px;color:var(--dim)}.remember{padding:10px 12px;background:#111820;border:1px solid var(--line);border-radius:7px;color:var(--text);width:100%}.remembernote{font-size:11px!important;margin-top:0!important}.filedialog{color:var(--text);background:var(--panel);border:1px solid #496071;border-radius:14px;padding:0;width:min(680px,calc(100% - 32px));max-height:85vh;overflow:auto;box-shadow:0 24px 90px #0009}.filedialog::backdrop{background:#080d13b8;backdrop-filter:blur(3px)}.dialoghead{position:sticky;top:0;display:flex;justify-content:space-between;gap:20px;align-items:center;background:var(--panel);padding:16px 24px;border-bottom:1px solid var(--line);z-index:1}.dialoghead h2{font-size:14px}.dialogclose{width:32px;height:32px;border:1px solid var(--line);border-radius:7px;background:var(--raised);font-size:20px;line-height:1}.detailsbody{padding:24px}.detailsbody h1{font-size:23px;overflow-wrap:anywhere;margin-top:0}.detailspath{font-size:11px;overflow-wrap:anywhere;color:var(--dim);margin:6px 0 20px}.detailgrid{display:grid;grid-template-columns:minmax(130px,.8fr) minmax(0,1.3fr);margin:0}.detailgrid dt,.detailgrid dd{margin:0;padding:10px 0;border-bottom:1px solid var(--line);overflow-wrap:anywhere;font-size:12px}.detailgrid dt{color:var(--dim);padding-right:20px}.detailgrid dd{white-space:pre-wrap}.detailheading{font-size:13px;margin:26px 0 8px}.detailnote{font-size:11px;line-height:1.7;margin-bottom:12px}.detailpanel{max-width:760px;margin:0 auto}.detailsbody .actions{margin-top:24px}.detailfallback{margin-top:18px}.filedialog .detailfallback{display:none}.filedialog:focus{outline:none}.filedialog .detailsbody{border:0}.visuallyhidden{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}@media(prefers-reduced-motion:reduce){.project>summary:before{transition:none}}@media(max-width:760px){.projectcount{font-size:10px}.project>summary{padding:12px 16px}.detailgrid{grid-template-columns:110px minmax(0,1fr)}.detailsbody{padding:20px}.dialoghead{padding:12px 20px}.filters{padding:12px}.actions{gap:8px}.project table th:nth-child(3),.project table td:nth-child(3){display:none}}
.file-explorer{background:#1a2530;border-color:#354857}.file-explorer>.panelhead,.file-explorer>.filesintro{background:#1d2b37}.file-explorer .panelhead{padding-bottom:12px}.file-explorer .filesintro{margin:0;padding-bottom:17px}.file-explorer .project{border-top-color:#354857}.file-explorer .project>summary{background:#223241}.file-explorer .project>summary:hover{background:#273b4c}.file-explorer .project table{background:#19242e}.file-explorer th{background:#17212b;border-color:#30414f}.file-explorer td{border-bottom-color:#30414f}.file-explorer tbody tr:hover{background:#1e2e3c}.filepreview{position:relative;width:48px;height:40px;display:grid;place-items:center;overflow:hidden;border:1px solid #40576a;border-radius:6px;background:#111b24}.filepreview .nativeicon{width:30px;height:30px}.filepreview:has(img) .nativeicon{visibility:hidden}.filethumb{position:absolute;inset:0;display:block;width:48px;height:40px;object-fit:cover;border-radius:inherit}.detailpreview{display:grid;place-items:center;margin:0 0 22px;padding:12px;background:#101923;border:1px solid #354857;border-radius:9px;min-height:90px}.detailpreview img{display:block;max-width:100%;max-height:320px;object-fit:contain;border-radius:4px}.detailpreview figcaption{font-size:10px;color:var(--dim);padding-top:9px}.detailpreview[hidden]{display:none}
.observebutton{margin-top:12px;width:100%;font-size:11px}.observerdialog{width:min(880px,calc(100% - 32px))}.observerbody{padding:20px 24px}.observerstatus{display:flex;gap:8px;align-items:center;font-size:12px;min-height:24px}.observerstatus .dot{background:var(--amber)}.observerstatus[data-state=ready] .dot{background:var(--green)}.observerstage{margin:16px 0 10px;min-height:250px;aspect-ratio:8/5;display:grid;place-items:center;background:#101923;border:1px solid #354857;border-radius:9px;overflow:hidden;position:relative}.observerstage img{width:100%;height:100%;object-fit:contain;display:block;grid-area:1/1}.observerstage img[hidden]{display:none}.observerplaceholder{grid-area:1/1;color:var(--dim);font-size:13px;padding:24px;text-align:center}.observerplaceholder[hidden]{display:none}.observermeta{font-size:11px;color:var(--dim);min-height:20px;overflow-wrap:anywhere}.observernote{font-size:11px;color:var(--dim);line-height:1.7;margin:12px 0 0}.observerdialog[data-state=busy] .observerstage img,.observerdialog[data-state=rendering] .observerstage img,.observerdialog[data-state=unavailable] .observerstage img{opacity:.65}@media(max-width:760px){.observerbody{padding:16px}.observerstage{min-height:180px;aspect-ratio:4/3}}

/* Stable filters and live counters share one visual surface. The form stays
   outside the live fragment so updates preserve typed text and keyboard focus. */
.filters{border-bottom:0;border-radius:10px 10px 0 0;margin-bottom:0;padding-bottom:16px}
.cards{gap:0;background:var(--panel);border:1px solid var(--line);border-top:0;border-radius:0 0 10px 10px;overflow:hidden}
.stat{background:transparent;border:0;border-radius:0;min-width:0}
.stat+.stat{border-left:1px solid var(--line)}
@media(max-width:760px){.stat:nth-child(odd){border-left:0}.stat:nth-child(n+3){border-top:1px solid var(--line)}}
@media(max-width:400px){.stat+.stat{border-left:0;border-top:1px solid var(--line)}.filters .search{flex-basis:100%}}

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
        "<form class=\"loginform\" method=\"post\" action=\"/workspace/login\" autocomplete=\"off\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label for=\"token\">MCP-Token</label><input id=\"token\" name=\"token\" type=\"password\" required minlength=\"32\" maxlength=\"512\" autocomplete=\"off\" spellcheck=\"false\" autocapitalize=\"none\"><label for=\"remember\">Angemeldet bleiben</label><select class=\"remember\" id=\"remember\" name=\"remember\"><option value=\"session\">Für diese Sitzung (1 Stunde)</option><option value=\"7d\">7 Tage</option><option value=\"30d\">30 Tage</option><option value=\"forever\">Dauerhaft auf diesem Gerät</option></select><p class=\"remembernote\">Nur auf deinem eigenen Gerät verwenden. Gespeichert wird eine geschützte Browser-Anmeldung.</p><button class=\"primary\" type=\"submit\">Arbeitsbereich öffnen</button></form><p class=\"loginhint\">Den Speicherort des Tokens findest du im Container-Log unter „token.location“. Verwende denselben Token wie für deinen MCP-Client.</p><p class=\"loginhint\">Die Anmeldung gilt für diese Übersicht. Die <a href=\"/\">Browser-CAD-App</a> bleibt unabhängig.</p></section></main></body></html>",
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
        "browser_storage_unavailable" => {
            "Die Browser-Anmeldung konnte nicht sicher gespeichert oder widerrufen werden. Prüfe das beschreibbare Datenvolume. Für eine neue Anmeldung kannst du vorübergehend die Option für eine Stunde verwenden."
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

fn event_date(seconds: u64) -> String {
    i64::try_from(seconds)
        .ok()
        .and_then(chrono::TimeDelta::try_seconds)
        .and_then(|duration| chrono::Utc::now().checked_sub_signed(duration))
        .map(|date| date.format("%d.%m.%Y, %H:%M UTC").to_string())
        .unwrap_or_else(|| "Zeitpunkt unbekannt".into())
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
    let (source, label) = match kind {
        "model" => (ICON_MODEL, "SolidCraft-Modell"),
        "export" => (ICON_EXPORT, "Exportdatei"),
        _ => (ICON_FILE, "Datei"),
    };
    let mut preview = icon(source);
    let preview_class = if is_png(name) {
        let _ = write!(
            preview,
            "<img class=\"filethumb\" src=\"{}\" loading=\"lazy\" decoding=\"async\" width=\"48\" height=\"40\" alt=\"\" data-file-preview>",
            preview_url(name)
        );
        " filepreview"
    } else {
        ""
    };
    let short_name = name.split_once('/').map(|(_, rest)| rest).unwrap_or(name);
    let details = download_url(name).replacen("/workspace/files/", "/workspace/details/", 1);
    let _ = write!(
        html,
        "<tr><td><span class=\"fileicon{preview_class}\" aria-hidden=\"true\">{preview}</span><div class=\"file\"><a class=\"detailslink\" href=\"{details}\" data-file-details=\"{details}\">{}</a></div><div class=\"filetype\">{label} · {}</div></td><td>",
        escape(short_name),
        escape(text(file, "format"))
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
        "</td><td class=\"nowrap\"><time datetime=\"{}\">{}</time></td><td class=\"nowrap\">{}</td><td><div class=\"actions\"><a class=\"action\" href=\"{details}\" data-file-details=\"{details}\">Details</a>",
        escape(modified),
        escape(&date),
        size(number(file, "size_bytes"))
    );
    edit_link(html, file, "action", "Bearbeiten ↗");
    let _ = write!(html, "<a class=\"action\" href=\"{}\" download>Download ↓</a>", download_url(name));
    delete_link(html, name);
    html.push_str("</div>");
}

fn is_png(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("png"))
}

fn preview_url(name: &str) -> String {
    download_url(name).replacen("/workspace/files/", "/workspace/preview/", 1)
}

fn edit_link(html: &mut String, file: &Value, class: &str, label: &str) {
    if text(file, "kind") == "model" && text(file, "name").to_ascii_lowercase().ends_with(".solidcraft") {
        let url = download_url(text(file, "name")).replacen("/workspace/files/", "/workspace/open/", 1);
        let _ = write!(
            html,
            "<a class=\"{class}\" href=\"{url}\" target=\"_blank\" rel=\"noopener\" title=\"Gespeicherte Kopie in einem neuen Browser-CAD-Tab öffnen\">{label}</a>"
        );
    }
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
    if let Some(project) = session.get("project").and_then(Value::as_str).filter(|project| !project.is_empty()) {
        let _ = write!(html, "<br>Projekt: {}", escape(project));
    } else {
        html.push_str("<br>Projekt noch nicht gewählt");
    }
    if let Some(seconds) = session.get("last_model_change_seconds").and_then(Value::as_u64) {
        let _ = write!(html, "<br>Letzte Modelländerung: {}", event_date(seconds));
    }
    let _ = write!(html, "<br>Letzte Anfrage: {}", event_date(number(session, "last_request_seconds")));
    if busy {
        html.push_str("<br>Eine Anfrage wird gerade ausgeführt");
    } else if current.is_some() {
        html.push_str("<br>Eigenes Modell · keine Dateisperre");
    } else if has_model {
        html.push_str("<br>Modell ist nur in dieser Sitzung vorhanden");
    }
    html.push_str("</div>");
    let handle = text(session, "observer_handle");
    if handle.len() == 36 && uuid::Uuid::parse_str(handle).is_ok() {
        let _ = write!(
            html,
            "<button class=\"secondary observebutton\" type=\"button\" data-observe=\"{}\" aria-haspopup=\"dialog\" aria-controls=\"observer-dialog\">Live-Vorschau ansehen</button>",
            escape(handle)
        );
    }
    html.push_str("</section>");
}

fn project_files(html: &mut String, files: &[Value], directories: &[Value]) {
    let mut projects = std::collections::BTreeMap::<String, Vec<&Value>>::new();
    for file in files.iter().take(2000) {
        let project = file.get("project").unwrap_or(&Value::Null);
        let id = text(project, "id");
        projects.entry(if id.is_empty() { text(file, "name").into() } else { id.into() }).or_default().push(file);
    }
    for directory in directories.iter().filter_map(Value::as_str) {
        let name = directory.split('/').next().unwrap_or(directory);
        projects.entry(format!("directory:{name}")).or_default();
    }
    for (id, files) in projects {
        let fallback = serde_json::json!({"name":id.strip_prefix("directory:").unwrap_or(&id),"basis":"directory"});
        let first = files.first().copied().unwrap_or(&Value::Null);
        let project = first.get("project").unwrap_or(&fallback);
        let name = text(project, "name");
        let name = if text(project, "basis") == "ungrouped" {
            "Weitere Dateien"
        } else if name.is_empty() {
            text(first, "name")
        } else {
            name
        };
        let basis = match text(project, "basis") {
            "directory" => "Projektordner im Serverspeicher",
            "design_stem" | "file_stem" => "Virtuelle Gruppe aus Dateinamen",
            "ungrouped" => "Ohne eindeutige Projektzuordnung",
            _ => "Einzeldatei",
        };
        let _ = write!(
            html,
            "<details class=\"project\" data-project=\"{}\"><summary>{}<div><div class=\"projectname\">{}</div><div class=\"projectmeta\">{basis}</div></div><span class=\"projectcount\">{} Datei{}</span></summary>",
            escape(&id),
            icon(ICON_FOLDER),
            escape(name),
            files.len(),
            if files.len() == 1 { "" } else { "en" }
        );
        if text(project, "basis") == "directory" {
            // Folder actions use authoritative filesystem paths, never a virtual stem.
            let prefix = format!("{name}/");
            for directory in
                std::iter::once(name).chain(directories.iter().filter_map(Value::as_str).filter(|directory| directory.starts_with(&prefix)))
            {
                let _ = write!(html, "<div class=\"folderactions\" role=\"group\" aria-label=\"Ordneraktionen für {}\">", escape(directory));
                if let Some(relative) = directory.strip_prefix(&prefix) {
                    let _ = write!(html, "<span>{}</span>", escape(relative));
                }
                folder_actions(html, directory);
                html.push_str("</div>");
            }
        }
        html.push_str("<div class=\"tablewrap\"><table><thead><tr><th scope=\"col\">Name</th><th scope=\"col\">Bearbeitung</th><th scope=\"col\">Zuletzt gespeichert</th><th scope=\"col\">Größe</th><th scope=\"col\">Aktionen</th></tr></thead><tbody>");
        for file in files {
            file_row(html, file);
        }
        html.push_str("</tbody></table></div></details>");
    }
}

/// The query form and dialog stay outside the live region so incoming updates
/// do not interrupt typing, close a properties popup or reset keyboard focus.
pub fn page(snapshot: &Value, query: &str, kind: &str) -> String {
    let mut html = start("Dateien &amp; Sitzungen", Some(text(snapshot, "csrf")));
    html.push_str("<div class=\"heading\"><div><div class=\"eyebrow\">Dein CAD-Arbeitsbereich</div><h1>Was liegt vor. Was entsteht gerade.</h1><p>Gespeicherte Modelle, Exporte und die aktuellen MCP-Sitzungen an einem Ort.</p></div><a class=\"primary\" href=\"/\">Browser-CAD öffnen ↗</a></div><div id=\"workspace-status\" class=\"livebar\" data-state=\"connecting\" role=\"status\" aria-live=\"polite\"><span class=\"dot\" aria-hidden=\"true\"></span><span id=\"workspace-status-text\">Live-Verbindung wird hergestellt …</span><a href=\"\" aria-label=\"Arbeitsbereich mit denselben Filtern aktualisieren\">Jetzt aktualisieren ↻</a></div><section class=\"filters\" aria-label=\"Dateien filtern\"><form class=\"toolbar\" method=\"get\" action=\"/workspace\">");
    let _ = write!(
        html,
        "<input class=\"search\" type=\"search\" name=\"q\" value=\"{}\" placeholder=\"Dateiname suchen …\" aria-label=\"Dateiname suchen\" maxlength=\"256\"><select class=\"filter\" name=\"kind\" aria-label=\"Dateityp\">",
        escape(query)
    );
    for (value, label) in [("all", "Alle Dateitypen"), ("model", "SolidCraft-Modelle"), ("export", "Exporte"), ("other", "Weitere Dateien")] {
        let selected = kind == value || value == "all" && !matches!(kind, "model" | "export" | "other");
        let _ = write!(html, "<option value=\"{value}\"{}>{label}</option>", if selected { " selected" } else { "" });
    }
    html.push_str("</select><button class=\"secondary\" type=\"submit\">Anzeigen</button></form><noscript><p class=\"note\">Aktualisiere diese Seite, um neue Dateien und Sitzungsänderungen zu sehen. Dateidetails lassen sich als eigene Seite öffnen.</p></noscript></section><div id=\"workspace-live\">");
    html.push_str(&fragment(snapshot, query, kind));
    html.push_str("</div><dialog id=\"file-dialog\" class=\"filedialog\" aria-labelledby=\"file-dialog-title\"><div class=\"dialoghead\"><h2 id=\"file-dialog-title\">Dateidetails</h2><button id=\"file-dialog-close\" class=\"dialogclose\" type=\"button\" aria-label=\"Dialog schließen\">×</button></div><div id=\"file-dialog-content\"></div></dialog>");
    html.push_str("<dialog id=\"observer-dialog\" class=\"filedialog observerdialog\" aria-labelledby=\"observer-title\" aria-describedby=\"observer-note\"><div class=\"dialoghead\"><h2 id=\"observer-title\">Live-Vorschau der MCP-Sitzung</h2><button id=\"observer-close\" class=\"dialogclose\" type=\"button\" autofocus aria-label=\"Live-Vorschau schließen\">×</button></div><div class=\"observerbody\"><div id=\"observer-status\" class=\"observerstatus\" role=\"status\" aria-live=\"polite\"><span class=\"dot\" aria-hidden=\"true\"></span><span id=\"observer-status-text\">Vorschau wird verbunden …</span></div><figure class=\"observerstage\"><img id=\"observer-image\" alt=\"Aktueller Modellstand der beobachteten MCP-Sitzung\" decoding=\"async\" width=\"800\" height=\"500\" hidden><img id=\"observer-next\" alt=\"Aktueller Modellstand der beobachteten MCP-Sitzung\" decoding=\"async\" width=\"800\" height=\"500\" hidden><figcaption id=\"observer-placeholder\" class=\"observerplaceholder\">Das erste Modellbild wird vorbereitet.</figcaption></figure><div id=\"observer-meta\" class=\"observermeta\"></div><div id=\"observer-warning\" class=\"observernote\" role=\"note\"></div><p id=\"observer-note\" class=\"observernote\">Nur ansehen · Echte Bilder des Sitzungsmodells, auch vor dem Speichern. Neue Bilder erscheinen nach abgeschlossenen Befehlen oder Stapeln; während einer Berechnung bleibt der vorherige Stand sichtbar. Die Ansicht verändert weder Modell noch Kamera der KI.</p></div></dialog></main><script type=\"module\" src=\"/workspace/boot.js\"></script></body></html>");
    html
}

fn detail_value(html: &mut String, label: &str, value: &str) {
    if !value.is_empty() {
        let _ = write!(html, "<dt>{}</dt><dd>{}</dd>", escape(label), escape(value));
    }
}

fn date(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&chrono::Utc).format("%d.%m.%Y, %H:%M UTC").to_string())
        .unwrap_or_else(|_| "Nicht verfügbar".into())
}

/// Show only metadata selected by the confined reader; never embed a raw design.
/// The same section is the popup content and a complete no-Wasm fallback page.
pub fn details(file: &Value) -> String {
    let mut html = start("Dateidetails", None);
    html.push_str(
        "<div class=\"panel detailpanel\"><section id=\"file-details\" class=\"detailsbody\"><div class=\"eyebrow\">Gespeicherte Datei</div>",
    );
    let name = text(file, "name");
    let _ = write!(html, "<h1>{}</h1><p class=\"detailspath\">{}</p>", escape(name.rsplit('/').next().unwrap_or(name)), escape(name));
    if is_png(name) {
        let _ = write!(
            html,
            "<figure class=\"detailpreview\"><img src=\"{}\" loading=\"lazy\" decoding=\"async\" alt=\"Gespeicherte PNG-Datei\" data-file-preview><figcaption>Gespeicherte Bildvorschau</figcaption></figure>",
            preview_url(name)
        );
    }
    html.push_str("<dl class=\"detailgrid\">");
    detail_value(&mut html, "Dateiformat", text(file, "format"));
    detail_value(&mut html, "Dateigröße", &size(number(file, "size_bytes")));
    detail_value(&mut html, "Erstellt", &date(text(file, "created_at")));
    detail_value(&mut html, "Zuletzt gespeichert", &date(text(file, "modified_at")));
    if let Some(project) = file.get("project") {
        detail_value(&mut html, "Projektgruppe", if text(project, "basis") == "ungrouped" { "Weitere Dateien" } else { text(project, "name") });
        detail_value(
            &mut html,
            "Zuordnung",
            match text(project, "basis") {
                "directory" => "Projektordner im Serverspeicher",
                "design_stem" | "file_stem" => "Virtuelle Gruppe aus Dateinamen",
                _ => "Ohne eindeutige Projektzuordnung",
            },
        );
    }
    html.push_str("</dl>");
    if file.get("project").is_some() {
        html.push_str("<p class=\"detailnote\">Die Projektgruppe wird aus dem Speicherpfad oder Dateinamen ermittelt. Sie ist keine Modelleigenschaft und verändert die Datei nicht.</p>");
    }
    html.push_str("<h2 class=\"detailheading\">Modelleigenschaften</h2>");
    let metadata = file.get("metadata").unwrap_or(&Value::Null);
    match text(metadata, "status") {
        "available" => {
            html.push_str("<p class=\"detailnote\">Eigenschaften aus dem gespeicherten Stand. Änderungen in aktiven Sitzungen erscheinen hier nach dem Speichern.</p><dl class=\"detailgrid\">");
            for (key, label) in [("title", "Titel"), ("project", "Projekt"), ("part_number", "Teilenummer"), ("revision", "Revision"), ("author", "Autor"), ("organization", "Organisation"), ("description", "Beschreibung"), ("license", "Lizenz"), ("format", "Designformat"), ("units", "Einheit")] {
                detail_value(&mut html, label, text(metadata, key));
            }
            for (key, label) in [("feature_count", "Features"), ("sketch_count", "Skizzen"), ("parameter_count", "Parameter"), ("component_count", "Komponenten"), ("occurrence_count", "Instanzen")] {
                if let Some(value) = metadata.get(key).and_then(Value::as_u64) {
                    detail_value(&mut html, label, &value.to_string());
                }
            }
            html.push_str("</dl>");
        }
        "not_applicable" => html.push_str("<p class=\"detailnote\">Für dieses Dateiformat sind keine SolidCraft-Modelleigenschaften verfügbar.</p>"),
        _ => html.push_str("<p class=\"detailnote\">Modelleigenschaften konnten für diesen gespeicherten Stand nicht gelesen werden. Die Datei bleibt zum Herunterladen verfügbar.</p>"),
    }
    html.push_str("<div class=\"actions\">");
    edit_link(&mut html, file, "primary", "In Browser-CAD bearbeiten ↗");
    let _ = write!(
        html,
        "<a class=\"secondary\" href=\"{}\" download>Datei herunterladen ↓</a><a class=\"secondary detailfallback\" href=\"/workspace\">Zurück zum Arbeitsbereich</a></div></section></div></main></body></html>",
        download_url(name)
    );
    html
}

/// Render an authenticated snapshot. The caller applies query/type filtering.
pub fn fragment(snapshot: &Value, query: &str, kind: &str) -> String {
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
    let mut html = String::new();
    let _ = write!(
        html,
        "<section class=\"cards\" aria-label=\"Übersicht\"><div class=\"stat\"><div class=\"statlabel\">{}</div><div class=\"statnumber\">{}</div><div class=\"statnote\">{models} Modelle · {exports} Exporte{}</div></div><div class=\"stat\"><div class=\"statlabel\"><span class=\"dot amber\" aria-hidden=\"true\"></span>Dateien in Bearbeitung</div><div class=\"statnumber\">{dirty_files}</div><div class=\"statnote\">{} mit bestätigten Änderungen</div></div><div class=\"stat\"><div class=\"statlabel\"><span class=\"dot\" aria-hidden=\"true\"></span>MCP-Sitzungen</div><div class=\"statnumber\">{used} <span class=\"statdenom\">/ {total}</span></div><div class=\"statnote\">{busy} beschäftigt · {} bereit</div></div><div class=\"stat\"><div class=\"statlabel\"><span class=\"dot blue\" aria-hidden=\"true\"></span>Neue Entwürfe</div><div class=\"statnumber\">{unsaved}</div><div class=\"statnote\">In Sitzungen · noch nie gespeichert</div></div></section>",
        if filtered { "Angezeigte Dateien" } else { "Gespeicherte Dateien" },
        files.len(),
        if files.len() > models + exports { " · weitere Dateien" } else { "" },
        if filtered { "Angezeigte Dateien" } else { "Gespeicherte Dateien" },
        sessions.len().saturating_sub(busy)
    );
    let _ = write!(
        html,
        "<div class=\"work\"><section class=\"panel file-explorer\" aria-labelledby=\"files-heading\"><div class=\"panelhead\"><div><h2 id=\"files-heading\">Projekte &amp; Dateien <span class=\"counter\">{}</span></h2><div class=\"sub\">Im gemeinsamen Serverspeicher</div></div></div><p class=\"filesintro\">Projektordner bündeln ihre Dateien und Unterordner. Dateien im Hauptordner werden nach passenden Modell-/Exportnamen virtuell gruppiert. ZIP-Downloads lassen Sicherungs-, temporäre und versteckte Dateien aus; Suchfilter schränken das Archiv nicht ein.</p>",
        files.len()
    );
    let directories = entries(snapshot, "directories")
        .iter()
        .filter(|directory| {
            !filtered || directory.as_str().is_some_and(|name| files.iter().any(|file| text(file, "name").starts_with(&format!("{name}/"))))
        })
        .cloned()
        .collect::<Vec<_>>();
    project_files(&mut html, files, &directories);
    if files.is_empty() {
        html.push_str(if filtered {
            "<div class=\"empty\"><strong>Keine passenden Dateien</strong>Ändere den Suchbegriff oder den Dateityp. <a href=\"/workspace\">Alle Dateien anzeigen</a></div>"
        } else {
            "<div class=\"empty\"><strong>Noch keine gespeicherten Dateien</strong>Speichert ein MCP-Client ein Modell oder einen Export im gemeinsamen Serverspeicher, erscheint die Datei hier automatisch.</div>"
        });
    }
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
        html.push_str("<div class=\"empty\"><strong>Alle Sitzungen frei</strong>Sobald sich ein MCP-Client verbindet, erscheint seine Sitzung hier automatisch.</div>");
    }
    html.push_str("</div><div class=\"unsaved\">Ungespeicherte Änderungen gehen beim Sitzungsende oder Container-Neustart verloren.</div></aside></div><div class=\"bottom\"><section class=\"panel explain\"><h3>Was bedeutet „in Bearbeitung“?</h3><p>Eine Sitzung enthält bestätigte Änderungen, die noch nicht in einer Datei gespeichert sind. „Geöffnet“ bedeutet nur, dass eine Sitzung eine Datei geladen hat. „Führt aus“ beschreibt eine gerade laufende Anfrage. Ein unbekannter Änderungsstatus zählt nicht als bestätigte Bearbeitung.</p><div class=\"legend\"><span><span class=\"dot amber\" aria-hidden=\"true\"></span> Ungespeichert / führt aus</span><span><span class=\"dot blue\" aria-hidden=\"true\"></span> Neuer Entwurf</span><span><span class=\"dot\" aria-hidden=\"true\"></span> Sitzungsplatz belegt</span></div></section><section class=\"panel explain\"><h3>Browser-CAD bleibt unabhängig</h3><p>Diese Übersicht zeigt Dateien und Sitzungen des MCP-Dienstes. Deine Browsermodelle leben weiterhin im Browser. „Bearbeiten“ öffnet eine Kopie des gespeicherten Modells in einem neuen CAD-Tab. Alternativ kannst du Dateien herunterladen und selbst importieren.</p><div class=\"small\">Dateien nur mit Bestätigung löschen · Sitzungen werden hier nicht beendet</div></section></div><footer><span>SolidCraft · Inoffizielle Unraid-Integration</span><span>Gespeicherte Dateistände · Zeiten in UTC</span></footer>");
    html
}

fn delete_link(html: &mut String, name: &str) {
    let url = download_url(name).replacen("/workspace/files/", "/workspace/delete/", 1);
    let _ = write!(html, "<a class=\"action danger\" href=\"{url}\" data-file-details=\"{url}\">Löschen …</a>");
}

fn folder_actions(html: &mut String, name: &str) {
    let url = download_url(name).replacen("/workspace/files/", "/workspace/archive/", 1);
    // The server attaches successful archives. Forcing `download` here also saves
    // HTML errors (expired login, origin rejection, manifest conflicts) as .htm.
    // Keep errors visible in a separate tab without losing the live workspace.
    let _ = write!(html, "<a class=\"action\" href=\"{url}\" target=\"_blank\" rel=\"noopener\">Als ZIP herunterladen ↓</a>");
    delete_link(html, name);
}

pub(crate) fn action_failure(message: &str) -> String {
    let mut html = start("Dateiaktion nicht möglich", None);
    let _ = write!(
        html,
        "<section id=\"file-details\" class=\"panel detailsbody detailpanel\"><h1>Dateiaktion nicht möglich</h1><p class=\"error\" role=\"alert\">{}</p><a class=\"secondary\" href=\"/workspace\">Zurück zum Arbeitsbereich</a></section></main></body></html>",
        escape(message)
    );
    html
}

pub(crate) fn delete_confirmation(path: &str, csrf: &str, ticket: &str, manifest: &crate::workspace_actions::manifest::Manifest) -> String {
    let mut html = start("Löschen bestätigen", Some(csrf));
    let folder = manifest.entries.first().is_some_and(|entry| entry.directory);
    let label = if folder { "Ordner" } else { "Datei" };
    let object_label = if folder { "diesen Ordner" } else { "diese Datei" };
    let _ = write!(
        html,
        "<div class=\"panel detailpanel\"><section id=\"file-details\" class=\"detailsbody\"><div class=\"eyebrow\">Löschen bestätigen</div><h1>{label} wirklich löschen?</h1><p class=\"detailspath\">{}</p><div class=\"deletewarning\">Dieser Vorgang entfernt den bestätigten Inhalt endgültig aus dem Serverspeicher. Es gibt keinen Papierkorb.</div><dl class=\"detailgrid\"><dt>Dateien</dt><dd>{}</dd><dt>Unterordner</dt><dd>{}</dd><dt>Gesamtgröße</dt><dd>{}</dd></dl><p class=\"detailnote\">Es wird der vollständige echte Pfad geprüft, unabhängig von Suchfiltern. Ändert sich der Inhalt oder nutzt eine MCP-Sitzung das Projekt, wird das Löschen abgelehnt. Die Bestätigung gilt fünf Minuten.</p><form class=\"deleteform\" method=\"post\" action=\"/workspace/delete\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"ticket\" value=\"{}\"><input type=\"hidden\" name=\"path\" value=\"{}\"><label><input type=\"checkbox\" name=\"confirm\" value=\"delete\" required> Ich möchte {object_label} mit dem angezeigten Inhalt endgültig löschen.</label><button class=\"primary\" type=\"submit\">Endgültig löschen</button><a class=\"secondary\" href=\"/workspace\">Abbrechen</a></form><a class=\"detailfallback\" href=\"/workspace\">Zurück zum Arbeitsbereich</a></section></div></main></body></html>",
        escape(path),
        manifest.files,
        manifest.folders.saturating_sub(usize::from(folder)),
        size(manifest.bytes),
        escape(csrf),
        escape(ticket),
        escape(path)
    );
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_real_folders_have_actions_but_legacy_virtual_groups_never_do() {
        let snapshot = serde_json::json!({"directories":["Demo","Demo/empty"],"files":[{"name":"part.step","kind":"export","project":{"id":"stem:part","name":"part","basis":"file_stem"}}],"sessions":[]});
        let html = page(&snapshot, "", "");
        assert!(html.contains("/workspace/archive/Demo/empty"));
        assert!(html.contains("href=\"/workspace/archive/Demo\" target=\"_blank\" rel=\"noopener\""));
        assert!(!html.contains("href=\"/workspace/archive/Demo\" download"));
        assert!(html.contains("/workspace/delete/Demo"));
        assert!(html.contains("Virtuelle Gruppe aus Dateinamen"));
        assert!(!html.contains("/workspace/archive/part"));
        assert!(!html.contains("/workspace/delete/part\""));
        assert!(!html.contains("<details open"));
        let real_project = html.split("data-project=\"directory:Demo\"").nth(1).unwrap();
        let summary_end = real_project.find("</summary>").unwrap();
        let actions = real_project.find("class=\"folderactions\"").unwrap();
        let columns = real_project.find("<thead>").unwrap();
        assert!(summary_end < actions && actions < columns);
        assert!(real_project.contains("aria-label=\"Ordneraktionen für Demo/empty\""));
        assert!(real_project.contains("<span>empty</span>"));
        assert!(!html.contains("Echter Ordner"));
        assert!(!real_project.contains("colspan=\"5\""));
    }

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
        assert_eq!(html.matches("<script ").count(), 1);
        assert!(html.contains("src=\"/workspace/boot.js\""));
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
        for duration in ["session", "7d", "30d", "forever"] {
            assert!(html.contains(&format!("value=\"{duration}\"")));
        }
    }

    #[test]
    fn projects_use_original_icons_and_keep_state_outside_live_fragment() {
        let snapshot = json!({"files":[
            {"name":"part.solidcraft","kind":"model","format":"SolidCraft","project":{"id":"stem:part","name":"part","basis":"design_stem"}},
            {"name":"part.step","kind":"export","format":"STEP","project":{"id":"stem:part","name":"part","basis":"design_stem"}}
        ]});
        let fragment = fragment(&snapshot, "part", "all");
        assert_eq!(fragment.matches("data-project=\"stem:part\"").count(), 1);
        assert!(fragment.contains("<details class=\"project\" data-project="));
        assert!(!fragment.contains("<details class=\"project\" open"));
        assert!(fragment.contains("2 Dateien"));
        assert!(fragment.contains("<title>Folder</title>"));
        assert!(fragment.contains("<title>Box</title>"));
        assert!(fragment.contains("<title>Export</title>"));
        assert!(!fragment.contains("<form"));
        assert!(!fragment.contains("<dialog"));
        assert!(!fragment.contains("<script"));
        let page = page(&snapshot, "part", "all");
        assert!(page.contains("id=\"file-dialog\""));
        assert!(page.contains("id=\"workspace-status\""));
        assert!(page.contains("data-file-details=\"/workspace/details/part.solidcraft\""));
    }

    #[test]
    fn derived_group_and_saved_project_are_distinct_in_file_details() {
        let html = details(&json!({
            "name":"fixture/part.solidcraft",
            "project":{"id":"directory:fixture","name":"fixture","basis":"directory"},
            "metadata":{"status":"available","project":"Saved model property"}
        }));
        assert!(html.contains("<dt>Projektgruppe</dt><dd>fixture</dd>"));
        assert!(html.contains("<dt>Zuordnung</dt><dd>Projektordner im Serverspeicher</dd>"));
        assert!(html.contains("<dt>Projekt</dt><dd>Saved model property</dd>"));
        assert!(html.contains("Sie ist keine Modelleigenschaft"));
    }

    #[test]
    fn sessions_show_the_server_selected_project_without_using_model_metadata() {
        let mut html = String::new();
        session_row(&mut html, &json!({"project":"Fixture & Tools","metadata":{"project":"Unrelated saved property"}}));
        assert!(html.contains("Projekt: Fixture &amp; Tools"));
        assert!(!html.contains("Unrelated saved property"));
        assert!(!html.contains("Projekt noch nicht gewählt"));
        let mut unselected = String::new();
        session_row(&mut unselected, &json!({"project":null}));
        assert!(unselected.contains("Projekt noch nicht gewählt"));
    }

    #[test]
    fn observer_buttons_expose_only_a_valid_opaque_handle() {
        let handle = "b84ca986-1593-4acc-855c-7db6287b2871";
        let mut row = String::new();
        session_row(&mut row, &json!({"observer_handle":handle,"display_id":"s-display","transport_id":"secret-transport"}));
        assert!(row.contains(&format!("data-observe=\"{handle}\"")));
        assert!(row.contains("aria-haspopup=\"dialog\" aria-controls=\"observer-dialog\""));
        assert!(!row.contains("secret-transport"));
        for invalid in ["", "------------------------------------", "not-a-handle", "\"><script>bad</script>"] {
            let mut row = String::new();
            session_row(&mut row, &json!({"observer_handle":invalid}));
            assert!(!row.contains("data-observe="));
        }
    }

    #[test]
    fn observer_modal_survives_workspace_fragment_replacement() {
        let snapshot = json!({"sessions":[{"observer_handle":"b84ca986-1593-4acc-855c-7db6287b2871"}]});
        let live = fragment(&snapshot, "", "all");
        assert!(live.contains("data-observe="));
        assert!(!live.contains("<dialog"));
        let html = page(&snapshot, "", "all");
        assert_eq!(html.matches("id=\"observer-dialog\"").count(), 1);
        let modal = html.find("<dialog id=\"observer-dialog\"").expect("observer dialog");
        assert!(html[..modal].ends_with("</div></dialog>"));
        assert!(html[modal..].contains("aria-labelledby=\"observer-title\" aria-describedby=\"observer-note\""));
        assert!(html[modal..].contains("id=\"observer-close\" class=\"dialogclose\" type=\"button\" autofocus"));
        assert!(html[modal..].contains("role=\"status\" aria-live=\"polite\""));
        assert_eq!(html[modal..].matches("<img ").count(), 2);
        assert!(!html[modal..].contains("src=\"/workspace/observe/"));
        assert!(html[modal..].contains("id=\"observer-warning\""));
        assert!(html[modal..].contains("auch vor dem Speichern"));
        assert!(html[modal..].contains("verändert weder Modell noch Kamera"));
    }

    #[test]
    fn saved_properties_escape_content_and_ignore_unknown_metadata() {
        let file = json!({"name":"parts/teil #1.solidcraft","format":"SolidCraft","size_bytes":42,
            "metadata":{"status":"available","title":"<script>unsafe</script>","author":"A & B","description":"Line one\nLine two","feature_count":3,"secret":"private-value"}});
        let html = details(&file);
        assert!(html.contains("&lt;script&gt;unsafe&lt;/script&gt;"));
        assert!(html.contains("A &amp; B"));
        assert!(html.contains("Line one\nLine two"));
        assert!(html.contains("<dt>Features</dt><dd>3</dd>"));
        assert!(html.contains("id=\"file-details\""));
        assert!(html.contains("/workspace/files/parts/teil%20%231.solidcraft"));
        assert!(!html.contains("private-value"));
        assert!(!html.contains("<script"));
        assert!(details(&json!({"metadata":{"status":"unavailable"}})).contains("nicht gelesen"));
        assert!(details(&json!({"metadata":{"status":"not_applicable"}})).contains("keine SolidCraft-Modelleigenschaften"));
    }

    #[test]
    fn png_thumbnails_use_only_encoded_cookie_authenticated_preview_routes() {
        let name = "renders/öffentlich #1 &\".PNG";
        let mut row = String::new();
        file_row(&mut row, &json!({"name":name,"kind":"export","format":"PNG"}));
        assert!(row.contains("src=\"/workspace/preview/renders/%C3%B6ffentlich%20%231%20%26%22.PNG\""));
        assert!(row.contains("loading=\"lazy\" decoding=\"async\" width=\"48\" height=\"40\""));
        assert!(row.contains("class=\"fileicon filepreview\""));
        assert!(row.contains("data-file-preview"));
        assert!(row.contains("<title>Export</title>"));
        assert!(row.contains("öffentlich #1 &amp;&quot;.PNG"));
        assert!(!row.contains("data:image"));
        assert!(!row.contains("src=\"/files/"));
        assert!(!row.contains("token="));
        let popup = details(&json!({"name":name,"kind":"export","format":"PNG","metadata":{"status":"not_applicable"}}));
        assert!(popup.contains("class=\"detailpreview\""));
        assert!(popup.contains("src=\"/workspace/preview/"));
        let mut other = String::new();
        file_row(&mut other, &json!({"name":"part.step","kind":"export"}));
        assert!(!other.contains("<img"));
        assert!(!is_png("png"));
    }

    #[test]
    fn only_native_models_offer_an_independent_browser_edit_action() {
        let native = json!({"name":"projects/teil #1.solidcraft","kind":"model"});
        let mut row = String::new();
        file_row(&mut row, &native);
        assert!(row.contains("href=\"/workspace/open/projects/teil%20%231.solidcraft\" target=\"_blank\" rel=\"noopener\""));
        assert!(row.contains(">Bearbeiten ↗</a>"));
        assert!(details(&native).contains("In Browser-CAD bearbeiten ↗"));
        for file in [json!({"name":"part.step","kind":"export"}), json!({"name":"not-native.solidcraft","kind":"other"})] {
            let mut row = String::new();
            file_row(&mut row, &file);
            assert!(!row.contains("/workspace/open/"));
            assert!(!details(&file).contains("/workspace/open/"));
        }
        let dashboard = fragment(&json!({}), "", "all");
        assert!(dashboard.contains("class=\"panel file-explorer\""));
        assert!(dashboard.contains("aria-labelledby=\"files-heading\""));
    }
}
