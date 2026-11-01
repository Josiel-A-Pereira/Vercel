use serde_json::json;

use super::*;
use crate::system::window::foreign::foreign_fake::{cause, deadline};

fn not_delivered() -> DeliverySemantics {
    DeliverySemantics::not_delivered()
}

fn details(error: &AdapterError) -> &Value {
    error.details.as_ref().expect("a refusal carries details")
}

fn shown() -> ToplevelFlags {
    ToplevelFlags {
        activated: true,
        ..ToplevelFlags::default()
    }
}

#[test]
fn several_toplevels_are_an_ambiguous_target_that_names_the_candidates() {
    let error = ambiguous(3);
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(cause(&error), "ambiguous_window");
    assert_eq!(details(&error)["candidates"], 3);
    assert_eq!(details(&error)["physical_delivery_started"], false);
    assert_eq!(error.disposition, not_delivered());
    assert!(error.message.contains("names no process"));
    assert!(error.suggestion.is_some());
}

#[test]
fn an_unrelated_title_holder_lists_the_app_ids_it_saw() {
    let error = unrelated(&["kitty".to_owned(), "foot".to_owned()]);
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "toplevel_identity_unproven");
    assert_eq!(details(&error)["app_ids"], json!(["kitty", "foot"]));
    assert_eq!(details(&error)["physical_delivery_started"], false);
    assert_eq!(error.disposition, not_delivered());
}

#[test]
fn a_missing_title_is_window_not_found_and_points_at_list_windows() {
    let error = no_window();
    assert_eq!(error.code, ErrorCode::WindowNotFound);
    assert_eq!(cause(&error), "window_not_mapped");
    assert_eq!(details(&error)["physical_delivery_started"], false);
    assert_eq!(error.disposition, not_delivered());
    assert!(
        error
            .suggestion
            .is_some_and(|text| text.contains("list-windows"))
    );
}

#[test]
fn move_and_resize_are_unsupported_on_the_wayland_session() {
    let error = move_or_resize();
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(cause(&error), "wayland_session");
    assert_eq!(details(&error)["physical_delivery_started"], false);
    assert_eq!(error.disposition, not_delivered());
    assert!(error.message.contains("no move or resize"));
}

#[test]
fn each_proof_maps_to_its_own_refusal() {
    assert_eq!(
        cause(&unproven(&Proof::Several(vec![1, 2]))),
        "ambiguous_window"
    );
    assert_eq!(cause(&unproven(&Proof::NoWindow)), "window_not_mapped");
    assert_eq!(
        cause(&unproven(&Proof::Unrelated(vec!["kitty".to_owned()]))),
        "toplevel_identity_unproven"
    );
    assert_eq!(unproven(&Proof::One(1)).code, ErrorCode::Internal);
}

#[test]
fn a_readback_mismatch_reports_what_was_asked_and_what_was_seen() {
    let error = not_applied(ToplevelRequest::SetMaximized(true), &shown());
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "readback_mismatch");
    assert_eq!(details(&error)["requested"], json!({ "maximized": true }));
    assert_eq!(details(&error)["observed"]["activated"], true);
    assert_eq!(details(&error)["observed"]["maximized"], false);
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
}

#[test]
fn a_minimize_that_left_the_window_shown_has_its_own_cause() {
    let error = minimize_not_applied(&shown());
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "minimize_not_applied");
    assert_eq!(details(&error)["requested"], json!({ "minimized": true }));
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert!(error.message.contains("left the window shown"));
    assert!(
        error
            .suggestion
            .is_some_and(|text| text.contains("compositor's own controls"))
    );
}

#[test]
fn a_window_closed_during_the_check_is_delivered_unverified() {
    let error = window_destroyed();
    assert_eq!(error.code, ErrorCode::WindowNotFound);
    assert_eq!(cause(&error), "window_destroyed");
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
}

#[test]
fn a_timed_out_port_call_is_a_timeout_that_delivered_nothing() {
    let error = control_failure(ControlError::TimedOut, deadline());
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, not_delivered());
}

#[test]
fn a_failed_flush_is_uncertain() {
    let error = control_failure(ControlError::SendFailed, deadline());
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "send_failed");
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
}

#[test]
fn an_unknown_or_stale_id_is_window_not_found_and_nothing_was_sent() {
    for failure in [ControlError::BadWindow, ControlError::StaleWindow] {
        let error = control_failure(failure, deadline());
        assert_eq!(error.code, ErrorCode::WindowNotFound);
        assert_eq!(cause(&error), "window_not_mapped");
        assert_eq!(error.disposition, not_delivered());
    }
}

#[test]
fn a_missing_connection_or_seat_is_action_failed_and_nothing_was_sent() {
    let error = control_failure(ControlError::NoDisplay, deadline());
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "no_display");
    assert_eq!(error.disposition, not_delivered());
}
