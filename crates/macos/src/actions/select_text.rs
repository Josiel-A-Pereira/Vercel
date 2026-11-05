use std::ops::Range;
use std::time::{Duration, Instant};

use accessibility_sys::{AXValueCreate, kAXValueTypeCFRange};
use agent_desktop_core::{
    ActionResult, AdapterError, Deadline, DeliverySemantics, ErrorCode, ResolvedTextSelection,
    TextSelection,
};
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    string::CFString,
};
use core_foundation_sys::base::CFRange;

use super::select_text_focus::{
    FocusSnapshot, after_uncertain_write, after_write_error, focus_changed_error,
    preflight_focus_error, read_global_focus,
};
use super::select_text_ranges::{
    SelectionRead, read_selection_ranges, read_single_selection, selection_matches,
};
use crate::tree::AXElement;

const TEXT_LIMIT: usize = 65_536;
const STABILITY: Duration = Duration::from_millis(100);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";

#[derive(Debug, PartialEq, Eq)]
enum SelectionProgress {
    Waiting,
    Stable,
    Cleared,
}

pub(crate) fn execute(
    element: &AXElement,
    selection: &TextSelection,
    deadline: Deadline,
) -> Result<ActionResult, AdapterError> {
    let initial_focus = read_global_focus(deadline).map_err(not_delivered)?;
    reject_secure_text(element, deadline).map_err(not_delivered)?;
    if !crate::actions::ax_helpers::is_attr_settable(element, RANGE_ATTRIBUTE, deadline)
        .map_err(not_delivered)?
    {
        return Err(unsupported("Text selection range is not settable"));
    }
    let original_text = read_complete_text(element, "AXValue", deadline).map_err(not_delivered)?;
    let resolved = selection.resolve(&original_text).map_err(not_delivered)?;
    read_selection_ranges(element, deadline).map_err(not_delivered)?;
    read_complete_text(element, "AXSelectedText", deadline).map_err(not_delivered)?;
    let before_write = read_global_focus(deadline).map_err(not_delivered)?;
    if !initial_focus.matches(&before_write) {
        return Err(preflight_focus_error(&initial_focus, &before_write));
    }
    if read_complete_text(element, "AXValue", deadline).map_err(not_delivered)? != original_text {
        return Err(unsupported("Text changed during selection preflight"));
    }

    set_selection(element, &resolved, deadline).map_err(|error| {
        after_uncertain_write(error, &before_write, || read_global_focus(deadline))
    })?;
    verify_selection(element, &original_text, &resolved, &before_write, deadline)
}

fn reject_secure_text(element: &AXElement, deadline: Deadline) -> Result<(), AdapterError> {
    let role = read_text_attribute(element, "AXRole", deadline)?;
    let subrole = read_text_attribute(element, "AXSubrole", deadline)?;
    if is_secure_text(role.as_deref(), subrole.as_deref()) {
        return Err(unsupported(
            "Text selection is unavailable for secure fields",
        ));
    }
    if role.is_none() {
        return Err(unsupported("Text field role could not be verified"));
    }
    Ok(())
}

fn read_text_attribute(
    element: &AXElement,
    attribute: &str,
    deadline: Deadline,
) -> Result<Option<String>, AdapterError> {
    crate::tree::properties::attributes::copy_string_attr_result(element, attribute, deadline)
        .map_err(|error| crate::system::focus::map_ax_read_error(error, "read text field role"))
}

fn read_complete_text(
    element: &AXElement,
    attribute: &str,
    deadline: Deadline,
) -> Result<String, AdapterError> {
    let mut usage = crate::tree::observation_usage::ObservationUsage::new(
        agent_desktop_core::ObservationBudget {
            max_field_bytes: TEXT_LIMIT,
            max_text_bytes: TEXT_LIMIT,
            ..agent_desktop_core::ObservationBudget::default()
        },
    );
    let value = crate::tree::properties::attributes::copy_string_attr_bounded_result(
        element, attribute, deadline, &mut usage,
    )
    .map_err(|error| crate::system::focus::map_ax_read_error(error, "read complete text"))?
    .ok_or_else(|| unsupported("Complete text or selected text is unavailable"))?;
    if !value.complete || value.value.len() > TEXT_LIMIT {
        return Err(unsupported(
            "Text is incomplete or exceeds 65,536 UTF-8 bytes",
        ));
    }
    Ok(value.value)
}

fn set_selection(
    element: &AXElement,
    selection: &ResolvedTextSelection,
    deadline: Deadline,
) -> Result<(), AdapterError> {
    let start = isize::try_from(selection.utf16_range.start)
        .map_err(|_| unsupported("Text selection offset is out of range"))?;
    let length = isize::try_from(selection.utf16_range.end - selection.utf16_range.start)
        .map_err(|_| unsupported("Text selection length is out of range"))?;
    let mut range = CFRange {
        location: start,
        length,
    };
    let value = unsafe { AXValueCreate(kAXValueTypeCFRange, (&mut range as *mut CFRange).cast()) };
    if value.is_null() {
        return Err(unsupported(
            "Could not create the native text selection range",
        ));
    }
    let value = unsafe { CFType::wrap_under_create_rule(value as CFTypeRef) };
    let attribute = CFString::new(RANGE_ATTRIBUTE);
    let error = crate::tree::properties::ax_ipc::set_attribute_value(
        element,
        attribute.as_concrete_TypeRef(),
        value.as_CFTypeRef(),
        deadline,
    )?;
    let delivered = crate::actions::ax_mutation::classify_result(
        element,
        RANGE_ATTRIBUTE,
        "AXUIElementSetAttributeValue",
        error,
    )?;
    if !delivered {
        return Err(unsupported(
            "Accessibility rejected the text selection range",
        ));
    }
    Ok(())
}

fn verify_selection(
    element: &AXElement,
    original_text: &str,
    requested: &ResolvedTextSelection,
    before: &FocusSnapshot,
    deadline: Deadline,
) -> Result<ActionResult, AdapterError> {
    let expected = requested.utf16_range.clone();
    let mut exact_since = None;
    loop {
        let text = read_complete_text(element, "AXValue", deadline)
            .map_err(|error| post_write_error(error, before, deadline))?;
        if text != original_text {
            return Err(post_write_error(
                AdapterError::new(ErrorCode::ActionFailed, "Text changed during selection"),
                before,
                deadline,
            ));
        }
        let selected_text = read_complete_text(element, "AXSelectedText", deadline)
            .map_err(|error| post_write_error(error, before, deadline))?;
        let selected = read_selection_ranges(element, deadline)
            .map_err(|error| post_write_error(error, before, deadline))?;
        let range = read_single_selection(element, deadline)
            .map_err(|error| post_write_error(error, before, deadline))?;
        let exact =
            selection_matches(&selected, &range, &expected) && selected_text == requested.text;
        let after =
            read_global_focus(deadline).map_err(|error| after_write_error(error, before, None))?;
        if !before.matches(&after) {
            return Err(focus_changed_error(before, &after));
        }
        let now = Instant::now();
        match observe_selection(exact, &mut exact_since, now) {
            SelectionProgress::Stable => {
                return Ok(requested.verified_result(RANGE_ATTRIBUTE, selected.count));
            }
            SelectionProgress::Cleared => {
                return Err(after_write_error(
                    selection_mismatch(
                        &selected,
                        &range,
                        &selected_text,
                        &expected,
                        &requested.text,
                    ),
                    before,
                    Some(&after),
                ));
            }
            SelectionProgress::Waiting => {}
        }
        let pause = deadline
            .remaining_slice(POLL_INTERVAL)
            .map_err(|error| after_write_error(error, before, Some(&after)))?;
        let remaining_stability = exact_since
            .map(|start| STABILITY.saturating_sub(now.duration_since(start)))
            .unwrap_or(pause);
        std::thread::sleep(pause.min(remaining_stability));
    }
}

fn observe_selection(
    exact: bool,
    exact_since: &mut Option<Instant>,
    now: Instant,
) -> SelectionProgress {
    match (exact, *exact_since) {
        (false, Some(_)) => SelectionProgress::Cleared,
        (false, None) => SelectionProgress::Waiting,
        (true, None) => {
            *exact_since = Some(now);
            SelectionProgress::Waiting
        }
        (true, Some(started)) if now.duration_since(started) >= STABILITY => {
            SelectionProgress::Stable
        }
        (true, Some(_)) => SelectionProgress::Waiting,
    }
}

fn selection_mismatch(
    selected: &SelectionRead,
    range: &Range<usize>,
    selected_text: &str,
    expected: &Range<usize>,
    expected_text: &str,
) -> AdapterError {
    let mut details = serde_json::json!({
        "expected_utf16_range": [expected.start, expected.end],
        "selected_ranges": selected.only_range.as_ref().map(|value| [value.start, value.end]),
        "selected_range": [range.start, range.end],
        "expected_selected_text": expected_text,
        "observed_selected_text": selected_text,
    });
    if let Some(count) = selected.count {
        details["selection_count"] = serde_json::json!(count);
    }
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Accessibility cleared or changed the verified text selection",
    )
    .with_details(details)
}

fn post_write_error(
    error: AdapterError,
    before: &FocusSnapshot,
    deadline: Deadline,
) -> AdapterError {
    match read_global_focus(deadline) {
        Ok(after) => after_write_error(error, before, Some(&after)),
        Err(_) => after_write_error(error, before, None),
    }
}

fn is_secure_text(role: Option<&str>, subrole: Option<&str>) -> bool {
    role == Some("AXSecureTextField") || subrole == Some("AXSecureTextField")
}

fn unsupported(message: &str) -> AdapterError {
    AdapterError::new(ErrorCode::ActionNotSupported, message)
        .with_disposition(DeliverySemantics::not_delivered())
}

fn not_delivered(error: AdapterError) -> AdapterError {
    error.with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "select_text_tests.rs"]
mod tests;
