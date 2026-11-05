//! Global UIA focus and foreground-window evidence around a selection write.

use super::{FocusEffect, pre_write_error};
use crate::tree::automation::{automation_client, uia_error};
use crate::tree::element::UIAElement;
use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

#[derive(Clone)]
pub(super) struct GlobalFocus {
    focused: UIAElement,
    foreground: isize,
}

pub(super) fn capture_global_focus(deadline: Deadline) -> Result<GlobalFocus, AdapterError> {
    deadline.ensure_remaining()?;
    let foreground_before = unsafe { GetForegroundWindow() } as isize;
    if foreground_before == 0 {
        return Err(pre_write_error(
            "The foreground window could not be identified",
        ));
    }
    let client = automation_client()?;
    let focused = client.get_focused_element().map_err(|error| {
        pre_write_error("The globally focused element could not be identified")
            .with_platform_detail(error.to_string())
    })?;
    let foreground_after = unsafe { GetForegroundWindow() } as isize;
    if foreground_after == 0 || foreground_after != foreground_before {
        return Err(pre_write_error(
            "The foreground window changed while global focus was read",
        ));
    }
    deadline.ensure_remaining()?;
    Ok(GlobalFocus {
        focused: UIAElement::from(focused),
        foreground: foreground_before,
    })
}

pub(super) fn compare_global_focus(
    before: &GlobalFocus,
    after: &GlobalFocus,
) -> Result<FocusEffect, AdapterError> {
    let client = automation_client()?;
    let focus_same = client
        .compare_elements(&before.focused.0, &after.focused.0)
        .map_err(|error| {
            uia_error(&error, "compare globally focused elements")
                .with_disposition(DeliverySemantics::delivered_unverified())
        })?;
    Ok(FocusEffect {
        focus_changed: !focus_same,
        activated: before.foreground != after.foreground,
    })
}

pub(super) fn focus_side_effect_error(effect: FocusEffect) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Text selection changed global focus or brought the application forward",
    )
    .with_suggestion("Inspect the live selection before retrying; the write may have succeeded with a focus side effect.")
    .with_details(serde_json::json!({
        "verification_scope": "text_selection",
        "focus_changed": effect.focus_changed,
        "activated": effect.activated,
        "focus_evidence": "observed",
    }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

pub(super) fn focus_changed_before_write(effect: FocusEffect) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Global focus changed before text selection was written",
    )
    .with_suggestion("Inspect the active application before retrying text selection.")
    .with_details(serde_json::json!({
        "verification_scope": "text_selection",
        "focus_changed": effect.focus_changed,
        "activated": effect.activated,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

pub(super) fn attach_focus_evidence(
    error: AdapterError,
    focus_before: &GlobalFocus,
    deadline: Deadline,
) -> AdapterError {
    if has_focus_evidence(&error) {
        return error;
    }
    let effect = if deadline.is_expired() {
        None
    } else {
        capture_global_focus(deadline)
            .ok()
            .and_then(|focus_after| compare_global_focus(focus_before, &focus_after).ok())
    };
    with_focus_evidence(error, effect)
}

fn has_focus_evidence(error: &AdapterError) -> bool {
    error
        .details
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .is_some_and(|details| {
            details.contains_key("focus_changed") && details.contains_key("activated")
        })
}

pub(super) fn with_focus_evidence(
    mut error: AdapterError,
    effect: Option<FocusEffect>,
) -> AdapterError {
    let mut details = match error.details.take() {
        Some(serde_json::Value::Object(details)) => serde_json::Value::Object(details),
        Some(cause) => serde_json::json!({ "cause": cause }),
        None => serde_json::json!({}),
    };
    if let Some(object) = details.as_object_mut() {
        let (focus_changed, activated, evidence) = match effect {
            Some(effect) => (
                serde_json::Value::Bool(effect.focus_changed),
                serde_json::Value::Bool(effect.activated),
                "observed",
            ),
            None => (serde_json::Value::Null, serde_json::Value::Null, "unknown"),
        };
        object.insert("focus_changed".into(), focus_changed);
        object.insert("activated".into(), activated);
        object.insert("focus_evidence".into(), evidence.into());
    }
    error.with_details(details)
}
