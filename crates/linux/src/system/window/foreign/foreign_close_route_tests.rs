use agent_desktop_core::{AdapterError, ErrorCode};

use super::*;
use crate::system::window::foreign::foreign_fake::{
    FakePort, deadline, ignores, obeys, record, thunar,
};
use crate::system::window::foreign::foreign_record::ToplevelRecord;

fn frames(titles: &[&str]) -> Result<Vec<Frame>, ControlError> {
    Ok(titles
        .iter()
        .map(|title| Frame {
            title: (*title).to_owned(),
            prompt: false,
        })
        .collect())
}

fn ids(windows: &[Toplevel]) -> Vec<u32> {
    windows.iter().map(|window| window.id).collect()
}

fn raises_a_save_prompt(records: &mut Vec<ToplevelRecord>, _: u32, request: ToplevelRequest) {
    if request == ToplevelRequest::Close {
        records.push(record(9, "Save changes?", "thunar"));
    }
}

fn raises_a_prompt_titled_like_its_window(
    records: &mut Vec<ToplevelRecord>,
    _: u32,
    request: ToplevelRequest,
) {
    if request == ToplevelRequest::Close {
        records.push(record(9, "Home", "thunar"));
    }
}

fn read(route: &ForeignCloseRoute<FakePort>, shown: &[&str]) -> Vec<Toplevel> {
    route
        .standing(&thunar(), || frames(shown), deadline())
        .expect("a read")
}

#[test]
fn a_session_without_a_manager_has_no_route_and_a_failed_open_is_retried() {
    let route = ForeignCloseRoute::<FakePort>::default();
    assert!(!route.reach(|| Ok(None)).expect("no manager"));
    assert!(!route.in_use());
    let late = ForeignCloseRoute::<FakePort>::default();
    let error = late
        .reach(|| Err(AdapterError::new(ErrorCode::Timeout, "late")))
        .expect_err("a missed deadline");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(
        late.reach(|| Ok(Some(FakePort::showing(Vec::new()))))
            .expect("opens on the next call")
    );
}

#[test]
fn the_link_is_opened_once() {
    let route = ForeignCloseRoute::over(FakePort::showing(Vec::new()));
    assert!(
        route
            .reach(|| panic!("opened a second link"))
            .expect("kept")
    );
}

#[test]
fn a_read_without_a_link_is_a_dead_connection() {
    let route = ForeignCloseRoute::<FakePort>::default();
    let outcome = route.standing(&thunar(), || frames(&["Home"]), deadline());
    assert_eq!(outcome, Err(ControlError::NoDisplay));
    assert_eq!(route.close(1, deadline()), Err(ControlError::NoDisplay));
}

#[test]
fn every_read_refreshes_the_link_first() {
    let port = FakePort::showing(vec![record(1, "Home", "thunar")]).then_showing(vec![
        record(1, "Home", "thunar"),
        record(2, "Docs", "thunar"),
    ]);
    let route = ForeignCloseRoute::over(port);
    assert_eq!(ids(&read(&route, &["Home", "Docs"])), vec![1, 2]);
    assert_eq!(ids(&read(&route, &["Home", "Docs"])), vec![1, 2]);
}

#[test]
fn a_refresh_that_fails_is_an_error_not_a_stale_list() {
    for error in [ControlError::TimedOut, ControlError::NoDisplay] {
        let port = FakePort::showing(vec![record(1, "Home", "thunar")]).failing_refresh(error);
        let route = ForeignCloseRoute::over(port);
        let outcome = route.standing(&thunar(), || frames(&["Home"]), deadline());
        assert_eq!(outcome, Err(error));
        assert!(!route.in_use());
    }
}

#[test]
fn two_windows_with_different_titles_are_both_listed_and_both_closed() {
    let port = FakePort::showing(vec![
        record(1, "Home", "thunar"),
        record(2, "Docs", "thunar"),
    ]);
    let route = ForeignCloseRoute::over(port);
    let windows = read(&route, &["Home", "Docs"]);
    assert_eq!(ids(&windows), vec![1, 2]);
    for window in &windows {
        route.close(window.id, deadline()).expect("sent");
    }
    assert!(route.in_use());
    assert_eq!(route.contested(), 0);
}

#[test]
fn a_contested_title_is_counted_and_none_of_it_is_listed() {
    let port = FakePort::showing(vec![
        record(1, "Home", "thunar"),
        record(2, "Home", "thunar"),
    ]);
    let route = ForeignCloseRoute::over(port);
    assert!(read(&route, &["Home"]).is_empty());
    assert_eq!(route.contested(), 1);
}

#[test]
fn a_related_toplevel_on_screen_before_any_close_is_not_listed() {
    let port = FakePort::showing(vec![
        record(1, "Home", "thunar"),
        record(2, "Other", "thunar"),
    ])
    .reacting(ignores);
    let route = ForeignCloseRoute::over(port);
    assert_eq!(ids(&read(&route, &["Home"])), vec![1]);
    route.close(1, deadline()).expect("sent");
    assert_eq!(ids(&read(&route, &["Home"])), vec![1]);
}

#[test]
fn the_toplevel_a_close_raises_is_a_dialog_and_is_never_sent_a_close() {
    let port = FakePort::showing(vec![record(1, "Home", "thunar")]).reacting(raises_a_save_prompt);
    let route = ForeignCloseRoute::over(port);
    assert_eq!(ids(&read(&route, &["Home"])), vec![1]);
    route.close(1, deadline()).expect("sent");
    let windows = read(&route, &["Home"]);
    assert_eq!(ids(&windows), vec![1, 9]);
    assert_eq!(windows[0].kind, WindowKind::Normal);
    assert_eq!(windows[1].kind, WindowKind::Dialog);
    assert_eq!(route.close(9, deadline()), Err(ControlError::BadWindow));
    assert_eq!(route.close(9, deadline()), Err(ControlError::BadWindow));
    let sent = route.port().expect("a port").sent();
    assert_eq!(sent, vec![(1, ToplevelRequest::Close)]);
}

#[test]
fn a_prompt_that_shows_up_as_a_frame_is_still_a_dialog() {
    let port = FakePort::showing(vec![record(1, "Home", "thunar")]).reacting(raises_a_save_prompt);
    let route = ForeignCloseRoute::over(port);
    assert_eq!(ids(&read(&route, &["Home"])), vec![1]);
    route.close(1, deadline()).expect("sent");
    let windows = read(&route, &["Home", "Save changes?"]);
    assert_eq!(ids(&windows), vec![1, 9]);
    assert_eq!(windows[1].kind, WindowKind::Dialog);
}

#[test]
fn the_ids_on_screen_are_recorded_once_at_the_first_close() {
    let port = FakePort::showing(vec![
        record(1, "Home", "thunar"),
        record(2, "Docs", "thunar"),
    ])
    .reacting(obeys);
    let route = ForeignCloseRoute::over(port);
    read(&route, &["Home", "Docs"]);
    route.close(1, deadline()).expect("sent");
    route.close(2, deadline()).expect("sent");
    assert_eq!(*route.known.borrow(), vec![1, 2]);
}

#[test]
fn a_window_that_closed_meanwhile_is_a_bad_window_and_a_failed_flush_is_reported() {
    let route = ForeignCloseRoute::over(FakePort::showing(vec![record(1, "Home", "thunar")]));
    read(&route, &["Home"]);
    assert_eq!(route.close(7, deadline()), Err(ControlError::BadWindow));
    let broken = ForeignCloseRoute::over(
        FakePort::showing(vec![record(1, "Home", "thunar")]).failing_send(ControlError::SendFailed),
    );
    read(&broken, &["Home"]);
    assert_eq!(broken.close(1, deadline()), Err(ControlError::SendFailed));
}

#[test]
fn no_frames_means_nothing_is_listed_and_the_titles_are_read_again_next_time() {
    let route = ForeignCloseRoute::over(FakePort::showing(vec![record(1, "Home", "thunar")]));
    assert!(read(&route, &[]).is_empty());
    assert_eq!(ids(&read(&route, &["Home"])), vec![1]);
}

#[test]
fn a_prompt_titled_like_its_window_is_a_dialog_not_a_contested_title() {
    let port = FakePort::showing(vec![record(1, "Home", "thunar")])
        .reacting(raises_a_prompt_titled_like_its_window);
    let route = ForeignCloseRoute::over(port);
    assert_eq!(ids(&read(&route, &["Home"])), vec![1]);
    route.close(1, deadline()).expect("sent");
    let windows = read(&route, &["Home"]);
    assert_eq!(ids(&windows), vec![1, 9]);
    assert_eq!(windows[1].kind, WindowKind::Dialog);
    assert_eq!(route.contested(), 0);
    assert_eq!(route.close(9, deadline()), Err(ControlError::BadWindow));
}

#[test]
fn a_prompt_already_open_at_the_first_read_is_never_sent_a_close() {
    let port = FakePort::showing(vec![
        record(1, "Home", "thunar"),
        record(2, "Save changes?", "thunar"),
    ]);
    let route = ForeignCloseRoute::over(port);
    let windows = route
        .standing(
            &thunar(),
            || {
                Ok(vec![
                    Frame {
                        title: "Home".to_owned(),
                        prompt: false,
                    },
                    Frame {
                        title: "Save changes?".to_owned(),
                        prompt: true,
                    },
                ])
            },
            deadline(),
        )
        .expect("a read");
    assert_eq!(windows[1].kind, WindowKind::Dialog);
    assert_eq!(route.close(2, deadline()), Err(ControlError::BadWindow));
    assert!(route.port().expect("a port").sent().is_empty());
}
