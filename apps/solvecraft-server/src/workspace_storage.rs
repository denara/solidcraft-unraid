//! Cached capacity of the actual shared-data filesystem; never the container root.
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct Storage {
    cached: Mutex<Option<(Instant, Value)>>,
}
impl Storage {
    pub(crate) fn invalidate(&self) {
        if let Ok(mut cached) = self.cached.lock() {
            *cached = None;
        }
    }
    pub(crate) fn snapshot(&self, root: &Path) -> Value {
        let Ok(mut cached) = self.cached.lock() else { return json!({"label":"Speicherplatz nicht verfügbar"}) };
        if let Some((when, value)) = cached.as_ref()
            && when.elapsed() < Duration::from_secs(15)
        {
            return value.clone();
        }
        let value = capacity(root);
        *cached = Some((Instant::now(), value.clone()));
        value
    }
}
fn units(bytes: u64) -> String {
    let (div, unit) = if bytes >= 1 << 40 {
        (1099511627776., "TiB")
    } else if bytes >= 1 << 30 {
        (1073741824., "GiB")
    } else {
        (1048576., "MiB")
    };
    format!("{:.1} {unit}", bytes as f64 / div)
}
fn capacity(root: &Path) -> Value {
    #[cfg(unix)]
    if let Ok(stat) = rustix::fs::statvfs(root) {
        let block = stat.f_frsize;
        if let (Some(available), Some(total)) = (stat.f_bavail.checked_mul(block), stat.f_blocks.checked_mul(block))
            && total > 0
            && available <= total
        {
            // MiB precision prevents background filesystem noise from spamming SSE.
            let available = (available >> 20) << 20;
            let total = (total >> 20) << 20;
            return json!({"available_bytes":available,"total_bytes":total,"label":format!("Speicher frei: {} von {}",units(available),units(total))});
        }
    }
    let _ = root;
    json!({"available_bytes":null,"total_bytes":null,"label":"Speicherplatz nicht verfügbar"})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_units_remain_readable() {
        assert_eq!(units(32 << 20), "32.0 MiB");
        assert_eq!(units(4 << 30), "4.0 GiB");
        assert_eq!(units(2 << 40), "2.0 TiB");
    }
    #[test]
    fn unavailable_is_not_reported_as_full() {
        assert_eq!(capacity(Path::new("/no-such-solidcraft-volume"))["available_bytes"], Value::Null);
    }
    #[cfg(unix)]
    #[test]
    fn capacity_uses_data_root_and_is_cached() {
        let s = Storage::default();
        let a = s.snapshot(&std::env::temp_dir());
        assert!(a["available_bytes"].is_u64());
        assert_eq!(a, s.snapshot(Path::new("/missing")));
        s.invalidate();
        assert!(s.snapshot(Path::new("/missing"))["available_bytes"].is_null());
    }
}
