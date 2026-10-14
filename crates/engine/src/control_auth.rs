//! The control channel's per-launch token (docs/control-protocol.md). `solidcraft --control PORT`
//! makes a random token, writes it to a file only the user can read
//! ([`token_path`]) and refuses every connection whose first request doesn't carry it, so other
//! users on the same machine (loopback is shared, on Windows across remote-desktop sessions too)
//! can't drive the app. Clients find it with [`client_token`].

use std::io::{Error, ErrorKind, Result, Write};
use std::path::{Path, PathBuf};

/// Shortest token accepted from `SOLIDCRAFT_CONTROL_TOKEN` (hex characters).
pub const MIN_TOKEN_LEN: usize = 32;

/// The token file of the app listening on `port`: `<data folder>/control/<port>.token`
/// (`SOLIDCRAFT_CONTROL_DIR` replaces the folder).
pub fn token_path(port: u16) -> Option<PathBuf> {
    let dir = match solidcraft_geom::env::var_os("CONTROL_DIR").filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => crate::recovery::data_dir()?.join("control"),
    };
    Some(dir.join(format!("{port}.token")))
}

/// A token set on purpose with `SOLIDCRAFT_CONTROL_TOKEN` (test harnesses, sandboxed packages
/// whose data folder the client can't see), if it is long enough.
pub fn env_token() -> Option<String> {
    solidcraft_geom::env::var("CONTROL_TOKEN").map(|t| t.trim().to_string()).filter(|t| t.len() >= MIN_TOKEN_LEN)
}

/// The token a client sends to the app on `port`: `explicit` (`--token`), else
/// `SOLIDCRAFT_CONTROL_TOKEN`, else the app's token file.
pub fn client_token(port: u16, explicit: Option<&str>) -> Option<String> {
    if let Some(t) = explicit.map(str::trim).filter(|t| !t.is_empty()) {
        return Some(t.to_string());
    }
    if let Some(t) = env_token() {
        return Some(t);
    }
    // Read directly, not through `vfs`: the app's own file, outside any agent's folders.
    let text = std::fs::read_to_string(token_path(port)?).ok().filter(|t| t.len() <= 4096)?;
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

/// `a == b`, taking the same time wherever they differ.
pub fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Write `token` to `path`, readable and writable only by the user: the folder is made `0700`
/// and the file `0600` on Unix; on Windows the file's inherited permissions are replaced by one
/// entry for the user (`icacls`) before the token goes in. A file left by an earlier launch is
/// replaced.
pub fn write_token_file(path: &Path, token: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::new(ErrorKind::InvalidInput, "no folder"))?;
    std::fs::create_dir_all(dir)?;
    let _ = std::fs::remove_file(path);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    #[cfg(windows)]
    restrict_to_user(path)?;
    f.write_all(token.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()
}

/// Windows: drop inherited entries and grant only the current user full access.
#[cfg(windows)]
fn restrict_to_user(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let user = std::env::var("USERNAME").map_err(|_| Error::other("USERNAME is not set"))?;
    let domain = std::env::var("USERDOMAIN").ok().filter(|d| !d.is_empty());
    let who = domain.map_or(user.clone(), |d| format!("{d}\\{user}"));
    let status = std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &format!("{who}:F")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000)
        .status()?;
    if status.success() { Ok(()) } else { Err(Error::other(format!("icacls failed to restrict {}", path.display()))) }
}

/// Removes the token file when the app ends.
pub struct TokenFile(pub PathBuf);

impl Drop for TokenFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_whole() {
        assert!(tokens_match("abc123", "abc123"));
        assert!(!tokens_match("abc123", "abc124"));
        assert!(!tokens_match("abc123", "abc12"));
        assert!(!tokens_match("", "a"));
    }

    /// The token file is the user's alone, and a client reads it back.
    #[cfg(unix)]
    #[test]
    fn the_token_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("solidcraft-control-auth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("control/7878.token");
        std::fs::create_dir_all(dir.join("control")).unwrap();
        std::fs::write(&path, "stale").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_token_file(&path, "0123456789abcdef0123456789abcdef").unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), "0123456789abcdef0123456789abcdef");
        drop(TokenFile(path.clone()));
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
