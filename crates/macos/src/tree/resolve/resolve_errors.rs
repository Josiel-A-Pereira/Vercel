use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode, RefEntry};

pub(in crate::tree) fn native_identifier_role_reuse(entry: &RefEntry) -> AdapterError {
    AdapterError::new(
        ErrorCode::StaleRef,
        "Saved native identifier was found only on a different accessibility role",
    )
    .with_suggestion("Run 'snapshot' to refresh, then retry with the updated ref.")
    .with_disposition(DeliverySemantics::not_delivered())
    .with_details(serde_json::json!({
        "kind": "native_identifier_role_reuse",
        "expected_role": entry.identity.role,
        "complete": true,
        "retryable": false,
    }))
}

/// Core owns the payload (`agent_desktop_core::refs::resolve_errors::identity_unknown_error`)
/// so both adapters share one definition: an unreadable candidate
/// means the same thing whichever adapter's search produced it.
pub(in crate::tree) use agent_desktop_core::refs::resolve_errors::identity_unknown_error as identity_unknown;

pub(in crate::tree) fn incomplete_traversal(phase: &str, depth: u8) -> AdapterError {
    AdapterError::new(
        ErrorCode::AppUnresponsive,
        format!(
            "Strict element resolution observed an incomplete accessibility tree during {phase}"
        ),
    )
    .with_suggestion("Retry after the target application finishes updating its accessibility tree")
    .with_details(serde_json::json!({
        "kind": "resolution_traversal_incomplete",
        "phase": phase,
        "depth": depth,
        "complete": false,
        "retryable": true,
    }))
}

/// The saved window's CoreGraphics record still exists but no live AXWindow
/// carries its number. A window that CoreGraphics reports on screen may not
/// have reached the accessibility tree yet, so that miss stays retryable. An
/// off-screen record is a window the app closed but kept, as TextEdit does for
/// a closed document, or a window on another Space; waiting cannot bring it
/// into the accessibility tree, so the ref is stale.
pub(in crate::tree) fn unmatched_source_window(
    entry: &RefEntry,
    source_window_on_screen: bool,
) -> AdapterError {
    if !source_window_on_screen {
        return AdapterError::new(
            ErrorCode::StaleRef,
            "The ref's window is closed or off screen, for example on another Space",
        )
        .with_suggestion("Run 'snapshot' to refresh, then retry with the updated ref.")
        .with_disposition(DeliverySemantics::not_delivered())
        .with_details(serde_json::json!({
            "kind": "source_window_off_screen",
            "source_window_id": entry.source.source_window_id,
            "complete": true,
            "retryable": false,
        }));
    }
    AdapterError::new(
        ErrorCode::AppUnresponsive,
        "The verified CoreGraphics window could not be matched to one live AXWindow",
    )
    .with_suggestion("Retry after the application finishes updating its accessibility windows")
    .with_details(serde_json::json!({
        "kind": "resolution_window_bridge_miss",
        "source_window_id": entry.source.source_window_id,
        "complete": false,
        "retryable": true,
    }))
}

pub(in crate::tree) fn matched_or_unmatched_window<T>(
    found: Option<T>,
    entry: &RefEntry,
    source_window_on_screen: bool,
) -> Result<Option<T>, AdapterError> {
    found
        .map(Some)
        .ok_or_else(|| unmatched_source_window(entry, source_window_on_screen))
}

pub(in crate::tree) fn is_unmatched_source_window(error: &AdapterError) -> bool {
    matches!(
        error
            .details
            .as_ref()
            .and_then(|details| details["kind"].as_str()),
        Some("resolution_window_bridge_miss" | "source_window_off_screen")
    )
}

#[cfg(test)]
#[path = "resolve_errors_tests.rs"]
mod tests;
