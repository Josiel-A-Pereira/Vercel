use std::cell::Cell;

use accessibility_sys::AXUIElementCreateSystemWide;
use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};

use super::{FocusSnapshot, after_uncertain_write};
use crate::tree::AXElement;

fn focus_snapshot(element: AXElement) -> FocusSnapshot {
    FocusSnapshot {
        frontmost_pid: 406,
        frontmost_launch_time: Some(1.0),
        application: element.clone(),
        window: Some(element.clone()),
        focused: Some(element),
    }
}

fn uncertain_setter_error() -> AdapterError {
    AdapterError::new(
        ErrorCode::AppUnresponsive,
        "Selection setter outcome is uncertain",
    )
    .with_details(serde_json::json!({"ax_error": -25204}))
    .with_platform_detail("AXUIElementSetAttributeValue returned -25204")
    .with_disposition(DeliverySemantics::uncertain())
}

#[test]
fn uncertain_setter_error_records_unknown_focus_when_reread_fails() {
    let before = focus_snapshot(AXElement(unsafe { AXUIElementCreateSystemWide() }));
    let error = after_uncertain_write(uncertain_setter_error(), &before, || {
        Err(AdapterError::timeout("focus read failed"))
    });

    assert_eq!(error.code, ErrorCode::AppUnresponsive);
    assert_eq!(error.message, "Selection setter outcome is uncertain");
    assert_eq!(
        error.platform_detail.as_deref(),
        Some("AXUIElementSetAttributeValue returned -25204")
    );
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    let details = error.details.unwrap();
    assert_eq!(details["ax_error"], -25204);
    assert_eq!(details["verification_scope"], "text_selection");
    assert!(details["focus_changed"].is_null());
    assert!(details["activated"].is_null());
    assert_eq!(details["focus_observation"], "unknown");
}

#[test]
fn uncertain_setter_error_records_observed_focus_when_reread_succeeds() {
    let element = AXElement(unsafe { AXUIElementCreateSystemWide() });
    let before = focus_snapshot(element.clone());
    let after = focus_snapshot(element);
    let error = after_uncertain_write(uncertain_setter_error(), &before, || Ok(after));

    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    let details = error.details.unwrap();
    assert_eq!(details["focus_changed"], false);
    assert_eq!(details["activated"], false);
    assert_eq!(details["focus_observation"], "complete");
}

#[test]
fn not_delivered_setter_error_keeps_prewrite_disposition_without_focus_reread() {
    let before = focus_snapshot(AXElement(unsafe { AXUIElementCreateSystemWide() }));
    let read_called = Cell::new(false);
    let original = AdapterError::new(ErrorCode::ActionNotSupported, "Selection is unsupported")
        .with_disposition(DeliverySemantics::not_delivered());
    let error = after_uncertain_write(original, &before, || {
        read_called.set(true);
        Err(AdapterError::timeout("focus read failed"))
    });

    assert!(!read_called.get());
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.message, "Selection is unsupported");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(error.details.is_none());
}

#[test]
fn focus_read_timeout_is_not_absence() {
    let system = AXElement(unsafe { AXUIElementCreateSystemWide() });
    let error = super::read_element(
        &system,
        "AXFocusedWindow",
        agent_desktop_core::Deadline::after(0).unwrap(),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(
        error.details.unwrap()["ax_error"],
        accessibility_sys::kAXErrorCannotComplete
    );
}

#[test]
fn optional_focus_transitions_preserve_preflight_and_postwrite_semantics() {
    let element = AXElement(unsafe { AXUIElementCreateSystemWide() });
    for window in [false, true] {
        for focused in [false, true] {
            let mut before = focus_snapshot(element.clone());
            before.window = window.then(|| element.clone());
            before.focused = focused.then(|| element.clone());
            assert!(before.matches(&before));
            let unchanged = super::focus_details(&before, Some(&before));
            assert_eq!(unchanged["focus_changed"], false);
            assert_eq!(unchanged["activated"], false);
            for (next_window, next_focused) in [(!window, focused), (window, !focused)] {
                let mut after = focus_snapshot(element.clone());
                after.window = next_window.then(|| element.clone());
                after.focused = next_focused.then(|| element.clone());
                assert!(!before.matches(&after));
                let preflight = super::preflight_focus_error(&before, &after);
                let postwrite = super::focus_changed_error(&before, &after);
                assert_eq!(preflight.disposition, DeliverySemantics::not_delivered());
                assert_eq!(postwrite.disposition, DeliverySemantics::uncertain());
                for error in [preflight, postwrite] {
                    let details = error.details.unwrap();
                    assert_eq!(details["focus_changed"], focused != next_focused);
                    assert_eq!(details["activated"], window != next_window);
                    assert_eq!(details["focus_observation"], "complete");
                }
            }
        }
    }
}
