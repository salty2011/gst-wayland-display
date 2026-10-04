//! `Command::FollowClientSize`: for a nested guest display server that speaks no
//! `wlr-output-management` at all but still resizes its own fullscreen window when the user
//! picks a resolution inside it, treat that resize as an implicit mode request --
//! `maybe_follow_client_size`, wired into the commit handler in
//! `wayland/handlers/compositor.rs`.

use crate::Command;
use crate::comp::{apply_output_modes, apply_video_info};
use crate::tests::fixture::Fixture;
use crate::tests::test_resolution::make_video_info;
use crate::utils::RenderTarget;
use std::sync::mpsc::{Receiver, channel};
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

/// Wire a mode-request receiver into the fixture's compositor state.
fn mode_requests(f: &mut Fixture) -> Receiver<Command> {
    let (tx, rx) = channel();
    f.server.mode_request_tx = Some(tx);
    rx
}

fn drain_requests(rx: &Receiver<Command>) -> Vec<(i32, i32, i32)> {
    let mut out = Vec::new();
    while let Ok(cmd) = rx.try_recv() {
        if let Command::ModeRequest {
            width,
            height,
            refresh_mhz,
        } = cmd
        {
            out.push((width, height, refresh_mhz));
        }
    }
    out
}

/// Map a fullscreen app with no `wp_viewport`: its committed buffer size IS the "physical
/// pixels after any viewport destination" size `maybe_follow_client_size` reads -- the same
/// measurement `window_fullscreen_fit` uses. Acks the initial configure, so this is only for
/// the FIRST commit of a window.
fn map_fullscreen(f: &mut Fixture, buf_w: u16, buf_h: u16) {
    f.client.create_window();
    f.round_trip();
    f.client
        .setup_window_solid_no_viewport(buf_w, buf_h, 0x7f_7f_7f);
    f.round_trip();
    f.round_trip();
    for window in f.server.space.elements() {
        window.on_commit();
    }
}

/// Resize the (already-mapped) fullscreen window from [`map_fullscreen`] by committing a new
/// buffer, same as a nested guest resizing its own window on its own initiative -- no new
/// configure arrives to ack.
fn resize_fullscreen(f: &mut Fixture, buf_w: u16, buf_h: u16) {
    f.client.resize_solid_no_viewport(buf_w, buf_h, 0x7f_7f_7f);
    f.round_trip();
    f.round_trip();
    for window in f.server.space.elements() {
        window.on_commit();
    }
}

const MONITOR: &[(i32, i32, i32)] = &[
    (2560, 1440, 143_981),
    (2560, 1440, 119_998),
    (1920, 1080, 119_880),
    (1920, 1080, 60_000),
    (1280, 720, 60_000),
];

/// (a) Property OFF: a resized fullscreen buffer produces no `ModeRequest`, even to a size
/// that is advertised.
#[test]
fn off_produces_no_request() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    assert!(!f.server.follow_client_size, "default is off");
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    map_fullscreen(&mut f, 1920, 1080);

    resize_fullscreen(&mut f, 2560, 1440);
    assert!(
        drain_requests(&rx).is_empty(),
        "follow_client_size is off; must behave exactly as before"
    );
}

/// (b) Property ON: a buffer resized to an advertised size produces exactly one
/// `ModeRequest`, and it picks the advertised mode of that size whose refresh is nearest the
/// CURRENT mode's (not the first or the highest).
#[test]
fn on_requests_the_matching_size_with_nearest_refresh() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60); // current: 1920x1080 @ 60_000
    f.server.follow_client_size = true;
    map_fullscreen(&mut f, 1920, 1080); // matches current: no request yet

    resize_fullscreen(&mut f, 2560, 1440);

    // 2560x1440 is advertised at both 143_981 and 119_998; nearest to the current 60_000 is
    // 119_998 (|119998-60000| < |143981-60000|).
    assert_eq!(drain_requests(&rx), vec![(2560, 1440, 119_998)]);
}

/// (c) Property ON, buffer size equals the CURRENT mode's size: no request -- that is the
/// echo after the host already moved the output, not a new pick.
#[test]
fn on_matching_current_size_produces_no_request() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    f.server.follow_client_size = true;
    map_fullscreen(&mut f, 1920, 1080);

    // Re-commit the exact same (current) size again.
    resize_fullscreen(&mut f, 1920, 1080);

    assert!(drain_requests(&rx).is_empty());
}

/// (d) Property ON, buffer size not in `output_modes`: no request.
#[test]
fn on_unadvertised_size_produces_no_request() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    f.server.follow_client_size = true;
    map_fullscreen(&mut f, 1920, 1080);

    resize_fullscreen(&mut f, 800, 600);

    assert!(drain_requests(&rx).is_empty());
}

/// (e) Property ON, a SECOND commit of the same (already-pending) size while the first
/// request is still outstanding: no second request.
#[test]
fn on_repeated_commit_of_pending_size_requests_once() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    f.server.follow_client_size = true;
    map_fullscreen(&mut f, 1920, 1080);

    resize_fullscreen(&mut f, 2560, 1440);
    resize_fullscreen(&mut f, 2560, 1440);
    resize_fullscreen(&mut f, 2560, 1440);

    assert_eq!(
        drain_requests(&rx),
        vec![(2560, 1440, 119_998)],
        "only the first commit of a still-pending size requests"
    );
}

/// (f) Property ON: after `apply_video_info` moves the current mode to the requested size
/// (the owner honouring the request), a LATER resize to a *different* advertised size
/// requests again -- proving the pending-clear-on-success path works, not just the
/// 3-second timeout.
#[test]
fn on_requests_again_after_the_mode_actually_moves() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60); // current: 1920x1080 @ 60_000
    f.server.follow_client_size = true;
    map_fullscreen(&mut f, 1920, 1080);

    resize_fullscreen(&mut f, 2560, 1440);
    assert_eq!(drain_requests(&rx), vec![(2560, 1440, 119_998)]);

    // The owner re-negotiates the caps at the requested size -- the current mode actually
    // becomes 2560x1440, clearing the pending guard immediately (not via the timeout).
    apply_encode(&mut f, 2560, 1440, 120);
    assert_eq!(
        f.server.pending_follow_request, None,
        "the pending guard must clear the instant the mode actually moves"
    );

    // A later resize to a different advertised size must be requested, proving the guard was
    // really cleared rather than coincidentally still within the 3s window.
    resize_fullscreen(&mut f, 1920, 1080);
    assert_eq!(
        drain_requests(&rx),
        vec![(1920, 1080, 119_880)],
        "nearest to the new current refresh 119_998 among the 1920x1080 entries"
    );
}
