use agent_desktop_core::{AdapterError, ErrorCode, Point, Rect};

use super::super::cg_window::{
    bool_field, int_field, int_or_float_field, rect_field, string_field,
};
use super::{MAIN_MENU_LAYER, contains, stacked_window_dictionaries, window_at_point_until};

const ANOTHER_USER: &str = "<another user's window>";

fn deadline() -> std::time::Instant {
    std::time::Instant::now() + std::time::Duration::from_secs(5)
}

/// WindowServer and other system processes run as their own users and can
/// put a window on screen. Signalling such a process is refused with
/// `EPERM`, which tells another user's process apart without the identity
/// read the hit test itself uses.
fn owned_by_another_user(pid: i64) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    let signalled = unsafe { libc::kill(pid, 0) };
    signalled == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// The hit test fails closed on a window whose owner it cannot read.
fn refused_as_another_users(error: &AdapterError) -> bool {
    error.code == ErrorCode::PermDenied
        && error
            .details
            .as_ref()
            .and_then(|details| details["kind"].as_str())
            == Some("process_identity_permission")
}

#[test]
fn containment_includes_the_origin_edge_and_excludes_the_far_edge() {
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
    };
    assert!(contains(&bounds, &Point { x: 10.0, y: 20.0 }));
    assert!(contains(&bounds, &Point { x: 109.9, y: 69.9 }));
    assert!(!contains(&bounds, &Point { x: 110.0, y: 30.0 }));
    assert!(!contains(&bounds, &Point { x: 50.0, y: 70.0 }));
    assert!(!contains(&bounds, &Point { x: 9.9, y: 30.0 }));
}

#[test]
fn a_point_beyond_every_display_has_no_receiving_window() {
    let receiver = window_at_point_until(
        &Point {
            x: -9_000_000.0,
            y: -9_000_000.0,
        },
        deadline(),
    )
    .expect("the window inventory reads");
    assert!(receiver.is_none(), "{receiver:?}");
}

/// The frontmost visible window owns the centre of its own frame, so a hit
/// test there must answer with that window's application, or refuse when
/// that application runs as another user. The menu bar is judged as the
/// frontmost app instead, which the grid test covers.
#[test]
fn the_frontmost_window_receives_its_own_centre() {
    let own = std::process::id() as i64;
    let Some((owner, pid, centre)) = stacked_window_dictionaries()
        .expect("window inventory")
        .into_iter()
        .find_map(|dictionary| {
            let onscreen = bool_field(&dictionary, "kCGWindowIsOnscreen").ok()?;
            let bounds = rect_field(&dictionary, "kCGWindowBounds")?;
            let owner = string_field(&dictionary, "kCGWindowOwnerName")?;
            let pid = int_field(&dictionary, "kCGWindowOwnerPID")?;
            let see_through = int_or_float_field(&dictionary, "kCGWindowAlpha") == Some(0.0);
            let menu_bar = int_field(&dictionary, "kCGWindowLayer") == Some(MAIN_MENU_LAYER);
            (onscreen
                && !see_through
                && !menu_bar
                && bounds.width > 1.0
                && bounds.height > 1.0
                && pid != own)
                .then(|| {
                    (
                        owner,
                        pid,
                        Point {
                            x: bounds.x + bounds.width / 2.0,
                            y: bounds.y + bounds.height / 2.0,
                        },
                    )
                })
        })
    else {
        return;
    };
    match window_at_point_until(&centre, deadline()) {
        Ok(receiver) => assert_eq!(receiver.expect("a window owns the centre").app, owner),
        Err(error) => assert!(
            refused_as_another_users(&error) && owned_by_another_user(pid),
            "only another user's window is refused: {owner} ({pid}): {error:?}"
        ),
    }
}

/// Windows overlap, so the receiver at a point is the frontmost window
/// there, which only the on-screen listing orders. Across a grid over the
/// screen the hit test must agree with that listing's first window
/// containing each point (the menu bar standing for the frontmost app);
/// the full listing (on- and off-screen) is in another order and named
/// covered windows, or a window behind a cover.
#[test]
fn every_point_is_judged_by_the_frontmost_window_over_it() {
    let mut mismatch = None;
    for _ in 0..3 {
        match first_mismatch_on_the_grid() {
            None => return,
            Some(found) => mismatch = Some(found),
        }
    }
    panic!("three rounds disagreed with the stacked listing: {mismatch:?}");
}

/// One round against a fresh listing; a window that moves during the round
/// can disagree once, a hit test in the wrong order disagrees every round.
/// Over another user's window the hit test may name the owner, when it can
/// read the owner's identity (root's `loginwindow`), or refuse.
fn first_mismatch_on_the_grid() -> Option<String> {
    let own = std::process::id() as i64;
    let stacked: Vec<_> = stacked_window_dictionaries()
        .expect("window inventory")
        .into_iter()
        .filter_map(|dictionary| {
            let bounds = rect_field(&dictionary, "kCGWindowBounds")?;
            let owner = string_field(&dictionary, "kCGWindowOwnerName")?;
            let pid = int_field(&dictionary, "kCGWindowOwnerPID")?;
            let see_through = int_or_float_field(&dictionary, "kCGWindowAlpha") == Some(0.0);
            let menu_bar = int_field(&dictionary, "kCGWindowLayer") == Some(MAIN_MENU_LAYER);
            let another_user = owned_by_another_user(pid);
            (!see_through && pid > 0 && pid != own).then_some((
                owner,
                bounds,
                menu_bar,
                another_user,
            ))
        })
        .collect();
    let frontmost = super::super::window_inventory::focused_window_until(deadline())
        .expect("focused window")
        .map(|window| window.app);
    for x in (0..2000).step_by(97) {
        for y in (0..1200).step_by(83) {
            let point = Point {
                x: f64::from(x),
                y: f64::from(y),
            };
            let over = stacked
                .iter()
                .find(|(_, bounds, _, _)| contains(bounds, &point));
            let expected = over.and_then(|(owner, _, menu_bar, _)| {
                if *menu_bar {
                    frontmost.clone()
                } else {
                    Some(owner.clone())
                }
            });
            let refusal_allowed =
                over.is_some_and(|(_, _, menu_bar, another_user)| *another_user && !*menu_bar);
            let actual = match window_at_point_until(&point, deadline()) {
                Ok(window) => window.map(|window| window.app),
                Err(error) if refused_as_another_users(&error) => Some(ANOTHER_USER.to_string()),
                Err(error) => panic!("hit test at ({x}, {y}): {error:?}"),
            };
            let refused = actual.as_deref() == Some(ANOTHER_USER);
            if actual != expected && !(refused && refusal_allowed) {
                return Some(format!("at ({x}, {y}): {actual:?} != {expected:?}"));
            }
        }
    }
    None
}
