//! Compatibility API for the upstream 3Dconnexion driver client.
//! The Unraid fork omits the platform FFI bridge and returns Unsupported.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::fmt;

/// What the driver reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A 3D mouse was plugged in or the driver connected to one.
    DeviceAdded,
    /// The 3D mouse was unplugged or the driver went away.
    DeviceRemoved,
    /// The cap moved: push left/right, forward/back, lift/press, then tilt forward/back,
    /// tilt left/right and twist, each about -350..350 as the HID device reports them.
    Axes { translation: [i16; 3], rotation: [i16; 3] },
    /// The buttons that are down, one bit each.
    Buttons(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Not macOS.
    Unsupported,
    /// The driver's framework is not installed (or not loadable).
    NotInstalled,
    /// The framework lacks a function this client needs (another driver version).
    MissingSymbol(&'static str),
    /// The driver refused the handlers (its `OSErr`).
    Handlers(i16),
    /// The driver refused the client registration.
    Register,
    /// [`start`] was called before.
    AlreadyStarted,
    /// The client's thread could not be started.
    Thread(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported => write!(f, "the 3Dconnexion driver client only exists on macOS"),
            Error::NotInstalled => write!(f, "the 3Dconnexion driver is not installed"),
            Error::MissingSymbol(name) => write!(f, "the 3Dconnexion driver lacks {name}"),
            Error::Handlers(code) => write!(f, "the 3Dconnexion driver refused the handlers (error {code})"),
            Error::Register => write!(f, "the 3Dconnexion driver refused the client"),
            Error::AlreadyStarted => write!(f, "the 3Dconnexion client is already started"),
            Error::Thread(reason) => write!(f, "the 3Dconnexion client thread did not start: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// The platform driver bridge is not included in this container fork.
/// Returns Unsupported without starting a thread or invoking the callback.
pub fn start(name: &str, sink: impl Fn(Event) + Send + Sync + 'static) -> Result<(), Error> {
    let _ = (name, sink);
    Err(Error::Unsupported)
}

#[cfg(test)]
mod tests {
    #[test]
    fn omitted_driver_bridge_reports_unsupported_without_invoking_the_sink() {
        let invoked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sink_invoked = std::sync::Arc::clone(&invoked);
        let result = super::start("public test", move |_| {
            sink_invoked.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        assert_eq!(result, Err(super::Error::Unsupported));
        assert!(!invoked.load(std::sync::atomic::Ordering::Relaxed));
    }
}
