//! A toplevel that maps before the `wl_output` exists must be parked, not dropped.
//!
//! The map path in `wayland/handlers/compositor.rs` needs the output (it sizes the initial
//! configure from the output mode). It used to `swap_remove` the window out of
//! `pending_windows` and only *then* check `self.output.is_none()`, so the removed `Window`
//! was dropped on the floor: never re-queued, never mapped, invisible for the whole session.
//!
//! In production the output exists ~140 ms before any client connects, so this never fired —
//! but that is a timing accident, not an invariant, and the failure mode ("the app never
//! appears") is expensive to diagnose. quasar #487.

use crate::comp::apply_video_info;
use crate::tests::fixture::Fixture;
use crate::tests::test_resolution::make_video_info;
use crate::utils::RenderTarget;
use test_log::test;

/// Create the output and set its first mode — the one call that brings a `wl_output` into
/// existence in production (`Command::VideoInfo`).
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

#[test]
fn a_toplevel_committed_before_the_output_exists_survives_and_maps() {
    let mut f = Fixture::new_cold();
    assert!(f.server.output.is_none(), "precondition: no Output yet");

    // A client that commits a mapped buffer while there is still no output. Before the fix
    // this commit consumed the pending window and dropped it.
    //
    // Note this cannot go through `Fixture::create_window`: that helper acks the initial
    // configure, and in this ordering there is no configure to ack — the compositor cannot
    // size one without an output. Committing bare and waiting is exactly what a real client
    // does here, and it is why parking alone is not enough (nothing would ever wake it).
    f.client.create_window();
    f.round_trip();
    f.client.setup_window_unconfigured(320, 240);
    f.round_trip();
    f.round_trip();

    assert_eq!(
        f.server.pending_windows.len(),
        1,
        "the toplevel must stay parked in pending_windows while there is no output",
    );
    assert_eq!(
        f.server.space.elements().count(),
        0,
        "nothing can be mapped before there is an output to map into",
    );

    // The output comes into existence. `apply_output_mode` sends the deferred initial
    // configure to whatever is still parked, which unblocks a client that is waiting on it.
    apply_encode(&mut f, 1920, 1080, 60);

    // The client acks that configure and commits again; the normal map path takes over.
    f.client.setup_window(320, 240);
    f.round_trip();
    f.round_trip();

    assert_eq!(
        f.server.space.elements().count(),
        1,
        "the parked toplevel must map once an output exists",
    );
    assert!(
        f.server.pending_windows.is_empty(),
        "a mapped toplevel leaves pending_windows",
    );
}
