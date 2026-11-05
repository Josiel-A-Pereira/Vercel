//! Exact selection readback followed by a bounded stability observation.

use super::select_text_focus::{
    GlobalFocus, capture_global_focus, compare_global_focus, focus_side_effect_error,
};
use super::select_text_target::{
    ExpectedSelection, actual_prefix, bounded_range_text, read_document_text,
};
use super::{POLL_SLICE, STABILITY_WINDOW};
use crate::tree::automation::uia_error;
use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};
use std::time::Instant;
use uiautomation::patterns::UITextPattern;

struct Observation {
    selection_exact: bool,
    document_unchanged: bool,
    selection_count: usize,
}

pub(super) fn exact_since_after(
    exact_since: Option<Instant>,
    now: Instant,
    selection_exact: bool,
    selection_count: usize,
) -> Result<Option<Instant>, AdapterError> {
    match (exact_since, selection_exact) {
        (None, false) => Ok(None),
        (None, true) => Ok(Some(now)),
        (Some(since), true) => Ok(Some(since)),
        (Some(_), false) => Err(selection_mismatch(selection_count)),
    }
}

pub(super) fn verify_stability(
    pattern: &UITextPattern,
    expected: &ExpectedSelection,
    document: &str,
    focus_before: &GlobalFocus,
    deadline: Deadline,
) -> Result<(), AdapterError> {
    let mut exact_since: Option<Instant> = None;
    loop {
        let observed = observe_once(pattern, expected, document, deadline)?;
        if !observed.document_unchanged {
            return Err(document_changed());
        }
        let focus_after = capture_global_focus(deadline)?;
        let effect = compare_global_focus(focus_before, &focus_after)?;
        if effect.changed() {
            return Err(focus_side_effect_error(effect));
        }
        let now = Instant::now();
        exact_since = exact_since_after(
            exact_since,
            now,
            observed.selection_exact,
            observed.selection_count,
        )?;
        if exact_since.is_some_and(|since| now.duration_since(since) >= STABILITY_WINDOW) {
            return Ok(());
        }
        std::thread::sleep(deadline.remaining_slice(POLL_SLICE)?);
    }
}

fn observe_once(
    pattern: &UITextPattern,
    expected: &ExpectedSelection,
    document: &str,
    deadline: Deadline,
) -> Result<Observation, AdapterError> {
    deadline.ensure_remaining()?;
    let selections = pattern
        .get_selection()
        .map_err(|error| uia_error(&error, "read TextPattern.GetSelection"))?;
    let selection_count = selections.len();
    let selection_exact = match selections.as_slice() {
        [actual] => {
            let range_exact = expected
                .range
                .compare(actual)
                .map_err(|error| uia_error(&error, "compare TextPattern ranges"))?;
            let text = bounded_range_text(actual)?;
            let prefix = actual_prefix(pattern, actual, deadline)?;
            range_exact && text == expected.text && prefix == expected.prefix
        }
        _ => false,
    };
    let document_unchanged = read_document_text(pattern, deadline)? == document;
    Ok(Observation {
        selection_exact,
        document_unchanged,
        selection_count,
    })
}

fn document_changed() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "The document text changed during text selection",
    )
    .with_suggestion("Inspect the document and selection before retrying.")
    .with_details(serde_json::json!({
        "verification_scope": "text_selection",
        "document_changed": true,
    }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

fn selection_mismatch(selection_count: usize) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Text selection did not remain exactly verified",
    )
    .with_suggestion("Inspect the live text and selection before retrying.")
    .with_details(serde_json::json!({
        "verification_scope": "text_selection",
        "selection_count": selection_count,
    }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}
