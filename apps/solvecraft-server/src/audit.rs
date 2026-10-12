//! Operational events only: never accept request headers, bodies or error text here.
use std::io::Write;

use serde_json::Value;

#[derive(Clone, Copy)]
pub enum Event {
    Ready,
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
    #[cfg(test)]
    pub rows: std::sync::Mutex<Vec<String>>,
}

impl Audit {
    pub fn emit(&self, event: Event, fields: Value) {
        let (category, icon, name) = event.label();
        // Plain severity words let log viewers apply their own palette; no ANSI escapes.
        let slots = fields
            .get("slots_used")
            .and_then(Value::as_u64)
            .zip(fields.get("slots_total").and_then(Value::as_u64))
            .map(|(used, total)| format!(" slots={used}/{total}"))
            .unwrap_or_default();
        let row = format!("{} [{category}] {icon} {name}{slots} {fields}", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        let _ = writeln!(std::io::stderr().lock(), "{row}");
        #[cfg(test)]
        if let Ok(mut rows) = self.rows.lock() {
            rows.push(row);
        }
    }
}
