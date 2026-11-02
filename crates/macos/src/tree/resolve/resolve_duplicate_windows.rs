#[cfg(target_os = "macos")]
use agent_desktop_core::AdapterError;

/// What to do when several AX windows report the same CoreGraphics window number.
///
/// Some apps (Voice Memos launched hidden) expose two AX windows for one CG
/// window. Plain window refs can keep every match as a root, because the saved
/// path plus identity check selects the element. Sheet, popover and alert refs
/// take a single scoped window, so they still require exactly one match.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tree) enum DuplicateWindows {
    Reject,
    KeepAll,
}

#[cfg(target_os = "macos")]
pub(in crate::tree) fn enforce_duplicate_policy(
    match_count: usize,
    window_number: i64,
    duplicates: DuplicateWindows,
) -> Result<(), AdapterError> {
    match duplicates {
        DuplicateWindows::Reject => require_unique_window_number_match(match_count, window_number),
        DuplicateWindows::KeepAll => Ok(()),
    }
}

#[cfg(target_os = "macos")]
pub(in crate::tree) fn require_unique_window_number_match(
    match_count: usize,
    window_number: i64,
) -> Result<(), AdapterError> {
    if match_count > 1 {
        return Err(AdapterError::ambiguous_target(format!(
            "Multiple AX windows matched verified CoreGraphics window w-{window_number}"
        ))
        .with_details(serde_json::json!({
            "kind": "source_window_number_ambiguous",
            "source_window_id": format!("w-{window_number}"),
            "candidate_count": match_count,
        })));
    }
    Ok(())
}

#[cfg(test)]
#[path = "resolve_duplicate_windows_tests.rs"]
mod tests;
