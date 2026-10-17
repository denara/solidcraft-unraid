use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

pub enum TokenSource {
    File(PathBuf),
    Environment,
}

impl TokenSource {
    pub fn location(&self) -> serde_json::Value {
        match self {
            Self::File(path) => serde_json::json!({"source":"file","path":path.to_string_lossy()}),
            Self::Environment => serde_json::json!({"source":"environment","environment_variable":"SOLVECRAFT_MCP_TOKEN"}),
        }
    }
}

pub struct Config {
    pub listen: String,
    pub cli: String,
    pub web: PathBuf,
    pub files: PathBuf,
    pub token: String,
    pub token_source: TokenSource,
    pub log_color: bool,
    pub origins: Vec<String>,
    pub max_sessions: usize,
    pub idle: Duration,
    pub timeout: Duration,
}

fn env(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.into())
}

fn number(name: &str, fallback: u64, min: u64, max: u64) -> Result<u64, String> {
    let value = match std::env::var(name) {
        Ok(v) => v.parse::<u64>().map_err(|_| format!("{name}: expected an integer"))?,
        Err(_) => fallback,
    };
    if !(min..=max).contains(&value) {
        return Err(format!("{name}: expected {min}..={max}"));
    }
    Ok(value)
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let data = PathBuf::from(env("SOLVECRAFT_DATA_DIR", "/data"));
        fs::create_dir_all(data.join("files")).map_err(|e| format!("data directory: {e}"))?;
        let (token, token_source) = match std::env::var("SOLVECRAFT_MCP_TOKEN") {
            Ok(t) if !t.is_empty() => (t, TokenSource::Environment),
            _ => {
                let path = data.join("mcp-token");
                (token_file(&path)?, TokenSource::File(path))
            }
        };
        if !(32..=512).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("MCP token must contain 32..512 visible ASCII characters, without spaces".into());
        }
        let origins: Vec<String> = env("SOLVECRAFT_ALLOWED_ORIGINS", "http://localhost:8781,http://127.0.0.1:8781")
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect();
        if origins.is_empty() || origins.iter().any(|o| o == "*" || !(o.starts_with("http://") || o.starts_with("https://"))) {
            return Err("SOLVECRAFT_ALLOWED_ORIGINS needs explicit http(s) origins, separated by commas".into());
        }
        Ok(Self {
            listen: env("SOLVECRAFT_LISTEN", "0.0.0.0:8080"),
            cli: env("SOLVECRAFT_CLI", "solidcraft-cli"),
            web: PathBuf::from(env("SOLVECRAFT_WEB_DIR", "/opt/solvecraft/web")),
            files: fs::canonicalize(data.join("files")).map_err(|e| e.to_string())?,
            token,
            token_source,
            log_color: crate::audit::color_enabled(
                std::env::var("SOLVECRAFT_LOG_COLOR").ok().as_deref(),
                std::env::var("NO_COLOR").ok().as_deref(),
                std::io::stderr().is_terminal(),
            )?,
            origins,
            max_sessions: number("SOLVECRAFT_MAX_SESSIONS", 4, 1, 32)? as usize,
            idle: Duration::from_secs(number("SOLVECRAFT_IDLE_SECONDS", 1800, 60, 86400)?),
            timeout: Duration::from_secs(number("SOLVECRAFT_TOOL_TIMEOUT", 300, 10, 1800)?),
        })
    }
}

fn token_file(path: &PathBuf) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(t) => return Ok(t.trim().to_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("token file: {e}")),
    }
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| format!("create token file: {e}"))?;
    writeln!(file, "{token}").map_err(|e| e.to_string())?;
    eprintln!("MCP token created in {}; use its contents in your client's Authorization header", path.display());
    Ok(token)
}
