use smithay::delegate_fractional_scale;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::fractional_scale::FractionalScaleHandler;

use crate::comp::{State, set_preferred_ui_scale};

impl FractionalScaleHandler for State {
    /// A client just created a `wp_fractional_scale_v1` for one of its surfaces: answer
    /// immediately with the currently requested UI scale, so a surface that appears after a
    /// `Command::UiScale` still learns about it. Later changes are re-announced from
    /// `comp::announce_ui_scale`.
    ///
    /// KNOWN HAZARD (not yet observed in the wild; the `previous`/`sent` fields logged below
    /// are there to catch it): smithay's `set_preferred_scale` only emits when the value
    /// *changes* (`wayland/fractional_scale/mod.rs:238-245`), and the per-surface state is
    /// created on demand by `with_fractional_scale` even when no object exists yet. A
    /// toplevel is in `pending_windows` from `get_toplevel()` onward, so an
    /// `announce_ui_scale` that runs between `get_toplevel()` and this callback stores the
    /// value against a surface that has no object — and then this call is debounced and the
    /// client never receives an initial `preferred_scale`. It recovers on the next *changed*
    /// scale, which is why a 1.0 -> 2.0 transition still works.
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.ui_scale;
        let previous = set_preferred_ui_scale(&surface, scale);
        tracing::info!(
            scale,
            ?previous,
            sent = previous != Some(scale),
            "Client created wp_fractional_scale_v1",
        );
    }
}

delegate_fractional_scale!(State);
