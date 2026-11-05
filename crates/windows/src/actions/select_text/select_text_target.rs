//! Provider text reads and scalar-to-provider endpoint mapping.

use super::select_text_endpoints::resolve_endpoint;
use super::{MAX_READ_UNITS, MAX_TEXT_UTF8_BYTES, pre_write_error};
use crate::tree::automation::uia_error;
use crate::tree::element::UIAElement;
use crate::tree::properties::property_ids::TreeProperty;
use crate::tree::properties::{PropertyOutcome, PropertyValue, read_one};
use agent_desktop_core::{
    AdapterError, Deadline, DeliverySemantics, ErrorCode, ResolvedTextSelection,
};
use uiautomation::patterns::{UITextPattern, UITextRange};
use uiautomation::types::{SupportedTextSelection, TextPatternRangeEndpoint, TextUnit};

pub(super) struct ExpectedSelection {
    pub(super) range: UITextRange,
    pub(super) prefix: String,
    pub(super) text: String,
}

pub(super) fn ensure_not_secure(element: &UIAElement) -> Result<(), AdapterError> {
    let safe = match read_one(element, TreeProperty::IsPassword) {
        PropertyOutcome::Absent => true,
        PropertyOutcome::Known(PropertyValue::Flag(value)) => !value,
        PropertyOutcome::Known(PropertyValue::Number(value)) => value == 0,
        PropertyOutcome::Known(_) | PropertyOutcome::Unknown => false,
    };
    if safe {
        Ok(())
    } else {
        Err(AdapterError::new(
            ErrorCode::ActionNotSupported,
            "Text selection is refused because the target may be a secure field",
        )
        .with_details(serde_json::json!({
            "kind": "secure_text_selection_refused",
            "platform": "windows",
        }))
        .with_disposition(DeliverySemantics::not_delivered()))
    }
}

pub(super) fn ensure_text_selection_available(element: &UIAElement) -> Result<(), AdapterError> {
    match read_one(element, TreeProperty::TextAvailable) {
        PropertyOutcome::Known(PropertyValue::Flag(true)) => Ok(()),
        PropertyOutcome::Known(PropertyValue::Number(value)) if value != 0 => Ok(()),
        _ => Err(selection_unavailable()),
    }
}

pub(super) fn text_pattern(element: &UIAElement) -> Result<UITextPattern, AdapterError> {
    element
        .0
        .get_pattern::<UITextPattern>()
        .map_err(|error| not_delivered(uia_error(&error, "acquire TextPattern")))
}

pub(super) fn supported_selection(
    pattern: &UITextPattern,
) -> Result<SupportedTextSelection, AdapterError> {
    pattern.get_supported_text_selection().map_err(|error| {
        not_delivered(uia_error(&error, "read TextPattern.SupportedTextSelection"))
    })
}

pub(super) fn read_document_text(
    pattern: &UITextPattern,
    deadline: Deadline,
) -> Result<String, AdapterError> {
    deadline.ensure_remaining()?;
    let range = pattern
        .get_document_range()
        .map_err(|error| not_delivered(uia_error(&error, "read TextPattern.DocumentRange")))?;
    bounded_range_text(&range)
}

pub(super) fn bounded_range_text(range: &UITextRange) -> Result<String, AdapterError> {
    let text = range
        .get_text(MAX_READ_UNITS)
        .map_err(|error| not_delivered(uia_error(&error, "read UI Automation text")))?;
    if text.len() > MAX_TEXT_UTF8_BYTES {
        return Err(AdapterError::new(
            ErrorCode::ActionNotSupported,
            format!("Text exceeds the {MAX_TEXT_UTF8_BYTES}-byte selection limit"),
        )
        .with_details(serde_json::json!({
            "kind": "text_too_large",
            "max_utf8_bytes": MAX_TEXT_UTF8_BYTES,
        }))
        .with_disposition(DeliverySemantics::not_delivered()));
    }
    Ok(text)
}

pub(super) fn expected_selection(
    pattern: &UITextPattern,
    document: &str,
    resolved: &ResolvedTextSelection,
    deadline: Deadline,
) -> Result<ExpectedSelection, AdapterError> {
    let scalar_count = document.chars().count();
    if resolved.range.start > resolved.range.end || resolved.range.end > scalar_count {
        return Err(pre_write_error(
            "Resolved text range is outside the live document",
        ));
    }
    let start_prefix: String = document.chars().take(resolved.range.start).collect();
    let end_prefix: String = document.chars().take(resolved.range.end).collect();
    let start_count = provider_endpoint_count(pattern, &start_prefix, deadline)?
        .ok_or_else(provider_units_unmappable)?;
    let end_count = provider_endpoint_count(pattern, &end_prefix, deadline)?
        .ok_or_else(provider_units_unmappable)?;
    let start = prefix_range(pattern, start_count, deadline)?;
    let end = prefix_range(pattern, end_count, deadline)?;
    let range = pattern.get_document_range().map_err(|error| {
        not_delivered(uia_error(&error, "create a TextPattern selection range"))
    })?;
    let [
        (start_target, start_source, start_source_endpoint),
        (end_target, end_source, end_source_endpoint),
    ] = selection_endpoints(&start, &end);
    range
        .move_endpoint_by_range(start_target, start_source, start_source_endpoint)
        .map_err(|error| not_delivered(uia_error(&error, "move the selection start")))?;
    range
        .move_endpoint_by_range(end_target, end_source, end_source_endpoint)
        .map_err(|error| not_delivered(uia_error(&error, "move the selection end")))?;
    let text = bounded_range_text(&range)?;
    if text != resolved.text {
        return Err(provider_units_unmappable());
    }
    Ok(ExpectedSelection {
        range,
        prefix: start_prefix,
        text,
    })
}

pub(super) fn selection_endpoints<'a, T>(
    start: &'a T,
    end: &'a T,
) -> [(TextPatternRangeEndpoint, &'a T, TextPatternRangeEndpoint); 2] {
    [
        (
            TextPatternRangeEndpoint::Start,
            start,
            TextPatternRangeEndpoint::End,
        ),
        (
            TextPatternRangeEndpoint::End,
            end,
            TextPatternRangeEndpoint::End,
        ),
    ]
}

pub(super) fn actual_prefix(
    pattern: &UITextPattern,
    selection: &UITextRange,
    deadline: Deadline,
) -> Result<String, AdapterError> {
    deadline.ensure_remaining()?;
    let prefix = pattern
        .get_document_range()
        .map_err(|error| uia_error(&error, "create a range for the selected text prefix"))?;
    prefix
        .move_endpoint_by_range(
            TextPatternRangeEndpoint::End,
            selection,
            TextPatternRangeEndpoint::Start,
        )
        .map_err(|error| uia_error(&error, "move the selected text prefix endpoint"))?;
    bounded_range_text(&prefix)
}

fn provider_endpoint_count(
    pattern: &UITextPattern,
    expected: &str,
    deadline: Deadline,
) -> Result<Option<usize>, AdapterError> {
    resolve_endpoint(expected, |count| prefix_text(pattern, count, deadline))
}

fn prefix_text(
    pattern: &UITextPattern,
    provider_units: usize,
    deadline: Deadline,
) -> Result<String, AdapterError> {
    prefix_range(pattern, provider_units, deadline).and_then(|range| bounded_range_text(&range))
}

fn prefix_range(
    pattern: &UITextPattern,
    provider_units: usize,
    deadline: Deadline,
) -> Result<UITextRange, AdapterError> {
    deadline.ensure_remaining()?;
    let count = i32::try_from(provider_units)
        .map_err(|_| pre_write_error("Provider text offset exceeds the supported range"))?;
    let anchor = pattern
        .get_document_range()
        .map_err(|error| not_delivered(uia_error(&error, "create a TextPattern prefix anchor")))?;
    let range = pattern
        .get_document_range()
        .map_err(|error| not_delivered(uia_error(&error, "create a TextPattern prefix range")))?;
    range
        .move_endpoint_by_range(
            TextPatternRangeEndpoint::End,
            &anchor,
            TextPatternRangeEndpoint::Start,
        )
        .map_err(|error| not_delivered(uia_error(&error, "collapse a TextPattern prefix")))?;
    if count > 0 {
        range
            .move_endpoint_by_unit(TextPatternRangeEndpoint::End, TextUnit::Character, count)
            .map_err(|error| {
                not_delivered(uia_error(&error, "move a TextPattern prefix endpoint"))
            })?;
    }
    Ok(range)
}

pub(super) fn selection_unavailable() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionNotSupported,
        "The target does not expose a selectable UI Automation Text range",
    )
    .with_suggestion("Target a control that advertises a selectable Text pattern.")
    .with_disposition(DeliverySemantics::not_delivered())
}

fn provider_units_unmappable() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionNotSupported,
        "The provider could not map the requested scalar range to its text units",
    )
    .with_suggestion(
        "Target a provider whose Text pattern preserves the requested character boundaries.",
    )
    .with_disposition(DeliverySemantics::not_delivered())
}

fn not_delivered(error: AdapterError) -> AdapterError {
    error.with_disposition(DeliverySemantics::not_delivered())
}
