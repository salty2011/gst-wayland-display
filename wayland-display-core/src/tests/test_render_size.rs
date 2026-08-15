//! Render size (the app-facing `wl_output` mode) is decoupled from the encode size
//! carried by the negotiated caps, and is sticky across caps re-negotiation.

use crate::comp::{apply_render_size, apply_video_info};
use crate::tests::fixture::Fixture;
use crate::tests::test_resolution::{latest_mode_dimensions, make_video_info};
use crate::utils::RenderTarget;
use test_log::test;

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
