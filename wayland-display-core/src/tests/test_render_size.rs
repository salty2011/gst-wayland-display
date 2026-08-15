//! Render size (the app-facing `wl_output` mode) is decoupled from the encode size
//! carried by the negotiated caps, and is sticky across caps re-negotiation.

use crate::comp::{apply_render_size, apply_ui_scale, apply_video_info};
use crate::tests::fixture::Fixture;
use crate::tests::test_resolution::{latest_mode_dimensions, make_video_info};
use crate::utils::RenderTarget;
use test_log::test;
use wayland_client::protocol::wl_output;

fn apply_encode(f: &mut Fixture, width: u32, height: u32, fps: i32) {
    apply_video_info(
        &mut f.server,
        make_video_info(width, height, fps),
        &RenderTarget::Software,
        None,
    );
    f.round_trip();
    f.round_trip();
}

fn apply_render(f: &mut Fixture, width: i32, height: i32) {
    apply_render_size(&mut f.server, (width, height).into());
    f.round_trip();
    f.round_trip();
}

fn mode_dimensions(f: &mut Fixture) -> Option<(i32, i32)> {
    latest_mode_dimensions(f.client.get_output_events()).map(|(w, h, _)| (w, h))
}

#[test]
fn render_size_changes_wl_output_mode_but_not_encode_size() {
    let mut f = Fixture::new();
    f.create_window(320, 240);

    apply_encode(&mut f, 1920, 1080, 60);
    assert_eq!(mode_dimensions(&mut f), Some((1920, 1080)));

    f.client.get_output_events().clear();
    apply_render(&mut f, 1280, 720);
    assert_eq!(
        mode_dimensions(&mut f),
        Some((1280, 720)),
        "clients should observe the requested render size as the output mode",
    );

    // encode side untouched
    let vi = f.server.video_info.as_ref().unwrap();
    assert_eq!(
        (vi.width(), vi.height()),
        (1920, 1080),
        "render size must not disturb the negotiated encode size",
    );
}

#[test]
fn render_size_survives_video_info_reapply() {
    let mut f = Fixture::new();
    f.create_window(320, 240);

    apply_encode(&mut f, 1920, 1080, 60);
    apply_render(&mut f, 1280, 720);

    // re-negotiation of the same encode caps must NOT reset render size
    f.client.get_output_events().clear();
    apply_encode(&mut f, 1920, 1080, 60);

    assert_eq!(mode_dimensions(&mut f), Some((1280, 720)));
    let vi = f.server.video_info.as_ref().unwrap();
    assert_eq!((vi.width(), vi.height()), (1920, 1080));
}

#[test]
fn render_size_larger_than_encode_is_clamped() {
    let mut f = Fixture::new();
    f.create_window(320, 240);

    apply_encode(&mut f, 1280, 720, 60);
    f.client.get_output_events().clear();
    apply_render(&mut f, 1920, 1080);

    assert_eq!(
        mode_dimensions(&mut f),
        Some((1280, 720)),
        "a render size larger than the encode size is clamped to the encode size",
    );
}

#[test]
fn zero_render_size_returns_to_following_the_encode_size() {
    let mut f = Fixture::new();
    f.create_window(320, 240);

    apply_encode(&mut f, 1920, 1080, 60);
    apply_render(&mut f, 1280, 720);
    assert_eq!(mode_dimensions(&mut f), Some((1280, 720)));

    f.client.get_output_events().clear();
    apply_render(&mut f, 0, 0);
    assert_eq!(
        mode_dimensions(&mut f),
        Some((1920, 1080)),
        "a (0,0) render size means 'follow the encode size'",
    );
    assert!(f.server.render_size.is_none());
}

#[test]
fn render_size_before_video_info_is_applied_on_negotiation() {
    let mut f = Fixture::new();
    f.create_window(320, 240);

    apply_render(&mut f, 1280, 720);
    f.client.get_output_events().clear();
    apply_encode(&mut f, 1920, 1080, 60);

    assert_eq!(
        mode_dimensions(&mut f),
        Some((1280, 720)),
        "a render size requested before caps negotiation must survive it",
    );
}

#[test]
fn ui_scale_is_sent_as_preferred_scale_to_mapped_toplevel() {
    let mut f = Fixture::new();
    let surface = f.create_window_with_fractional_scale(320, 240);
    apply_encode(&mut f, 1920, 1080, 60);

    // The scale is announced at wp_fractional_scale_v1 creation time, at the default 1.0.
    assert_eq!(f.client.last_preferred_scale(&surface), Some(120));
    assert_eq!(mode_dimensions(&mut f), Some((1920, 1080)));

    let before = f.client.configure_count();
    f.client.get_output_events().clear();
    apply_ui_scale(&mut f.server, 2.0);
    f.round_trip();
    f.round_trip();

    assert_eq!(
        f.client.last_preferred_scale(&surface),
        Some(240),
        "the UI scale must reach the client as preferred_scale in 1/120ths",
    );
    assert!(
        f.client.configure_count() > before,
        "a UI-scale change must be followed by a (non-empty) configure (was {}, now {})",
        before,
        f.client.configure_count(),
    );
    assert!(
        f.client.configure_count() >= 2,
        "the toplevel should have been configured at least at map and at the scale change",
    );

    // A pure hint: the wl_output must not move at all -- no new scale, no new mode.
    let output_events = f.client.get_output_events();
    assert!(
        !output_events
            .iter()
            .any(|e| matches!(e, wl_output::Event::Scale { .. })),
        "a UI-scale change must NOT change the wl_output scale: {:?}",
        output_events,
    );
    assert!(
        !output_events
            .iter()
            .any(|e| matches!(e, wl_output::Event::Mode { .. })),
        "a UI-scale change must NOT change the wl_output mode: {:?}",
        output_events,
    );

    // ... and the encode size is untouched.
    let vi = f.server.video_info.as_ref().unwrap();
    assert_eq!((vi.width(), vi.height()), (1920, 1080));
}

#[test]
fn ui_scale_reaches_a_toplevel_still_pending_its_initial_configure() {
    let mut f = Fixture::new();

    // Created + committed, but the initial configure is not acked yet: the toplevel lives
    // in `pending_windows`, not in the space. This is the window a session-start UiScale
    // lands in.
    let surface = f.client.map_toplevel_with_fractional_scale();
    f.round_trip();
    assert_eq!(f.client.last_preferred_scale(&surface), Some(120));
    assert!(
        f.server.space.elements().next().is_none(),
        "precondition: the toplevel must not be mapped yet",
    );

    apply_ui_scale(&mut f.server, 2.0);
    f.round_trip();

    // Completing the map must not lose it either.
    f.finish_window(320, 240);

    assert_eq!(
        f.client.last_preferred_scale(&surface),
        Some(240),
        "a UiScale arriving before the initial configure must still reach the client",
    );
}

#[test]
fn ui_scale_out_of_range_is_clamped() {
    let mut f = Fixture::new();

    apply_ui_scale(&mut f.server, 7.5);
    assert_eq!(f.server.ui_scale, 3.0);

    apply_ui_scale(&mut f.server, 0.2);
    assert_eq!(f.server.ui_scale, 1.0);
}

#[test]
fn render_size_change_triggers_a_configure() {
    let mut f = Fixture::new();
    f.create_window(320, 240);
    apply_encode(&mut f, 1920, 1080, 60);

    let before = f.client.configure_count();
    apply_render(&mut f, 1280, 720);

    assert!(
        f.client.configure_count() > before,
        "a render-size change should re-configure mapped toplevels (was {}, now {})",
        before,
        f.client.configure_count(),
    );
}
