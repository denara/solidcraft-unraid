//! Private remembered browser sessions. Only hashed cookie identifiers are persisted.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{AuthStore, BrowserSession};
use crate::config::Config;

const MAX_BYTES: u64 = 64 * 1024;
const FOREVER_COOKIE: u64 = 400 * 86400;

#[derive(Clone, Copy)]
pub(super) enum Expiry {
    At(u64),
    Forever,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

impl Expiry {
    pub(super) fn cookie_seconds(self) -> u64 {
        match self {
            Self::At(deadline) => deadline.saturating_sub(now()).min(30 * 86400),
            Self::Forever => FOREVER_COOKIE,
        }
    }
}

pub(super) fn choice(value: Option<&str>) -> Result<Option<Expiry>, ()> {
    match value.unwrap_or("session") {
        "session" => Ok(None),
        "7d" => Ok(Some(Expiry::At(now().saturating_add(7 * 86400)))),
        "30d" => Ok(Some(Expiry::At(now().saturating_add(30 * 86400)))),
        "forever" => Ok(Some(Expiry::Forever)),
        _ => Err(()),
    }
}

pub(super) fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn load(config: &Config) -> AuthStore {
    let mut store = AuthStore::default();
    let read = || -> Option<Value> {
        let meta = fs::symlink_metadata(&config.browser_auth).ok()?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES {
            return None;
        }
        let mut bytes = Vec::new();
        fs::File::open(&config.browser_auth).ok()?.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > MAX_BYTES {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    };
    let Some(data) = read() else { return store };
    if data["version"] != 1 || data["token_binding"] != hash(&config.token) {
        return store;
    }
    let Some(rows) = data["sessions"].as_array().filter(|rows| rows.len() <= 64) else { return store };
    for row in rows {
        let (Some(key), Some(csrf)) = (row["key"].as_str().filter(|key| hex(key)), row["csrf"].as_str().filter(|value| hex(value))) else {
            continue;
        };
        let expiry = if row["expires"].is_null() {
            Expiry::Forever
        } else if let Some(time) = row["expires"].as_u64().filter(|time| *time > now() && *time <= now().saturating_add(30 * 86400)) {
            Expiry::At(time)
        } else {
            continue;
        };
        store.sessions.insert(
            key.to_owned(),
            BrowserSession {
                csrf: csrf.to_owned(),
                expires: Instant::now() + Duration::from_secs(expiry.cookie_seconds()),
                remembered: Some(expiry),
            },
        );
    }
    store
}

pub(super) fn save(config: &Config, store: &AuthStore) -> Result<(), ()> {
    // The store lives outside /data/files and is never exposed through file routes.
    if let Ok(meta) = fs::symlink_metadata(&config.browser_auth)
        && (!meta.is_file() || meta.file_type().is_symlink())
    {
        return Err(());
    }
    let rows: Vec<_> = store
        .sessions
        .iter()
        .filter_map(|(key, session)| {
            let expiry = session.remembered?;
            if session.expires <= Instant::now() {
                return None;
            }
            Some(json!({"key":key,"csrf":session.csrf,"expires":match expiry {Expiry::At(time) => json!(time), Expiry::Forever => Value::Null}}))
        })
        .collect();
    let bytes = serde_json::to_vec(&json!({"version":1,"token_binding":hash(&config.token),"sessions":rows})).map_err(|_| ())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(());
    }
    let parent = config.browser_auth.parent().ok_or(())?;
    let temp = parent.join(format!(".workspace-auth-{}.tmp", uuid::Uuid::new_v4()));
    let write = || -> Result<(), ()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|_| ())?;
        file.write_all(&bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        fs::rename(&temp, &config.browser_auth).map_err(|_| ())?;
        #[cfg(unix)]
        fs::File::open(parent).and_then(|directory| directory.sync_all()).map_err(|_| ())?;
        Ok(())
    };
    let result = write();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
