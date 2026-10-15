use once_cell::sync::Lazy;
use smithay::backend::drm::{DrmNode, NodeType};
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::renderer::gles::GlesRenderer;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// One EGL display per render node, never dropped (quasar #570). smithay terminates a
/// display with its last handle. NVIDIA's Vulkan driver keeps an EGL context of its own on
/// the same per-device display and destroys it with the process's last Vulkan instance,
/// which fails EGL_NOT_INITIALIZED on a display terminated first. Holding the handle for the
/// life of the process does not depend on which is torn down last.
static EGL_DISPLAYS: Lazy<Mutex<HashMap<Option<DrmNode>, Arc<EGLDisplay>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn get_egl_device_for_node(drm_node: &DrmNode) -> EGLDevice {
    let drm_node = drm_node
        .node_with_type(NodeType::Render)
        .and_then(Result::ok)
        .unwrap_or(*drm_node);
    EGLDevice::enumerate()
        .expect("Failed to enumerate EGLDevices")
        .find(|d| d.try_get_render_node().unwrap_or_default() == Some(drm_node))
        .expect("Unable to find EGLDevice for drm-node")
}

/// The table survives a panic of another caller: nothing in it is left half-written.
fn displays() -> MutexGuard<'static, HashMap<Option<DrmNode>, Arc<EGLDisplay>>> {
    EGL_DISPLAYS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn held_display(render_node: Option<DrmNode>) -> Arc<EGLDisplay> {
    let mut displays = displays();
    match displays.get(&render_node) {
        Some(display) => display.clone(),
        None => {
            let device = match render_node.as_ref() {
                Some(render_node) => get_egl_device_for_node(render_node),
                None => EGLDevice::enumerate()
                    .expect("Failed to enumerate EGLDevices")
                    .find(|device| {
                        device
                            .extensions()
                            .iter()
                            .any(|e| e == "EGL_MESA_device_software")
                    })
                    .expect("Failed to find software device"),
            };
            let egl = unsafe { EGLDisplay::new(device).expect("Failed to create EGLDisplay") };
            let display = Arc::new(egl);
            displays.insert(render_node, display.clone());
            display
        }
    }
}

pub fn setup_renderer(render_node: Option<DrmNode>) -> GlesRenderer {
    let egl = held_display(render_node);
    // GLES2 is fine for HDR: fp16/10-bit dmabufs import via EGLImage (bypassing the GLES core
    // texture-format limits), so they're in the renderer's importable set already. (A GLES3
    // context fails to configure in this headless/surfaceless EGL setup and isn't needed.)
    let context = EGLContext::new(&egl).unwrap_or_else(|err| {
        // A display that went bad (a GPU reset) is not kept: the next session makes its own.
        displays().remove(&render_node);
        panic!("Failed to initialize EGL context: {err}")
    });

    unsafe { GlesRenderer::new(context) }.expect("Failed to initialize renderer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::backend::egl::ffi;

    /// A display terminated with its last renderer answers no query; a held one does.
    #[test]
    fn the_egl_display_outlives_its_last_renderer() {
        let renderer = setup_renderer(None);
        let handle = renderer.egl_context().display().get_display_handle().handle;
        drop(renderer);

        let vendor = unsafe { ffi::egl::QueryString(handle, ffi::egl::VENDOR as i32) };
        assert!(!vendor.is_null(), "the display was terminated");

        let again = setup_renderer(None);
        assert_eq!(
            again.egl_context().display().get_display_handle().handle,
            handle
        );
    }
    /// A caller that panicked holding the table does not take every later session down.
    #[test]
    fn a_panic_holding_the_display_table_does_not_poison_it() {
        let _ = std::thread::spawn(|| {
            let _held = EGL_DISPLAYS.lock().unwrap();
            panic!("a caller died holding the table");
        })
        .join();
        assert!(EGL_DISPLAYS.is_poisoned());
        drop(setup_renderer(None));
    }
}
