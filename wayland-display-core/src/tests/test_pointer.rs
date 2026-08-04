use crate::ButtonState;
use crate::tests::client::MouseEvents;
use crate::tests::fixture::Fixture;
use smithay::utils::Point;
use test_log::test;
use wayland_client::protocol::wl_pointer;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1;

fn clean_events(events: &mut Vec<MouseEvents>) {
    while let Some(_event) = events.pop() {}
}

#[test]
fn move_mouse() {
    let mut f = Fixture::new();
    f.round_trip();
    f.create_window(320, 240);

    {
        // Mapping a toplevel now resolves pointer focus immediately: the map handler emits a
        // synthetic zero-delta motion, so the client receives its wl_pointer.enter at the
        // pointer's *current* location without waiting for a physical motion event.
        let map_location = f.server.pointer_location;

        let client_events = f.client.get_client_events();
        assert!(!client_events.is_empty());
        let MouseEvents::Pointer(client_event) = client_events.remove(0) else {
            panic!("Unexpected event: {:?}", client_events);
        };
        let wl_pointer::Event::Enter {
            surface_x,
            surface_y,
            ..
        } = client_event
        else {
            panic!("Unexpected event: {:?}", client_event);
        };
        assert_eq!(surface_x, map_location.x);
        assert_eq!(surface_y, map_location.y);

        clean_events(client_events);
    }

    let expected_location = Point::from((0.0, 0.0));
    f.server.pointer_motion_absolute(0, expected_location);
    f.round_trip();

    {
        // Server logic test
        assert_eq!(f.server.pointer_location, expected_location);

        // Client logic test
        let client_events = f.client.get_client_events();
        assert!(client_events.len() >= 1);
        // This first real motion after the map consumes the pending refocus edge trigger,
        // which cycles focus exactly once: one forced leave (plus its frame), then the
        // enter asserted below, so that a wl_pointer created after the map still learns it
        // has focus. Assert that shape precisely -- more than one leave would mean the
        // trigger is not edge-triggered.
        let leave = client_events.remove(0);
        assert!(
            matches!(leave, MouseEvents::Pointer(wl_pointer::Event::Leave { .. })),
            "expected the forced leave first, got: {:?}",
            leave
        );
        if matches!(
            client_events.first(),
            Some(MouseEvents::Pointer(wl_pointer::Event::Frame))
        ) {
            client_events.remove(0);
        }
        let MouseEvents::Pointer(client_event) = client_events.remove(0) else {
            panic!("Unexpected event: {:?}", client_events);
        };
        let wl_pointer::Event::Enter {
            // First time, we are entering the window
            surface_x,
            surface_y,
            ..
        } = client_event
        else {
            panic!("Unexpected event: {:?}", client_event);
        };
        assert_eq!(surface_x, expected_location.x);
        assert_eq!(surface_y, expected_location.y);

        clean_events(client_events);
    }

    let delta = Point::from((10.0, 15.0));
    f.server.pointer_motion(0, 0, delta, delta);
    f.round_trip();

    {
        // Server logic test
        assert_eq!(f.server.pointer_location, expected_location + delta);

        // Client logic test
        let client_events = f.client.get_client_events();
        assert!(client_events.len() >= 1);
        let MouseEvents::Pointer(client_event) = client_events.remove(0) else {
            panic!("Unexpected event: {:?}", client_events);
        };
        let wl_pointer::Event::Motion {
            // Second time, we are moving thru it
            surface_x,
            surface_y,
            ..
        } = client_event
        else {
            panic!("Unexpected event: {:?}", client_event);
        };
        assert_eq!(surface_x, delta.x);
        assert_eq!(surface_y, delta.y);

        clean_events(client_events);
    }
}

#[test]
fn lock_mouse() {
    let mut f = Fixture::new();
    f.round_trip();
    f.create_window(320, 240);

    let expected_location = Point::from((15.0, 45.0));
    f.server.pointer_motion_absolute(0, expected_location);
    f.round_trip();
    {
        let client_events = f.client.get_client_events();
        assert!(client_events.len() >= 1);
        clean_events(client_events);
    }

    let _lock = f.client.lock_pointer();
    let _relative_pointer = f.client.get_relative_pointer();
    f.round_trip();

    // Test pointer_motion()
    let delta = Point::from((10.0, 15.0));
    f.server.pointer_motion(0, 0, delta, delta);
    f.round_trip();
    {
        // Mouse shouldn't be moved!
        assert_eq!(f.server.pointer_location, expected_location);

        // But we should still get Relative mouse events
        let client_events = f.client.get_client_events();
        assert!(client_events.len() >= 2);
        let MouseEvents::Relative(client_event) = client_events.remove(0) else {
            panic!("Unexpected event: {:?}", client_events);
        };
        let zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } = client_event else {
            panic!("Unexpected event: {:?}", client_event);
        };
        assert_eq!(dx, delta.x);
        assert_eq!(dy, delta.y);

        // And no Pointer Motion events
        while let Some(event) = client_events.pop() {
            match event {
                MouseEvents::Pointer(p_event) => match p_event {
                    wl_pointer::Event::Motion { .. } => panic!("Unexpected event: {:?}", p_event),
                    _ => {}
                },
                _ => {}
            }
        }
    }

    // Test pointer_motion_absolute()
    let absolute_position = Point::from((100.0, 150.0));
    f.server.pointer_motion_absolute(0, absolute_position);
    f.round_trip();

    {
        // Mouse shouldn't be moved!
        assert_eq!(f.server.pointer_location, expected_location);

        // But we should still get Relative mouse events
        let client_events = f.client.get_client_events();
        assert!(client_events.len() >= 2);
        let MouseEvents::Relative(client_event) = client_events.remove(0) else {
            panic!("Unexpected event: {:?}", client_events);
        };
        let zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } = client_event else {
            panic!("Unexpected event: {:?}", client_event);
        };
        assert_eq!(dx, absolute_position.x - f.server.pointer_location.x);
        assert_eq!(dy, absolute_position.y - f.server.pointer_location.y);

        // And no Pointer Motion events
        while let Some(event) = client_events.pop() {
            match event {
                MouseEvents::Pointer(p_event) => match p_event {
                    wl_pointer::Event::Motion { .. } => panic!("Unexpected event: {:?}", p_event),
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

#[test]
fn confine_mouse_absolute_movement() {
    let mut f = Fixture::new();
    f.round_trip();
    f.create_window(320, 240);

    // We start with the pointer at (0,0)
    let expected_location = Point::from((0.0, 0.0));
    f.server.pointer_motion_absolute(0, expected_location);
    f.round_trip();

    {
        let client_events = f.client.get_client_events();
        assert!(client_events.len() >= 1);
        clean_events(client_events);
    }

    // We create a confine region in the south right corner
    let _confine = f.client.confine_pointer(200, 100, 120, 140);
    let _relative_pointer = f.client.get_relative_pointer();
    f.round_trip();

    let outside_position = Point::from((50.0, 50.0));
    f.server.pointer_motion_absolute(0, outside_position);
    f.round_trip();
    {
        // The confinement shouldn't be active, we aren't in the region
        assert!(!f.client.is_confined());

        // The pointer has been moved correctly
        assert_eq!(f.server.pointer_location, outside_position);

        // Pointer Motion and Relative Motion events have been triggered
        let client_events = f.client.get_client_events();
        assert_eq!(client_events.len(), 3); // motion, relative_motion, frame
        while let Some(event) = client_events.pop() {
            match event {
                MouseEvents::Pointer(p_event) => match p_event {
                    wl_pointer::Event::Motion {
                        surface_x,
                        surface_y,
                        ..
                    } => {
                        assert_eq!(surface_x, outside_position.x);
                        assert_eq!(surface_y, outside_position.y);
                    }
                    _ => {}
                },
                MouseEvents::Relative(r_event) => match r_event {
                    zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } => {
                        assert_eq!(dx, outside_position.x);
                        assert_eq!(dy, outside_position.y);
                    }
                    _ => {}
                },
            }
        }
    }

    // Let's now move to the confined region
    let inside_position = Point::from((250.0, 150.0));
    f.server.pointer_motion_absolute(0, inside_position);
    f.round_trip();
    {
        // The confinement should have been activated now
        assert!(f.client.is_confined());

        // The pointer has been moved correctly
        assert_eq!(f.server.pointer_location, inside_position);

        // Pointer Motion and Relative Motion events have been triggered
        let client_events = f.client.get_client_events();
        assert_eq!(client_events.len(), 3); // motion, relative_motion, frame
        while let Some(event) = client_events.pop() {
            match event {
                MouseEvents::Pointer(p_event) => match p_event {
                    wl_pointer::Event::Motion {
                        surface_x,
                        surface_y,
                        ..
                    } => {
                        assert_eq!(surface_x, inside_position.x);
                        assert_eq!(surface_y, inside_position.y);
                    }
                    _ => {}
                },
                MouseEvents::Relative(r_event) => match r_event {
                    zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } => {
                        assert_eq!(dx, inside_position.x - outside_position.x);
                        assert_eq!(dy, inside_position.y - outside_position.y);
                    }
                    _ => {}
                },
            }
        }
    }

    // Now, we shouldn't be able to move back out to the confined region
    f.server.pointer_motion_absolute(0, outside_position);
    f.round_trip();
    {
        // The confinement should still be active
        assert!(f.client.is_confined());

        // The pointer is still where it was
        assert_eq!(f.server.pointer_location, inside_position);

        // We'll only get a relative motion event in this case
        // Pointer Motion and Relative Motion events have been triggered
        let client_events = f.client.get_client_events();
        assert_eq!(client_events.len(), 2); // relative_motion, frame
        while let Some(event) = client_events.pop() {
            match event {
                MouseEvents::Pointer(p_event) => match p_event {
                    wl_pointer::Event::Frame { .. } => {}
                    _ => {
                        panic!("Unexpected event: {:?}", p_event)
                    }
                },
                MouseEvents::Relative(r_event) => match r_event {
                    zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } => {
                        assert_eq!(dx, -(inside_position.x - outside_position.x));
                        assert_eq!(dy, -(inside_position.y - outside_position.y));
                    }
                    _ => {}
                },
            }
        }
    }
}

/// A client whose wl_pointer did not exist when pointer focus was first resolved must still
/// receive a wl_pointer.enter. The armed `pending_pointer_refocus` flag forces the next
/// motion to cycle smithay's focus (leave -> enter) instead of taking its same-target
/// motion-only arm.
#[test]
fn pending_refocus_re_emits_enter() {
    let mut f = Fixture::new();
    f.round_trip();
    f.create_window(320, 240);

    // Establish focus the normal way, then drop the events it produced.
    f.server
        .pointer_motion_absolute(0, Point::from((10.0, 10.0)));
    f.round_trip();
    clean_events(f.client.get_client_events());

    // Without the flag, a second motion is motion-only (see `move_mouse`). With it armed,
    // the client must see a fresh Enter.
    f.server.pending_pointer_refocus = true;
    let delta = Point::from((5.0, 5.0));
    f.server.pointer_motion(0, 0, delta, delta);
    f.round_trip();

    assert!(
        !f.server.pending_pointer_refocus,
        "flag must be consumed when a surface is under the pointer"
    );

    let client_events = f.client.get_client_events();
    assert!(
        client_events
            .iter()
            .any(|e| matches!(e, MouseEvents::Pointer(wl_pointer::Event::Enter { .. }))),
        "expected a re-emitted wl_pointer.enter, got: {:?}",
        client_events
    );
}

/// The flag is RETAINED when nothing is under the pointer, so a later motion can still
/// deliver the enter once a surface is mappable there.
#[test]
fn pending_refocus_retained_without_surface() {
    let mut f = Fixture::new();
    f.round_trip();
    // No window created: `Space::element_under` resolves to None.

    f.server.pending_pointer_refocus = true;
    let delta = Point::from((5.0, 5.0));
    f.server.pointer_motion(0, 0, delta, delta);
    f.round_trip();

    assert!(
        f.server.pending_pointer_refocus,
        "flag must survive a motion with no surface under the pointer"
    );
}

/// The flag is RETAINED while the pointer is grabbed. smithay's default click grab is live
/// for as long as a button is held, and forcing a leave mid-click-drag would break the drag,
/// so the refocus must wait for the button to be released.
#[test]
fn pending_refocus_retained_while_grabbed() {
    const BTN_LEFT: u32 = 0x110;

    let mut f = Fixture::new();
    f.round_trip();
    f.create_window(320, 240);

    // Establish focus the normal way, then drop the events it produced.
    f.server
        .pointer_motion_absolute(0, Point::from((10.0, 10.0)));
    f.round_trip();
    clean_events(f.client.get_client_events());

    // Press: smithay's DefaultGrab installs the click grab, so `pointer.is_grabbed()` is
    // true from here until the release below.
    f.server.pointer_button(0, BTN_LEFT, ButtonState::Pressed);
    f.round_trip();
    clean_events(f.client.get_client_events());

    f.server.pending_pointer_refocus = true;
    let delta = Point::from((5.0, 5.0));
    f.server.pointer_motion(0, 0, delta, delta);
    f.round_trip();

    // The retained flag is the observable proof that the grab guard fired: with no grab this
    // motion would have consumed it (see `pending_refocus_re_emits_enter`, same setup).
    assert!(
        f.server.pending_pointer_refocus,
        "flag must survive a motion while the pointer is grabbed"
    );

    let client_events = f.client.get_client_events();
    assert!(
        !client_events
            .iter()
            .any(|e| matches!(e, MouseEvents::Pointer(wl_pointer::Event::Leave { .. }))),
        "no forced leave may be sent mid-click-drag, got: {:?}",
        client_events
    );
    clean_events(client_events);

    // Release, and the very next motion may consume the flag as usual.
    f.server.pointer_button(0, BTN_LEFT, ButtonState::Released);
    f.round_trip();
    clean_events(f.client.get_client_events());

    f.server.pointer_motion(0, 0, delta, delta);
    f.round_trip();
    assert!(
        !f.server.pending_pointer_refocus,
        "flag must be consumed once the grab is released"
    );
}
