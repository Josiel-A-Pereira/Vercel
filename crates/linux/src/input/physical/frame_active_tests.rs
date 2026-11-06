use super::active_of;
use crate::tree::Field;
use crate::tree::live::test_support::words;

#[test]
fn a_state_set_with_the_active_bit_is_active() {
    assert_eq!(active_of(&Field::Present(words(&[1, 25]))), Some(true));
}

#[test]
fn a_state_set_without_the_active_bit_is_inactive() {
    assert_eq!(active_of(&Field::Present(words(&[8, 25]))), Some(false));
}

#[test]
fn an_unreadable_state_set_is_unknown_and_not_inactive() {
    assert_eq!(active_of(&Field::Failed("gone".to_owned())), None);
    assert_eq!(active_of(&Field::Absent), None);
}

#[test]
fn the_active_bit_is_read_from_its_own_position_only() {
    assert_eq!(active_of(&Field::Present(words(&[33]))), Some(false));
}

use super::ActiveGate;
use crate::actions::physical_target::tests::fake::{
    FakeWorld, check_of, deadline, element, not_delivered,
};

#[test]
fn the_first_check_of_an_inactive_frame_refuses_before_any_key() {
    let world = FakeWorld::x11().active_script(&[Some(false)]);
    let target = element();
    let error = ActiveGate::new(&world, &target, deadline())
        .check()
        .unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert!(not_delivered(&error));
    assert!(error.suggestion.as_deref().unwrap().contains("dialog"));
}

#[test]
fn a_later_check_that_finds_the_frame_inactive_does_not_claim_nothing_was_sent() {
    let world = FakeWorld::x11().active_script(&[Some(true), Some(false)]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    let error = gate.check().unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert!(!not_delivered(&error));
    assert_eq!(world.count("pause"), 0);
}

#[test]
fn an_unreadable_frame_refuses_with_its_cause_before_any_key() {
    let world = FakeWorld::x11().active_script(&[None]);
    let target = element();
    let error = ActiveGate::new(&world, &target, deadline())
        .check()
        .unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert!(not_delivered(&error));
    assert_eq!(error.details.expect("details")["cause"], "unreadable");
}

#[test]
fn a_frame_the_compositor_does_not_call_activated_refuses_before_any_key() {
    let world = FakeWorld::wayland().compositor_script(&[Some(false)]);
    let target = element();
    let error = ActiveGate::new(&world, &target, deadline())
        .check()
        .unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert!(not_delivered(&error));
    assert_eq!(
        error.details.expect("details")["cause"],
        "compositor_not_activated"
    );
    assert!(world.count("pause") > 0);
}

#[test]
fn the_first_check_waits_for_a_compositor_that_lags_the_frame() {
    let world = FakeWorld::wayland().compositor_script(&[Some(false), Some(false), Some(true)]);
    let target = element();
    ActiveGate::new(&world, &target, deadline())
        .check()
        .expect("the compositor agrees on the third read");
    assert_eq!(world.count("read_compositor"), 3);
    assert_eq!(world.count("pause"), 2);
}

#[test]
fn a_later_check_reads_the_compositor_once_and_stops_the_delivery() {
    let world = FakeWorld::wayland().compositor_script(&[Some(true), Some(false)]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    let error = gate.check().unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert!(!not_delivered(&error));
    assert_eq!(
        error.details.expect("details")["cause"],
        "compositor_not_activated"
    );
    assert_eq!(world.count("read_compositor"), 2);
    assert_eq!(world.count("pause"), 0);
}

#[test]
fn a_session_without_a_compositor_window_list_leaves_the_frame_as_the_whole_proof() {
    let world = FakeWorld::wayland();
    let target = element();
    ActiveGate::new(&world, &target, deadline())
        .check()
        .expect("the frame is active and no compositor list exists");
    assert_eq!(world.count("read_compositor"), 1);
    assert_eq!(world.count("pause"), 0);
}

#[test]
fn the_compositor_is_asked_only_after_the_frame_reads_active() {
    let world = FakeWorld::wayland()
        .active_script(&[Some(false)])
        .compositor_script(&[Some(true)]);
    let target = element();
    let error = ActiveGate::new(&world, &target, deadline())
        .check()
        .unwrap_err();
    assert_eq!(error.details.expect("details")["cause"], "not_active");
    assert_eq!(world.count("read_compositor"), 0);
}

#[test]
fn the_first_check_pins_the_proven_toplevel_once_and_later_checks_use_only_the_pin() {
    let world = FakeWorld::wayland()
        .compositor_script(&[Some(true)])
        .watch_script(&[Some(true)]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    for _ in 0..5 {
        gate.check().expect("the pinned toplevel stays active");
    }
    assert_eq!(world.count("open_watch"), 1);
    assert_eq!(world.count("read_compositor"), 1);
    assert_eq!(world.watch_asked(), 4);
}

#[test]
fn a_title_that_changes_after_the_first_check_does_not_stop_the_delivery() {
    let world = FakeWorld::wayland()
        .compositor_script(&[Some(true), Some(false)])
        .watch_script(&[Some(true)]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    gate.check()
        .expect("a title read would now fail; the pin ignores it");
    assert_eq!(world.count("read_compositor"), 1);
}

#[test]
fn a_pinned_toplevel_that_is_no_longer_activated_refuses_with_the_lost_shape() {
    let world = FakeWorld::wayland()
        .compositor_script(&[Some(true)])
        .watch_script(&[Some(true), Some(false)]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    gate.check().unwrap();
    let error = gate.check().unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert!(!not_delivered(&error));
    assert_eq!(
        error.details.expect("details")["cause"],
        "compositor_not_activated"
    );
}

#[test]
fn a_pinned_toplevel_that_is_gone_refuses_like_one_that_lost_activation() {
    let world = FakeWorld::wayland()
        .compositor_script(&[Some(true)])
        .watch_script(&[Some(false)]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    let error = gate.check().unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert_eq!(
        error.details.expect("details")["cause"],
        "compositor_not_activated"
    );
}

#[test]
fn a_watch_whose_link_fails_stops_the_delivery_with_an_error_not_a_pass() {
    let world = FakeWorld::wayland()
        .compositor_script(&[Some(true)])
        .watch_script(&[None]);
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    assert!(gate.check().is_err());
}

#[test]
fn no_pin_is_opened_when_the_first_check_refuses() {
    let world = FakeWorld::wayland()
        .compositor_script(&[Some(false)])
        .watch_script(&[Some(true)]);
    let target = element();
    ActiveGate::new(&world, &target, deadline())
        .check()
        .unwrap_err();
    assert_eq!(world.count("open_watch"), 0);
}

#[test]
fn a_session_without_a_window_list_keeps_the_frame_proof_and_opens_no_pin() {
    let world = FakeWorld::wayland();
    let target = element();
    let mut gate = ActiveGate::new(&world, &target, deadline());
    gate.check().unwrap();
    gate.check().unwrap();
    assert_eq!(world.count("open_watch"), 0);
    assert_eq!(world.watch_asked(), 0);
}
