//! The display's real mode set (`Command::OutputModes`): advertised on `wl_output` with each
//! mode's own refresh rate, listed through `wlr-output-management`, and a client's `apply`
//! forwarded as a mode request -- the compositor itself changing nothing.

use crate::Command;
use crate::comp::{apply_output_modes, apply_video_info, snap_refresh};
use crate::tests::fixture::Fixture;
use crate::tests::test_resolution::make_video_info;
use crate::utils::RenderTarget;
use smithay::output::Mode as OutputMode;
use std::sync::mpsc::{Receiver, channel};
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

/// Every `wl_output::mode` the client has seen, as `(width, height, refresh, is_current)`.
fn modes(f: &mut Fixture) -> Vec<(i32, i32, i32, bool)> {
    f.client
        .get_output_events()
        .iter()
        .filter_map(|e| match e {
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => Some((
                *width,
                *height,
                *refresh,
                flags
                    .into_result()
                    .map(|f| f.contains(wl_output::Mode::Current))
                    .unwrap_or(false),
            )),
            _ => None,
        })
        .collect()
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

const MONITOR: &[(i32, i32, i32)] = &[
    (2560, 1440, 143_981),
    (2560, 1440, 119_998),
    (1920, 1080, 119_880),
    (1920, 1080, 60_000),
    (1280, 720, 60_000),
];

/// The set is advertised unfiltered -- including a mode LARGER than the encode size -- each
/// entry with its own refresh, and the current mode is the one matching the caps.
#[test]
fn output_modes_are_advertised_with_their_own_refresh() {
    let mut f = Fixture::new_cold();
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);

    let seen = modes(&mut f);
    for &(w, h, r) in MONITOR {
        assert!(
            seen.iter()
                .any(|&(sw, sh, sr, _)| (sw, sh, sr) == (w, h, r)),
            "{w}x{h}@{r} must be advertised; saw {seen:?}"
        );
    }
    let current: Vec<_> = seen.iter().filter(|m| m.3).collect();
    assert_eq!(current.len(), 1, "exactly one current mode; saw {seen:?}");
    assert_eq!(
        (current[0].0, current[0].1, current[0].2),
        (1920, 1080, 60_000)
    );
}

/// An integer caps framerate snaps onto the display's real refresh for that size, so the
/// client never sees a near-duplicate pair (`144.000` beside `143.981`).
#[test]
fn current_refresh_snaps_to_the_advertised_mode() {
    let mut f = Fixture::new_cold();
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 2560, 1440, 144);

    let current = f.server.output.as_ref().unwrap().current_mode().unwrap();
    assert_eq!(current.refresh, 143_981);
    let seen = modes(&mut f);
    assert!(
        !seen
            .iter()
            .any(|&(w, h, r, _)| (w, h, r) == (2560, 1440, 144_000)),
        "no synthetic 144.000 Hz mode beside the real 143.981 Hz one; saw {seen:?}"
    );
    // The snap is a pure function with a half-hertz window.
    let set: Vec<OutputMode> = MONITOR
        .iter()
        .map(|&(w, h, r)| OutputMode {
            size: (w, h).into(),
            refresh: r,
        })
        .collect();
    assert_eq!(snap_refresh(&set, (2560, 1440).into(), 120_000), 119_998);
    assert_eq!(
        snap_refresh(&set, (2560, 1440).into(), 60_000),
        60_000,
        "no entry: unchanged"
    );
    assert_eq!(snap_refresh(&set, (1920, 1080).into(), 120_000), 119_880);
    assert_eq!(snap_refresh(&[], (1920, 1080).into(), 60_000), 60_000);
}

/// A set applied AFTER the output exists is advertised at once and re-applying it is a no-op.
#[test]
fn output_modes_after_caps_are_applied_and_idempotent() {
    let mut f = Fixture::new_cold();
    apply_encode(&mut f, 1920, 1080, 60);
    apply_output_modes(&mut f.server, MONITOR);
    let output = f.server.output.clone().unwrap();
    let first: Vec<_> = output.modes();
    assert!(
        first
            .iter()
            .any(|m| m.size.w == 2560 && m.refresh == 143_981)
    );
    apply_output_modes(&mut f.server, MONITOR);
    assert_eq!(output.modes(), first, "a re-apply must not duplicate");
}

/// The `wlr-output-management` head lists the same modes `wl_output` does, with the current
/// one flagged, and a client's `apply` of an advertised mode is answered `succeeded` and
/// forwarded as a request. The compositor's own mode does not move.
#[test]
fn wlr_output_management_lists_the_modes_and_forwards_a_choice() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    f.round_trip();

    let wlr = f.client.wlr();
    assert_eq!(wlr.heads.len(), 1, "one head");
    let head = &wlr.heads[0];
    assert_eq!(head.name.as_deref(), Some("HEADLESS-1"));
    assert_eq!(head.enabled, Some(true));
    let triples: Vec<_> = head.modes.iter().filter_map(|m| m.triple()).collect();
    for &m in MONITOR {
        assert!(
            triples.contains(&m),
            "{m:?} listed through wlr; saw {triples:?}"
        );
    }
    let current = head.current.clone().expect("a current mode");
    let current_triple = head
        .modes
        .iter()
        .find(|m| m.mode == current)
        .and_then(|m| m.triple());
    assert_eq!(current_triple, Some((1920, 1080, 60_000)));
    assert!(!wlr.serials.is_empty(), "done was sent");

    f.client
        .wlr_configure((2560, 1440, 143_981), false, true, None);
    f.round_trip();
    f.round_trip();
    assert_eq!(f.client.wlr().results, vec!["succeeded"]);
    assert_eq!(drain_requests(&rx), vec![(2560, 1440, 143_981)]);
    let mode = f.server.output.as_ref().unwrap().current_mode().unwrap();
    assert_eq!(
        (mode.size.w, mode.size.h),
        (1920, 1080),
        "the compositor moved nothing"
    );
}

/// A custom mode is matched against the advertised set within half a hertz; `test` reports
/// without forwarding; a mode that is not advertised fails; a stale serial is cancelled;
/// disabling the only head fails.
#[test]
fn wlr_output_management_validates_requests() {
    let mut f = Fixture::new_cold();
    let rx = mode_requests(&mut f);
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    f.round_trip();

    // `144000` custom -> the real 143.981 Hz entry; test only, so no request.
    f.client
        .wlr_configure((2560, 1440, 144_000), true, false, None);
    f.round_trip();
    f.round_trip();
    assert_eq!(f.client.wlr().results, vec!["succeeded"]);
    assert!(drain_requests(&rx).is_empty(), "test never forwards");

    // Applied, the same custom mode is forwarded as the advertised entry.
    f.client
        .wlr_configure((2560, 1440, 144_000), true, true, None);
    f.round_trip();
    f.round_trip();
    assert_eq!(drain_requests(&rx), vec![(2560, 1440, 143_981)]);

    // Not advertised.
    f.client.wlr_configure((800, 600, 60_000), true, true, None);
    f.round_trip();
    f.round_trip();
    assert_eq!(f.client.wlr().results.last(), Some(&"failed"));
    assert!(drain_requests(&rx).is_empty());

    // Stale serial.
    let stale = f.client.wlr().serials.last().unwrap().wrapping_add(7);
    f.client
        .wlr_configure((1920, 1080, 119_880), false, true, Some(stale));
    f.round_trip();
    f.round_trip();
    assert_eq!(f.client.wlr().results.last(), Some(&"cancelled"));
    assert!(drain_requests(&rx).is_empty());

    // The only head cannot be disabled.
    f.client.wlr_disable_head();
    f.round_trip();
    f.round_trip();
    assert_eq!(f.client.wlr().results.last(), Some(&"failed"));
    assert!(drain_requests(&rx).is_empty());
}

/// When the owner acts on a request (new caps at the chosen mode), every manager learns the
/// new current mode with a fresh serial.
#[test]
fn wlr_output_management_publishes_the_new_current_mode() {
    let mut f = Fixture::new_cold();
    apply_output_modes(&mut f.server, MONITOR);
    apply_encode(&mut f, 1920, 1080, 60);
    f.round_trip();
    let serials_before = f.client.wlr().serials.len();

    apply_encode(&mut f, 2560, 1440, 144);
    f.round_trip();

    let wlr = f.client.wlr();
    assert!(wlr.serials.len() > serials_before, "a new done serial");
    let head = &wlr.heads[0];
    let current = head.current.clone().expect("a current mode");
    let current_triple = head
        .modes
        .iter()
        .find(|m| m.mode == current)
        .and_then(|m| m.triple());
    assert_eq!(current_triple, Some((2560, 1440, 143_981)));
}
