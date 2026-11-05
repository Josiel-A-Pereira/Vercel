use std::time::Instant;

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};
use serde_json::json;

use super::{not_delivered_after_verification, observe_selection, with_focus_evidence};
use crate::actions::select_text_focus::GlobalFocus;

fn focus(window: &str, element: &str) -> GlobalFocus {
    GlobalFocus {
        window: window.to_owned(),
        element: element.to_owned(),
    }
}

#[test]
fn selection_must_appear_before_it_can_clear_and_remain_stable() {
    let now = Instant::now();
    assert_eq!(observe_selection(false, None, now), Ok(None));
    assert_eq!(observe_selection(true, None, now), Ok(Some(now)));
    assert_eq!(
        observe_selection(false, Some(now), now),
        Err("selection_cleared")
    );
    assert_eq!(observe_selection(true, Some(now), now), Ok(Some(now)));
}

#[test]
fn failed_no_op_verification_remains_not_delivered() {
    let error = AdapterError::new(ErrorCode::Timeout, "selection is not stable")
        .with_disposition(DeliverySemantics::uncertain());
    let error = not_delivered_after_verification(error);
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}

#[test]
fn refused_setter_focus_evidence_is_current_or_explicitly_unknown() {
    let before = focus("editor", "field");
    let after = focus("editor", "toolbar");
    let error = AdapterError::new(ErrorCode::ActionNotSupported, "setter refused")
        .with_disposition(DeliverySemantics::not_delivered())
        .with_details(json!({"provider": "atspi"}));
    let annotated = with_focus_evidence(error, Some(&before), Some(&after));
    assert_eq!(annotated.disposition, DeliverySemantics::not_delivered());
    assert_eq!(
        annotated
            .details
            .as_ref()
            .and_then(|value| value.get("focus_changed")),
        Some(&json!(true))
    );
    assert_eq!(
        annotated
            .details
            .as_ref()
            .and_then(|value| value.get("activated")),
        Some(&json!(false))
    );

    let unknown = AdapterError::new(ErrorCode::ActionNotSupported, "setter refused")
        .with_disposition(DeliverySemantics::not_delivered());
    let annotated = with_focus_evidence(unknown, Some(&before), None);
    assert_eq!(annotated.disposition, DeliverySemantics::not_delivered());
    assert_eq!(
        annotated
            .details
            .as_ref()
            .and_then(|value| value.get("focus_changed")),
        Some(&json!(null))
    );
    assert_eq!(
        annotated
            .details
            .as_ref()
            .and_then(|value| value.get("activated")),
        Some(&json!(null))
    );
}
