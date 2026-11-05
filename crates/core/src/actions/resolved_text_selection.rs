use std::ops::Range;

use crate::{Action, ActionResult, ActionStep, StepMechanism, TextSelection};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTextSelection {
    pub range: Range<usize>,
    pub utf16_range: Range<usize>,
    pub text: String,
}

impl ResolvedTextSelection {
    /// Call after verifying the selection and unchanged focus.
    pub fn verified_result(&self, method: &'static str, count: Option<usize>) -> ActionResult {
        let action = Action::SelectText(TextSelection::Range {
            start: self.range.start,
            end: self.range.end,
        });
        let step = ActionStep::succeeded(method)
            .with_mechanism(StepMechanism::SemanticApi)
            .with_verified(true);
        ActionResult::from_execution(&action, vec![step])
            .with_details(self.verification_details(count))
    }

    pub fn verification_details(&self, count: Option<usize>) -> Value {
        let mut details = json!({
            "verification_scope": "text_selection",
            "range": {
                "start": self.range.start,
                "end": self.range.end,
                "unit": "unicode_scalar",
            },
            "focus_changed": false,
            "activated": false,
        });
        if let Some(count) = count {
            details["selection_count"] = json!(count);
        }
        details
    }
}

#[cfg(test)]
#[path = "resolved_text_selection_tests.rs"]
mod tests;
