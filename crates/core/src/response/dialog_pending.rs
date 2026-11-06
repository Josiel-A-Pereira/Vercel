use crate::{AdapterError, DeliverySemantics, ErrorCode};

impl AdapterError {
    /// A prompt opened by a close request is still waiting for an answer, so
    /// the app did not exit by the deadline. The request was delivered and
    /// its effect is unverified: answer the prompt in the app, or force the
    /// close, which discards unsaved work. One title per open dialog, in
    /// surface order; an untitled dialog contributes an empty string so the
    /// count still reports it.
    pub fn dialog_pending(app: &str, pid: i32, force: bool, dialog_titles: &[String]) -> Self {
        let dialogs = dialog_titles.len();
        let message = if dialogs == 1 {
            format!("Application '{app}' did not exit because 1 dialog is waiting for an answer")
        } else {
            format!(
                "Application '{app}' did not exit because {dialogs} dialogs are waiting for an answer"
            )
        };
        Self::new(ErrorCode::ActionFailed, message)
            .with_suggestion(
                "A prompt, such as one asking to save, may be open. Answer it in the app, \
                 or use --force, which discards unsaved work.",
            )
            .with_details(serde_json::json!({
                "cause": "dialog_pending",
                "app": app,
                "pid": pid,
                "force": force,
                "dialogs": dialogs,
                "dialog_titles": dialog_titles,
                "dialog": dialog_titles.first(),
            }))
            .with_disposition(DeliverySemantics::delivered_unverified())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RetryDisposition;

    #[test]
    fn dialog_pending_pins_code_message_suggestion_details_and_disposition() {
        let titles = vec!["Review changes before quitting?".to_string()];
        let error = AdapterError::dialog_pending("TextEdit", 47736, false, &titles);

        assert_eq!(error.code, ErrorCode::ActionFailed);
        assert_eq!(
            error.message,
            "Application 'TextEdit' did not exit because 1 dialog is waiting for an answer"
        );
        assert_eq!(
            error.suggestion.as_deref(),
            Some(
                "A prompt, such as one asking to save, may be open. Answer it in the app, \
                 or use --force, which discards unsaved work."
            )
        );
        assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
        assert_eq!(error.disposition.retry(), RetryDisposition::Unsafe);
        let details = error.details.as_ref().unwrap();
        assert_eq!(details["cause"], "dialog_pending");
        assert_eq!(details["app"], "TextEdit");
        assert_eq!(details["pid"], 47736);
        assert_eq!(details["force"], false);
        assert_eq!(details["dialogs"], 1);
        assert_eq!(
            details["dialog_titles"][0],
            "Review changes before quitting?"
        );
        assert_eq!(details["dialog"], "Review changes before quitting?");
    }

    #[test]
    fn dialog_pending_counts_untitled_prompts_and_reports_the_first_title() {
        let titles = vec![String::new(), "Save?".to_string()];
        let error = AdapterError::dialog_pending("TextEdit", 42, false, &titles);

        let details = error.details.as_ref().unwrap();
        assert_eq!(details["dialogs"], 2);
        assert_eq!(details["dialog"], "");
        assert_eq!(
            error.message,
            "Application 'TextEdit' did not exit because 2 dialogs are waiting for an answer"
        );
    }
}
