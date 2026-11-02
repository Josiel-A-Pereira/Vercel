use super::*;
use agent_desktop_core::ErrorCode;

#[test]
fn keep_all_policy_allows_duplicate_ax_windows_for_one_cg_window() {
    assert!(enforce_duplicate_policy(2, 42, DuplicateWindows::KeepAll).is_ok());
}

#[test]
fn reject_policy_still_rejects_duplicate_ax_windows() {
    let error = enforce_duplicate_policy(2, 42, DuplicateWindows::Reject).unwrap_err();

    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(error.details.unwrap()["candidate_count"], 2);
}

#[test]
fn single_ax_window_match_is_accepted_by_both_policies() {
    assert!(enforce_duplicate_policy(1, 42, DuplicateWindows::Reject).is_ok());
    assert!(enforce_duplicate_policy(1, 42, DuplicateWindows::KeepAll).is_ok());
}
