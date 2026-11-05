use std::thread;
use std::time::{Duration, Instant};

use agent_desktop_core::{
    ActionResult, AdapterError, DeliverySemantics, ErrorCode, ResolvedTextSelection,
};
use serde_json::{Value, json};

use super::super::context::Target;
use super::super::select_text_focus::{self, GlobalFocus};
use crate::tree::capabilities::text_selection::{TextState, read_text_state};

const SETTLE: Duration = Duration::from_millis(100);
const POLL: Duration = Duration::from_millis(10);

pub(super) struct VerificationContext<'a, 'target> {
    pub(super) target: &'a Target<'target>,
    pub(super) before: &'a TextState,
    pub(super) selection: &'a ResolvedTextSelection,
    pub(super) before_focus: Option<&'a GlobalFocus>,
}

impl<'a, 'target> VerificationContext<'a, 'target> {
    pub(super) fn preflight_satisfied(&self) -> bool {
        exact_selection(self.before, self.before, self.selection)
    }

    pub(super) fn satisfied_without_delivery(&self) -> ActionResult {
        ActionResult::satisfied_without_delivery("select-text").with_details(
            self.selection
                .verification_details(Some(self.selection_count())),
        )
    }

    fn current_focus(&self) -> Option<GlobalFocus> {
        select_text_focus::capture(self.target.bus, self.target.deadline)
            .ok()
            .flatten()
    }

    fn failure(
        &self,
        message: &'static str,
        state: Option<&TextState>,
        after_focus: Option<&GlobalFocus>,
    ) -> AdapterError {
        self.annotate(
            AdapterError::new(ErrorCode::ActionFailed, message)
                .with_disposition(DeliverySemantics::uncertain()),
            state,
            after_focus,
        )
    }

    fn annotate(
        &self,
        error: AdapterError,
        state: Option<&TextState>,
        after_focus: Option<&GlobalFocus>,
    ) -> AdapterError {
        let mut details = error.details.clone().unwrap_or_else(|| json!({}));
        if let Value::Object(values) = &mut details {
            values.insert("verification_scope".to_owned(), json!("text_selection"));
            values.insert(
                "range".to_owned(),
                json!({"start": self.selection.range.start, "end": self.selection.range.end, "unit": "unicode_scalar"}),
            );
            values.insert(
                "selection_count".to_owned(),
                json!(state.map(|s| s.selections.len())),
            );
            values.insert(
                "document_changed".to_owned(),
                json!(state.map(|s| s.text != self.before.text)),
            );
        }
        let error = error
            .with_details(details)
            .with_suggestion("Read the field before retrying; selection delivery was not verified");
        with_focus_evidence(error, self.before_focus, after_focus)
    }

    pub(super) fn annotate_error(
        &self,
        error: AdapterError,
        state: Option<&TextState>,
    ) -> AdapterError {
        let after_focus = self.current_focus();
        self.annotate(error, state, after_focus.as_ref())
    }

    fn failure_at_current_focus(
        &self,
        message: &'static str,
        state: Option<&TextState>,
    ) -> AdapterError {
        let after_focus = self.current_focus();
        self.failure(message, state, after_focus.as_ref())
    }

    pub(super) fn wait_without_delivery(&self) -> Result<(), AdapterError> {
        self.wait().map_err(not_delivered_after_verification)
    }

    pub(super) fn wait(&self) -> Result<(), AdapterError> {
        let after_write_focus = self.current_focus();
        if self.before_focus.is_none() || after_write_focus.as_ref() != self.before_focus {
            return Err(self.failure(
                "Text selection changed the global focused element or active window",
                None,
                after_write_focus.as_ref(),
            ));
        }
        let mut stable_since = None;
        loop {
            let state = read_text_state(self.target.bus, self.target.element, self.target.deadline)
                .map_err(|error| {
                    self.annotate_error(
                        error.with_disposition(DeliverySemantics::uncertain()),
                        None,
                    )
                })?;
            let Some(state) = state else {
                return Err(self.failure_at_current_focus(
                    "The provider's text selection could not be read back",
                    None,
                ));
            };
            if state.text != self.before.text {
                return Err(self.failure_at_current_focus(
                    "Text selection changed the document",
                    Some(&state),
                ));
            }
            let now = Instant::now();
            stable_since = observe_selection(
                exact_selection(&state, self.before, self.selection),
                stable_since,
                now,
            )
            .map_err(|_| {
                self.failure_at_current_focus(
                    "The provider cleared the requested selection during verification",
                    Some(&state),
                )
            })?;
            if stable_since.is_some_and(|since| now.duration_since(since) >= SETTLE) {
                break;
            }
            self.target.deadline.ensure_remaining().map_err(|error| {
                self.annotate_error(
                    error.with_disposition(DeliverySemantics::uncertain()),
                    Some(&state),
                )
            })?;
            thread::sleep(POLL.min(self.target.deadline.remaining()));
        }
        let settled_focus = self.current_focus();
        if settled_focus.as_ref() != self.before_focus {
            return Err(self.failure(
                "The global focused element or active window changed during selection verification",
                None,
                settled_focus.as_ref(),
            ));
        }
        Ok(())
    }

    pub(super) fn success(&self, method: &'static str) -> ActionResult {
        self.selection
            .verified_result(method, Some(self.selection_count()))
    }

    fn selection_count(&self) -> usize {
        usize::from(!self.selection.range.is_empty())
    }
}

fn not_delivered_after_verification(error: AdapterError) -> AdapterError {
    error.with_disposition(DeliverySemantics::not_delivered())
}

fn observe_selection(
    exact: bool,
    since: Option<Instant>,
    now: Instant,
) -> Result<Option<Instant>, &'static str> {
    match (since, exact) {
        (Some(_), false) => Err("selection_cleared"),
        (Some(since), true) => Ok(Some(since)),
        (None, true) => Ok(Some(now)),
        (None, false) => Ok(None),
    }
}

fn exact_selection(
    state: &TextState,
    before: &TextState,
    selection: &ResolvedTextSelection,
) -> bool {
    if state.text != before.text {
        return false;
    }
    if selection.range.is_empty() {
        return state.selections.is_empty() && state.caret == selection.range.start;
    }
    state.selections == [selection.range.clone()]
        && state
            .text
            .chars()
            .skip(selection.range.start)
            .take(selection.range.end - selection.range.start)
            .eq(selection.text.chars())
}

fn with_focus_evidence(
    error: AdapterError,
    before: Option<&GlobalFocus>,
    after: Option<&GlobalFocus>,
) -> AdapterError {
    let (focus_changed, activated) = match (before, after) {
        (Some(before), Some(after)) => {
            let (focus_changed, activated) = after.effects_from(before);
            (Some(focus_changed), Some(activated))
        }
        _ => (None, None),
    };
    let mut details = error.details.clone().unwrap_or_else(|| json!({}));
    if !details.is_object() {
        details = json!({"provider_details": details});
    }
    if let Value::Object(values) = &mut details {
        values.insert("focus_changed".to_owned(), json!(focus_changed));
        values.insert("activated".to_owned(), json!(activated));
    }
    error.with_details(details)
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;
