//! Keeps winit's window-move setting in sync with the Windows layout preference.

/// Passes Preferences › Layout's `windows_smooth_window_drag` to the vendored winit, at start and
/// whenever it changes; winit reads it at each message, so it applies at once.
#[derive(Default)]
pub struct WindowMove {
    /// The value winit was given last (`set_quiet_window_move`); `None` before the first.
    quiet: Option<bool>,
}

impl WindowMove {
    /// Give winit `smooth_window_drag` if it differs from what it was given last, or nothing was.
    pub fn follow(&mut self, smooth_window_drag: bool) {
        if let Some(quiet) = self.change(smooth_window_drag) {
            winit::platform::windows::set_quiet_window_move(quiet);
        }
    }

    /// The value to give winit for `smooth_window_drag` (remembered as given), `None` when winit
    /// has it already.
    fn change(&mut self, smooth_window_drag: bool) -> Option<bool> {
        let quiet = quiet_window_move(smooth_window_drag);
        (self.quiet != Some(quiet)).then(|| {
            self.quiet = Some(quiet);
            quiet
        })
    }
}

/// winit's switch for the preference: a quiet move (nothing drawn while the title bar is dragged,
/// the picture catches up when the drag ends) only when the preference is on. Off, winit keeps its
/// own behaviour: the app draws during the drag, as before.
fn quiet_window_move(smooth_window_drag: bool) -> bool {
    smooth_window_drag
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off (the default) leaves winit as it is: the app draws during the drag, as before. On
    /// gives winit a quiet move. Each value is given once, at start and whenever it changes. The
    /// pure part only: the switch itself is one per process, which the tests running in it share.
    #[test]
    fn window_move_gives_winit_the_preference_once_per_change() {
        assert!(!quiet_window_move(false), "the default: drawing goes on during the drag, as before");
        assert!(quiet_window_move(true), "the preference on: nothing drawn while the window is dragged");
        let mut m = WindowMove::default();
        assert_eq!(m.change(false), Some(false), "at start, whatever the value");
        assert_eq!(m.change(false), None, "unchanged: winit has it");
        assert_eq!(m.change(true), Some(true), "turned on");
        assert_eq!(m.change(true), None);
        assert_eq!(m.change(false), Some(false), "and off again");
        let mut on = WindowMove::default();
        assert_eq!(on.change(true), Some(true), "a saved `true` is given at start too");
    }
}
