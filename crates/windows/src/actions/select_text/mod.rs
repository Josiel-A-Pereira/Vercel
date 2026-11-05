//! Native TextPattern text selection with exact and stable readback.

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};

pub(crate) const SELECT_TEXT_LABEL: &str = "TextPattern.Select";

mod select_text_endpoints;

const MAX_TEXT_UTF8_BYTES: usize = 65_536;
const MAX_READ_UNITS: i32 = 65_538;
const STABILITY_WINDOW: std::time::Duration = std::time::Duration::from_millis(100);
const POLL_SLICE: std::time::Duration = std::time::Duration::from_millis(20);

/// The two global effects a semantic selection write must not produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusEffect {
    pub(crate) focus_changed: bool,
    pub(crate) activated: bool,
}

impl FocusEffect {
    pub(crate) fn changed(self) -> bool {
        self.focus_changed || self.activated
    }
}

fn pre_write_error(message: impl Into<String>) -> AdapterError {
    AdapterError::new(ErrorCode::ActionFailed, message)
        .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(target_os = "windows")]
mod select_text_focus;
#[cfg(target_os = "windows")]
mod select_text_target;
#[cfg(target_os = "windows")]
mod select_text_verify;

#[cfg(target_os = "windows")]
mod imp {
    use super::select_text_focus::{
        attach_focus_evidence, capture_global_focus, compare_global_focus,
        focus_changed_before_write,
    };
    use super::select_text_target::{
        ensure_not_secure, ensure_text_selection_available, expected_selection, read_document_text,
        selection_unavailable, supported_selection, text_pattern,
    };
    use super::select_text_verify::verify_stability;
    use super::{SELECT_TEXT_LABEL, pre_write_error};
    use crate::actions::chain::after_delivery;
    use crate::actions::mutation::{classify_success, classify_write};
    use crate::tree::element::UIAElement;
    use agent_desktop_core::{
        ActionResult, AdapterError, Deadline, DeliverySemantics, TextSelection,
    };
    use uiautomation::types::SupportedTextSelection;

    pub(crate) fn select_text_action(
        element: &UIAElement,
        selection: &TextSelection,
        deadline: Deadline,
    ) -> Result<ActionResult, AdapterError> {
        deadline.ensure_remaining()?;
        selection.validate()?;
        let initial_focus = capture_global_focus(deadline).map_err(not_delivered)?;
        ensure_not_secure(element)?;
        ensure_text_selection_available(element)?;
        let pattern = text_pattern(element)?;
        if supported_selection(&pattern)? == SupportedTextSelection::None {
            return Err(selection_unavailable());
        }
        let document = read_document_text(&pattern, deadline)?;
        let resolved = selection.resolve(&document)?;
        let expected = expected_selection(&pattern, &document, &resolved, deadline)?;
        if read_document_text(&pattern, deadline)? != document {
            return Err(pre_write_error(
                "The document changed before text selection",
            ));
        }
        let focus_before = capture_global_focus(deadline).map_err(not_delivered)?;
        let focus_effect =
            compare_global_focus(&initial_focus, &focus_before).map_err(not_delivered)?;
        if focus_effect.changed() {
            return Err(focus_changed_before_write(focus_effect));
        }
        deadline.ensure_remaining()?;
        let mutation = match expected.range.select() {
            Ok(()) => classify_success(),
            Err(error) => classify_write("Select", SELECT_TEXT_LABEL, &error),
        };
        let delivered =
            mutation.map_err(|error| attach_focus_evidence(error, &focus_before, deadline))?;
        if !delivered {
            return Err(attach_focus_evidence(
                selection_unavailable(),
                &focus_before,
                deadline,
            ));
        }
        verify_stability(&pattern, &expected, &document, &focus_before, deadline).map_err(
            |error| attach_focus_evidence(after_delivery(error), &focus_before, deadline),
        )?;
        Ok(resolved.verified_result(SELECT_TEXT_LABEL, Some(1)))
    }

    fn not_delivered(error: AdapterError) -> AdapterError {
        error.with_disposition(DeliverySemantics::not_delivered())
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use agent_desktop_core::{ActionResult, AdapterError, Deadline, TextSelection};

    use crate::tree::element::UIAElement;

    pub(crate) fn select_text_action(
        _element: &UIAElement,
        _selection: &TextSelection,
        _deadline: Deadline,
    ) -> Result<ActionResult, AdapterError> {
        Err(AdapterError::not_supported("select_text_action"))
    }
}

pub(crate) use imp::select_text_action;

#[cfg(all(test, target_os = "windows"))]
#[path = "select_text_tests.rs"]
mod tests;
