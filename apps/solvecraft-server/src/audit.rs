//! Operational events only: never accept request headers, bodies or error text here.
use std::io::Write;

use serde_json::Value;

/// Container logs are usually pipes, so default to color even without a TTY.
/// Explicit configuration wins over the conventional NO_COLOR opt-out.
pub fn color_enabled(mode: Option<&str>, no_color: Option<&str>, terminal: bool) -> Result<bool, String> {
    match mode {
        Some("always") => Ok(true),
        Some("never") => Ok(false),
        Some("auto") => Ok(terminal && no_color.is_none_or(str::is_empty)),
        None => Ok(no_color.is_none_or(str::is_empty)),
        Some(_) => Err("SOLVECRAFT_LOG_COLOR: expected always, never or auto".into()),
    }
}

#[derive(Clone, Copy)]
pub enum Event {
    Ready,
    TokenLocation,
    Stopping,
    AuthAccepted,
    AuthRejected,
    OriginRejected,
    Opened,
    Closing,
    Released,
    Capacity,
    StartFailed,
    Busy,
    TimedOut,
    CadFailed,
    ToolFailed,
    SessionRejected,
}

impl Event {
    fn label(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Ready => ("system", "🚀", "server.ready"),
            Self::TokenLocation => ("system", "🔑", "token.location"),
            Self::Stopping => ("system", "🛑", "server.stopping"),
            Self::AuthAccepted => ("login", "✅", "auth.accepted"),
            Self::AuthRejected => ("warn", "⛔", "auth.rejected"),
            Self::OriginRejected => ("warn", "⛔", "origin.rejected"),
            Self::Opened => ("system", "🔒", "session.opened"),
            Self::Closing => ("system", "⏳", "session.closing"),
            Self::Released => ("system", "🔓", "session.released"),
            Self::Capacity => ("warn", "⚠️", "session.capacity_reached"),
            Self::StartFailed => ("error", "❌", "cad.start_failed"),
            Self::Busy => ("warn", "⏳", "session.busy"),
            Self::TimedOut => ("warn", "⌛", "cad.timed_out"),
            Self::CadFailed => ("error", "❌", "cad.exchange_failed"),
            Self::ToolFailed => ("warn", "⚠️", "tool.failed"),
            Self::SessionRejected => ("warn", "⛔", "session.rejected"),
        }
    }
}

#[derive(Default)]
pub struct Audit {
    color: bool,
    #[cfg(test)]
    pub rows: std::sync::Mutex<Vec<String>>,
}

impl Audit {
    pub fn new(color: bool) -> Self {
        Self {
            color,
            #[cfg(test)]
            rows: Default::default(),
        }
    }

    fn display(&self, category: &str, row: &str) -> String {
        let code = match category {
            "system" => "36",
            "login" => "95",
            "warn" => "33",
            "error" => "31",
            _ => return row.to_owned(),
        };
        if self.color { format!("\x1b[{code}m{row}\x1b[0m") } else { row.to_owned() }
    }

    pub fn startup(&self, config: &crate::config::Config) {
        self.emit(Event::Ready, serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"slots_total":config.max_sessions}));
        self.emit(Event::TokenLocation, config.token_source.location());
    }

    pub fn emit(&self, event: Event, fields: Value) {
        let (category, icon, name) = event.label();
        let slots = fields
            .get("slots_used")
            .and_then(Value::as_u64)
            .zip(fields.get("slots_total").and_then(Value::as_u64))
            .map(|(used, total)| format!(" slots={used}/{total}"))
            .unwrap_or_default();
        let row = format!("{} [{category}] {icon} {name}{slots} {fields}", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        let _ = writeln!(std::io::stderr().lock(), "{}", self.display(category, &row));
        #[cfg(test)]
        if let Ok(mut rows) = self.rows.lock() {
            rows.push(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_default_opt_out_and_terminal_modes() {
        assert_eq!(color_enabled(None, None, false), Ok(true));
        assert_eq!(color_enabled(None, Some(""), false), Ok(true));
        assert_eq!(color_enabled(None, Some("1"), true), Ok(false));
        assert_eq!(color_enabled(Some("always"), Some("1"), false), Ok(true));
        assert_eq!(color_enabled(Some("never"), None, true), Ok(false));
        assert_eq!(color_enabled(Some("auto"), None, false), Ok(false));
        assert_eq!(color_enabled(Some("auto"), None, true), Ok(true));
        assert_eq!(color_enabled(Some("auto"), Some("1"), true), Ok(false));
        assert!(color_enabled(Some("unexpected"), None, false).is_err());
    }

    #[test]
    fn category_palette_wraps_and_resets_each_line_without_changing_fields() {
        let colored = Audit::new(true);
        let plain = Audit::new(false);
        for (category, code) in [("system", "36"), ("login", "95"), ("warn", "33"), ("error", "31")] {
            let row = format!("2026-01-01T00:00:00.000Z [{category}] event {{\"slots_used\":1}}");
            assert_eq!(colored.display(category, &row), format!("\x1b[{code}m{row}\x1b[0m"));
            assert_eq!(plain.display(category, &row), row);
        }
        assert_eq!(colored.display("unknown", "plain"), "plain");
    }
}
