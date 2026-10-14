//! Where MCP tool calls end up: a control-channel method call (`docs/control-protocol.md`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{Value, json};
use solidcraft_engine::Session;
use solidcraft_engine::render::{StandardView, render_png};

/// Something that answers control-channel methods: `engine.execute`, `engine.commands`,
/// `document.inspect`, `ui.view`, `ui.render`, `ui.screenshot`.
pub trait Backend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    /// True when a real window is attached (`ui.screenshot` can capture it).
    fn has_ui(&self) -> bool;
    fn describe(&self) -> String;
    /// The open design's file, when this side knows it (its folder joins the agent's folders,
    /// see `vfs::PathScope::with_design`). A running app adds its own, so `Remote` says nothing.
    fn design_path(&self) -> Option<String> {
        None
    }
}

/// Longest reply line accepted from the app.
const MAX_REPLY: u64 = 64 << 20;

/// How long a reply may take: kernel operations (booleans, fillets) on big parts can take a while.
const READ_TIMEOUT: Duration = Duration::from_secs(300);

/// How long the app may take to run an `engine.batch` (its control server waits as long).
const BATCH_TIMEOUT: Duration = Duration::from_secs(3600);

/// A running SolidCraft app reached through its loopback control port
/// (`solidcraft --control PORT`).
pub struct Remote {
    addr: String,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    next_id: u64,
    /// The app's control token, sent with every request (docs/control-protocol.md).
    token: Option<String>,
}

impl Remote {
    /// Connect to the app at `addr`, with its control token found the usual way
    /// (`SOLIDCRAFT_CONTROL_TOKEN`, else the app's token file for that port).
    pub fn connect(addr: &str) -> std::io::Result<Self> {
        Self::connect_with_token(addr, None)
    }

    /// [`Remote::connect`] with the token given (`--token`); `None` finds it the usual way.
    pub fn connect_with_token(addr: &str, token: Option<&str>) -> std::io::Result<Self> {
        let port = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()).unwrap_or(0);
        let token = solidcraft_engine::control_auth::client_token(port, token);
        let mut r = Self { addr: addr.to_string(), conn: None, next_id: 1, token };
        r.reconnect()?;
        Ok(r)
    }

    fn reconnect(&mut self) -> std::io::Result<()> {
        self.conn = None;
        let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, format!("cannot resolve {}", self.addr));
        for sa in self.addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sa, Duration::from_millis(800)) {
                Ok(s) => {
                    s.set_nodelay(true).ok();
                    s.set_read_timeout(Some(READ_TIMEOUT)).ok();
                    let read = s.try_clone()?;
                    self.conn = Some((BufReader::new(read), s));
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn roundtrip(&mut self, line: &str) -> std::io::Result<String> {
        if self.conn.is_none() {
            self.reconnect()?;
        }
        let Some((reader, writer)) = self.conn.as_mut() else { return Err(std::io::Error::other("not connected")) };
        // One write: a request that loses its newline still reaches the app as a request.
        let sent = writer.write_all(format!("{line}\n").as_bytes()).and_then(|()| writer.flush());
        if let Err(e) = sent {
            self.conn = None;
            return Err(e);
        }
        let mut reply = String::new();
        let n = match reader.by_ref().take(MAX_REPLY).read_line(&mut reply) {
            Ok(n) => n,
            Err(e) => {
                // A timed-out read leaves a late reply on the stream: start afresh next time.
                self.conn = None;
                return Err(e);
            }
        };
        if n == 0 || !reply.ends_with('\n') {
            self.conn = None;
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "connection closed"));
        }
        Ok(reply)
    }

    fn set_read_timeout(&self, t: Duration) {
        if let Some((_, writer)) = &self.conn {
            writer.set_read_timeout(Some(t)).ok();
        }
    }

    /// `engine.batch` is never sent twice (#743): it goes on a fresh connection (an idle one may
    /// have been closed by an app restart; connecting again sends nothing), and once it is being
    /// written any error is final, as the app may have run it.
    fn send_batch(&mut self, line: &str) -> std::io::Result<String> {
        self.reconnect().or_else(|_| self.reconnect())?;
        self.set_read_timeout(BATCH_TIMEOUT);
        let r = self.roundtrip(line);
        self.set_read_timeout(READ_TIMEOUT);
        r
    }
}

impl Backend for Remote {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut params = params;
        // The app confines the files this request reaches to the agent's folders (control.rs).
        if let (Some(scope), Some(o)) = (solidcraft_engine::io::vfs::current_scope(), params.as_object_mut()) {
            o.insert("allow_paths".into(), json!(scope.roots().iter().map(|r| r.to_string_lossy()).collect::<Vec<_>>()));
        }
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(t) = &self.token {
            msg["token"] = json!(t);
        }
        let line = msg.to_string();
        let reply = if method == "engine.batch" {
            self.send_batch(&line).map_err(|e| format!("SolidCraft at {}: {e}", self.addr))?
        } else {
            match self.roundtrip(&line) {
                Ok(r) => r,
                // One retry on a fresh connection (the app may have restarted).
                Err(_) => self.roundtrip(&line).map_err(|e| format!("SolidCraft at {}: {e}", self.addr))?,
            }
        };
        let v: Value = serde_json::from_str(&reply).map_err(|e| format!("bad reply from the app: {e}"))?;
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(v.get("error").and_then(Value::as_str).unwrap_or("error").to_string())
        }
    }
    fn has_ui(&self) -> bool {
        true
    }
    fn describe(&self) -> String {
        format!("connected to the SolidCraft app at {} (the user watches the viewport)", self.addr)
    }
}

/// An in-process headless session (no window); renders with the CPU rasterizer.
pub struct Headless {
    pub session: Session,
    /// Orientation for renders; the camera is refitted to the model every time.
    pub view: StandardView,
}

impl Default for Headless {
    fn default() -> Self {
        Headless::new(Session::default())
    }
}

impl Headless {
    pub fn new(session: Session) -> Self {
        Headless { session, view: StandardView::Iso }
    }
}

fn dim(p: &Value, k: &str, default: f64) -> usize {
    p.get(k).and_then(Value::as_f64).filter(|x| x.is_finite()).unwrap_or(default).clamp(16.0, 4096.0) as usize
}

impl Backend for Headless {
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        let s = &mut self.session;
        let str_p = |k: &str| p.get(k).and_then(Value::as_str);
        match method {
            "engine.execute" | "command" => {
                let id = str_p("command").ok_or("missing `command`")?;
                let params = p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}));
                s.execute(id, &params).map_err(|e| e.to_string())
            }
            "engine.script" => s.run_script(&p).map(Value::from).map_err(|e| e.to_string()),
            "engine.commands" => s.execute("engine.commands", &json!({})).map_err(|e| e.to_string()),
            "document.inspect" => s.execute("document.inspect", &p).map_err(|e| e.to_string()),
            "ui.view" => {
                let v = str_p("view").unwrap_or("home");
                match v {
                    // Fit keeps the orientation, also of a camera `view.apply` chose.
                    "fit" => {
                        let b = solidcraft_engine::view::bounds(s);
                        if let Some(c) = s.camera.as_mut() {
                            c.fit(&b);
                        }
                    }
                    _ => {
                        self.view = StandardView::parse(v).ok_or_else(|| format!("unknown view `{v}`"))?;
                        s.camera = None;
                    }
                }
                Ok(json!({"view": self.view}))
            }
            "ui.render" => {
                let (w, h) = (dim(&p, "width", 1280.0), dim(&p, "height", 800.0));
                // The camera `view.apply` chose, else the standard view framing the model.
                let cam = s.camera.unwrap_or_else(|| {
                    let mut cam = solidcraft_engine::view::home_camera(s);
                    cam.set_view(self.view);
                    cam
                });
                let scene = solidcraft_engine::view::scene(s, &cam);
                let png = render_png(&scene, &cam, w, h).ok_or("render failed")?;
                let path = str_p("path").ok_or("missing `path`")?;
                solidcraft_engine::io::vfs::write(path, &png).map_err(|e| format!("{path}: {e}"))?;
                Ok(json!({"path": path, "bytes": png.len(), "width": w, "height": h}))
            }
            "ui.screenshot" => Err("no window in headless mode; use ui.render".into()),
            other => Err(format!("unknown method `{other}`")),
        }
    }
    fn has_ui(&self) -> bool {
        false
    }
    fn describe(&self) -> String {
        "headless in-process SolidCraft session".into()
    }
    fn design_path(&self) -> Option<String> {
        self.session.path.clone()
    }
}
