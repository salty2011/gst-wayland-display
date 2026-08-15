//! Property-surface tests for `waylanddisplaysrc`.
//!
//! These only construct the element (no `start()`, so no compositor thread, no GPU and
//! no wayland socket) and exercise the GObject property plumbing for the decoupled
//! render size + UI scale: defaults, read-back, and the boundary/clamping behaviour of
//! the ParamSpecs. Runtime forwarding to a live compositor is covered by the
//! `wayland-display-core` unit tests and by live QA.

use gst::prelude::*;
use std::sync::Once;

static INIT: Once = Once::new();

fn init() {
    INIT.call_once(|| {
        gst::init().expect("gst init");
        gstwaylanddisplaysrc::plugin_register_static().expect("register plugin");
    });
}

fn make_src() -> gst::Element {
    init();
    gst::ElementFactory::make("waylanddisplaysrc")
        .build()
        .expect("create waylanddisplaysrc")
}

#[test]
fn render_size_and_ui_scale_defaults() {
    let src = make_src();
    assert_eq!(src.property::<i32>("render-width"), 0);
    assert_eq!(src.property::<i32>("render-height"), 0);
    assert_eq!(src.property::<f64>("ui-scale"), 1.0);
}

#[test]
fn render_size_round_trips() {
    let src = make_src();
    src.set_property("render-width", 1280i32);
    src.set_property("render-height", 720i32);
    assert_eq!(src.property::<i32>("render-width"), 1280);
    assert_eq!(src.property::<i32>("render-height"), 720);
}

#[test]
fn render_size_resets_to_follow_encode() {
    let src = make_src();
    src.set_property("render-width", 1280i32);
    src.set_property("render-height", 720i32);
    src.set_property("render-width", 0i32);
    src.set_property("render-height", 0i32);
    assert_eq!(src.property::<i32>("render-width"), 0);
    assert_eq!(src.property::<i32>("render-height"), 0);
}

#[test]
fn ui_scale_round_trips() {
    let src = make_src();
    src.set_property("ui-scale", 1.5f64);
    assert_eq!(src.property::<f64>("ui-scale"), 1.5);
}

#[test]
fn properties_are_readwrite_and_not_construct_only() {
    let src = make_src();
    for name in ["render-width", "render-height", "ui-scale"] {
        let pspec = src.find_property(name).expect("property exists");
        let flags = pspec.flags();
        assert!(
            flags.contains(gst::glib::ParamFlags::READABLE)
                && flags.contains(gst::glib::ParamFlags::WRITABLE),
            "{name} must be READWRITE"
        );
        assert!(
            !flags.contains(gst::glib::ParamFlags::CONSTRUCT_ONLY),
            "{name} must be live-writable, not construct-only"
        );
    }
}

#[test]
fn render_size_bounds() {
    let src = make_src();
    let w = src.find_property("render-width").unwrap();
    let spec = w
        .downcast::<gst::glib::ParamSpecInt>()
        .expect("render-width is an int");
    assert_eq!(spec.minimum(), 0);
    assert_eq!(spec.maximum(), 16384);

    let s = src.find_property("ui-scale").unwrap();
    let spec = s
        .downcast::<gst::glib::ParamSpecDouble>()
        .expect("ui-scale is a double");
    assert_eq!(spec.minimum(), 1.0);
    assert_eq!(spec.maximum(), 3.0);
}
