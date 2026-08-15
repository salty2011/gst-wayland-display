use smithay::delegate_fractional_scale;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};

use crate::comp::State;

impl FractionalScaleHandler for State {
    /// A client just created a `wp_fractional_scale_v1` for one of its surfaces: answer
    /// immediately with the currently requested UI scale, so a surface that appears after a
    /// `Command::UiScale` still learns about it. Later changes are re-announced from
    /// `comp::configure_toplevels`.
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.ui_scale;
        with_states(&surface, |states| {
            with_fractional_scale(states, |fs| fs.set_preferred_scale(scale));
        });
    }
}

delegate_fractional_scale!(State);
