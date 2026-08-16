use crate::comp::State;
use smithay::delegate_pointer_constraints;
use smithay::input::pointer::PointerHandle;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};
use smithay::wayland::pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint};
use smithay::wayland::seat::WaylandFocus;

impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        if pointer
            .current_focus()
            .map(|x| &*(x.wl_surface().unwrap()) == surface)
            .unwrap_or(false)
        {
            // Resolved through `pointer_focus`, not `Space::element_under`: with a
            // fullscreen fit active the raw (render-space) pointer position misses the
            // window entirely outside its unscaled bbox, and the constraint's region would
            // be evaluated off by the fit scale. This is the lock-before-map path (SDL,
            // nested gamescope with --force-grab-cursor), so it must agree with the motion
            // path that activates constraints later.
            let (focus, position) = self.pointer_focus(self.pointer_location);
            let under = focus.map(|(window, origin)| (window.into(), origin));
            self.maybe_activate_pointer_constraint(&under, position);
        }
    }

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        if with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active())
        }) {
            let origin = self
                .space
                .elements()
                .find_map(|window| {
                    (window.wl_surface().as_deref() == Some(surface)).then(|| window.geometry())
                })
                .unwrap_or_default()
                .loc
                .to_f64();

            pointer.set_location(origin + location);
        }
    }
}

delegate_pointer_constraints!(State); // Needed by SDL in order to lock the pointer to the window
