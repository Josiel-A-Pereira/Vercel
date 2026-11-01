use super::*;

fn entry_in_window(id: &str) -> RefEntry {
    serde_json::from_value(serde_json::json!({
        "pid": 1,
        "role": "textfield",
        "path": [0],
        "states": [],
        "available_actions": [],
        "source_window_id": id
    }))
    .unwrap()
}

#[test]
fn an_off_screen_source_window_makes_the_ref_stale_without_waiting() {
    let error = unmatched_source_window(&entry_in_window("w-43258"), false);

    assert_eq!(error.code, ErrorCode::StaleRef);
    assert!(!error.is_explicitly_retryable());
    let details = error.details.unwrap();
    assert_eq!(details["kind"], "source_window_off_screen");
    assert_eq!(details["source_window_id"], "w-43258");
    assert_eq!(details["retryable"], false);
}

#[test]
fn an_on_screen_source_window_missing_from_ax_stays_a_retryable_bridge_miss() {
    let error = unmatched_source_window(&entry_in_window("w-43258"), true);

    assert_eq!(error.code, ErrorCode::AppUnresponsive);
    assert!(error.is_explicitly_retryable());
    assert_eq!(
        error.details.unwrap()["kind"],
        "resolution_window_bridge_miss"
    );
}

#[test]
fn both_unmatched_window_outcomes_still_try_the_saved_surface_fallback() {
    let entry = entry_in_window("w-7");

    assert!(is_unmatched_source_window(&unmatched_source_window(
        &entry, true
    )));
    assert!(is_unmatched_source_window(&unmatched_source_window(
        &entry, false
    )));
    assert!(!is_unmatched_source_window(&native_identifier_role_reuse(
        &entry
    )));
}
