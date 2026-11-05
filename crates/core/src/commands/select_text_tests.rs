use super::*;
use crate::adapter::{ActionOps, InputOps, ObservationOps, SystemOps};
use crate::commands::helpers::test_support::save_one_ref_snapshot;
use crate::refs::refs_test_support::HomeGuard;
use crate::{ActionRequest, ActionResult, AdapterError, NativeHandle, RefEntry};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

struct SelectionAdapter {
    requests: Mutex<Vec<ActionRequest>>,
    verified: bool,
    focus_calls: AtomicUsize,
}

impl SelectionAdapter {
    fn new(verified: bool) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            verified,
            focus_calls: AtomicUsize::new(0),
        }
    }
}

impl ObservationOps for SelectionAdapter {
    fn resolve_element_strict(
        &self,
        _: &RefEntry,
        _: crate::Deadline,
    ) -> Result<NativeHandle, AdapterError> {
        Ok(NativeHandle::null())
    }

    fn get_live_element(
        &self,
        _: &NativeHandle,
        _: crate::Deadline,
    ) -> Result<crate::LiveElement, AdapterError> {
        Ok(crate::LiveElement {
            identity: crate::LiveIdentity {
                name: crate::LocatorField::Known("Target".into()),
                description: crate::LocatorField::Absent,
                identifiers: crate::IdentifierEvidence::absent(),
            },
            state: crate::ElementState {
                role: "textfield".into(),
                states: vec![],
                value: None,
                enabled: Some(true),
                hidden: Some(true),
                offscreen: Some(true),
            },
            states_complete: true,
            bounds: None,
            available_actions: vec![crate::actions::capability::SELECT_TEXT.into()],
        })
    }
    crate::adapter::saved_ref_scope!("textfield", Some("Target"));
}

impl ActionOps for SelectionAdapter {
    fn execute_action(
        &self,
        _: &NativeHandle,
        request: ActionRequest,
        _: &crate::InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        self.requests.lock().unwrap().push(request);
        let result = ActionResult::delivered_unverified("select-text");
        Ok(if self.verified {
            result.with_verified_delivery()
        } else {
            result
        })
    }
}

impl InputOps for SelectionAdapter {}
impl SystemOps for SelectionAdapter {
    crate::adapter::guarded_interaction_lease!();

    fn resolve_window_strict(
        &self,
        window: &crate::WindowInfo,
        _: crate::Deadline,
    ) -> Result<crate::WindowInfo, AdapterError> {
        Ok(window.clone())
    }

    fn focus_window(
        &self,
        _: &crate::WindowInfo,
        _: &crate::InteractionLease,
    ) -> Result<(), AdapterError> {
        self.focus_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn headless_selection_dispatches_offscreen_without_geometry() {
    let _guard = HomeGuard::new();
    let snapshot = save_one_ref_snapshot("textfield", "SelectText");
    let adapter = SelectionAdapter::new(true);
    let selection = TextSelection::Range { start: 2, end: 5 };
    let result = execute(
        SelectArgs {
            ref_id: format!("@{snapshot}:e1"),
            value: None,
            selection: Some(selection.clone()),
            timeout_ms: None,
        },
        &adapter,
        &CommandContext::default(),
    )
    .unwrap();
    assert_eq!(result["action"], "select-text");
    let request = adapter.requests.lock().unwrap();
    let request = request.last().unwrap();
    assert_eq!(request.policy, crate::InteractionPolicy::headless());
    assert!(matches!(&request.action, Action::SelectText(actual) if actual == &selection));
}

#[test]
fn invalid_selection_fails_before_ref_resolution() {
    let adapter = SelectionAdapter::new(true);
    for (value, selection) in [
        (
            None,
            Some(TextSelection::Match {
                text: "".into(),
                nth: None,
            }),
        ),
        (None, None),
        (
            Some("choice".into()),
            Some(TextSelection::Range { start: 0, end: 1 }),
        ),
    ] {
        let error = execute(
            SelectArgs {
                ref_id: "@invalid:e1".into(),
                value,
                selection,
                timeout_ms: None,
            },
            &adapter,
            &CommandContext::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "INVALID_ARGS");
    }
    assert!(adapter.requests.lock().unwrap().is_empty());
}

#[test]
fn stale_ref_never_reaches_the_selection_setter() {
    let _guard = HomeGuard::new();
    let snapshot = save_one_ref_snapshot("textfield", "SelectText");
    let adapter = SelectionAdapter::new(true);
    let error = execute(
        SelectArgs {
            ref_id: format!("@{snapshot}:e2"),
            value: None,
            selection: Some(TextSelection::Range { start: 0, end: 1 }),
            timeout_ms: None,
        },
        &adapter,
        &CommandContext::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "STALE_REF");
    assert!(adapter.requests.lock().unwrap().is_empty());
}

#[test]
fn unverified_selection_is_an_error_with_unsafe_retry() {
    let _guard = HomeGuard::new();
    let snapshot = save_one_ref_snapshot("textfield", "SelectText");
    let adapter = SelectionAdapter::new(false);
    let error = execute(
        SelectArgs {
            ref_id: format!("@{snapshot}:e1"),
            value: None,
            selection: Some(TextSelection::Range { start: 0, end: 2 }),
            timeout_ms: Some(500),
        },
        &adapter,
        &CommandContext::default(),
    )
    .unwrap_err();
    match error {
        AppError::Adapter(error) => {
            assert_eq!(error.code, crate::ErrorCode::ActionFailed);
            assert_eq!(
                error.disposition,
                crate::DeliverySemantics::delivered_unverified()
            );
        }
        _ => panic!("expected adapter error"),
    }
    assert_eq!(adapter.requests.lock().unwrap().len(), 1);
}

#[test]
fn selection_needs_no_focus_geometry_or_scroll_in_either_policy() {
    let adapter = SelectionAdapter::new(true);
    let entry: RefEntry = serde_json::from_value(serde_json::json!({
        "pid": 1, "process_instance": "test-instance", "role": "textfield", "name": "Target",
        "source_window_id": "w-1", "source_surface": "window", "path": [0],
        "unique_at_capture": true, "available_actions": ["SelectText"], "states": []
    }))
    .unwrap();
    for headed in [false, true] {
        let action = Action::SelectText(TextSelection::Range { start: 0, end: 1 });
        let request = if headed {
            ActionRequest::headed(action)
        } else {
            ActionRequest::headless(action)
        };
        crate::ref_action::execute_entry(&adapter, &entry, request.with_timeout_ms(Some(500)))
            .unwrap();
    }
    assert_eq!(adapter.focus_calls.load(Ordering::SeqCst), 0);
    let requests = adapter.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].policy, crate::InteractionPolicy::headless());
    assert_eq!(requests[1].policy, crate::InteractionPolicy::headed());
}

#[test]
fn raw_handle_execution_validates_before_calling_the_adapter() {
    let adapter = SelectionAdapter::new(true);
    let lease = crate::InteractionLease::guarded(crate::Deadline::standard().unwrap(), ()).unwrap();
    let request = ActionRequest::headless(Action::SelectText(TextSelection::Match {
        text: "".into(),
        nth: None,
    }));
    let error = crate::execute_verified_action(&adapter, &NativeHandle::null(), request, &lease)
        .unwrap_err();
    assert_eq!(error.code, crate::ErrorCode::InvalidArgs);
    assert_eq!(error.disposition, crate::DeliverySemantics::not_delivered());
    assert!(adapter.requests.lock().unwrap().is_empty());
}
