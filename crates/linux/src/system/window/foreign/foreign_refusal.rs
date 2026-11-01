use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};
use serde_json::{Value, json};

use super::foreign_flags::ToplevelFlags;
use super::foreign_proof::Proof;
use super::foreign_request::ToplevelRequest;
use crate::system::x11::control::control_error::ControlError;

const RUN_LIST_WINDOWS: &str = "Run 'list-windows' to confirm the window, then retry";

fn refused(code: ErrorCode, cause: &str, message: &str) -> AdapterError {
    AdapterError::new(code, message)
        .with_details(json!({ "cause": cause, "physical_delivery_started": false }))
        .with_disposition(DeliverySemantics::not_delivered())
}

fn with_extra(mut error: AdapterError, key: &str, value: Value) -> AdapterError {
    if let Some(details) = error.details.as_mut().and_then(Value::as_object_mut) {
        details.insert(key.to_owned(), value);
    }
    error
}

fn flags_of(flags: &ToplevelFlags) -> Value {
    json!({
        "maximized": flags.maximized,
        "minimized": flags.minimized,
        "activated": flags.activated,
        "fullscreen": flags.fullscreen,
    })
}

pub(in crate::system) fn ambiguous(candidates: usize) -> AdapterError {
    AdapterError::ambiguous_target(
        "More than one toplevel has this title and belongs to this program, and the compositor names no process, so none is changed",
    )
    .with_suggestion("Give the windows different titles, then retry")
    .with_details(json!({
        "cause": "ambiguous_window",
        "physical_delivery_started": false,
        "candidates": candidates,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

pub(in crate::system) fn unrelated(app_ids: &[String]) -> AdapterError {
    let error = refused(
        ErrorCode::ActionFailed,
        "toplevel_identity_unproven",
        "The only toplevel with this title has an app id unrelated to this process, so none is changed",
    )
    .with_suggestion(RUN_LIST_WINDOWS);
    with_extra(error, "app_ids", json!(app_ids))
}

pub(in crate::system) fn no_window() -> AdapterError {
    refused(
        ErrorCode::WindowNotFound,
        "window_not_mapped",
        "No toplevel with this title is open on the compositor",
    )
    .with_suggestion("Run 'list-windows' to see the open windows")
}

pub(in crate::system) fn move_or_resize() -> AdapterError {
    refused(
        ErrorCode::ActionNotSupported,
        "wayland_session",
        "The compositor's toplevel protocol has no move or resize request",
    )
    .with_suggestion("Move or resize the window with the compositor's own controls")
}

/// The refusal for a toplevel list that did not prove exactly one window.
pub(in crate::system) fn unproven(proof: &Proof) -> AdapterError {
    match proof {
        Proof::One(_) => AdapterError::internal("A proven toplevel was refused"),
        Proof::Several(ids) => ambiguous(ids.len()),
        Proof::NoWindow => no_window(),
        Proof::Unrelated(app_ids) => unrelated(app_ids),
    }
}

fn after_send(error: AdapterError) -> AdapterError {
    error.with_disposition(DeliverySemantics::delivered_unverified())
}

pub(in crate::system) fn not_applied(
    request: ToplevelRequest,
    observed: &ToplevelFlags,
) -> AdapterError {
    after_send(
        AdapterError::new(
            ErrorCode::ActionFailed,
            "The compositor left the window other than requested",
        )
        .with_suggestion("Run 'list-windows' to see the window as it is now")
        .with_details(json!({
            "cause": "readback_mismatch",
            "requested": request.requested(),
            "observed": flags_of(observed),
        })),
    )
}

pub(in crate::system) fn minimize_not_applied(observed: &ToplevelFlags) -> AdapterError {
    after_send(
        AdapterError::new(
            ErrorCode::ActionFailed,
            "The compositor took the minimize request and left the window shown",
        )
        .with_suggestion("Operate the window from the compositor's own controls")
        .with_details(json!({
            "cause": "minimize_not_applied",
            "requested": ToplevelRequest::SetMinimized(true).requested(),
            "observed": flags_of(observed),
        })),
    )
}

pub(in crate::system) fn window_destroyed() -> AdapterError {
    after_send(
        AdapterError::new(
            ErrorCode::WindowNotFound,
            "The window closed while the request was being checked",
        )
        .with_suggestion(RUN_LIST_WINDOWS)
        .with_details(json!({ "cause": "window_destroyed" })),
    )
}

/// The error of a failed port call made before anything was sent.
pub(in crate::system) fn control_failure(error: ControlError, deadline: Deadline) -> AdapterError {
    match error {
        ControlError::TimedOut => deadline
            .timeout_error()
            .with_disposition(DeliverySemantics::not_delivered()),
        ControlError::SendFailed => AdapterError::new(
            ErrorCode::ActionFailed,
            "The request could not be flushed to the compositor, so its effect is unknown",
        )
        .with_suggestion(RUN_LIST_WINDOWS)
        .with_details(json!({ "cause": "send_failed" }))
        .with_disposition(DeliverySemantics::uncertain()),
        ControlError::BadWindow | ControlError::StaleWindow => {
            no_window().with_suggestion(RUN_LIST_WINDOWS)
        }
        ControlError::NoDisplay => refused(
            ErrorCode::ActionFailed,
            "no_display",
            "The compositor connection is unavailable, so nothing was sent",
        )
        .with_suggestion("Check that the compositor is running, then retry"),
    }
}

#[cfg(test)]
#[path = "foreign_refusal_tests.rs"]
mod tests;
