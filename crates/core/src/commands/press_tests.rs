use super::{PressArgs, execute};
use crate::AdapterError;
use crate::KeyCombo;
use crate::actions::action_request::ActionRequest;
use crate::actions::action_result::ActionResult;
use crate::adapter::{ActionOps, InputOps, NativeHandle, ObservationOps, SystemOps};
use crate::context::CommandContext;
use std::sync::Mutex;

struct BlockingAdapter;

impl ObservationOps for BlockingAdapter {}

impl ActionOps for BlockingAdapter {
    fn execute_action(
        &self,
        _handle: &NativeHandle,
        _request: ActionRequest,
        _lease: &crate::InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        Ok(ActionResult::delivered_unverified("PressKey"))
    }
}

impl InputOps for BlockingAdapter {}

impl SystemOps for BlockingAdapter {
    crate::adapter::guarded_interaction_lease!();

    fn is_blocked_combo(&self, _combo: &KeyCombo) -> bool {
        true
    }
}

struct AllowingAdapter;

impl ObservationOps for AllowingAdapter {}

impl ActionOps for AllowingAdapter {
    fn execute_action(
        &self,
        _handle: &NativeHandle,
        _request: ActionRequest,
        _lease: &crate::InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        Ok(ActionResult::delivered_unverified("PressKey"))
    }
}

impl InputOps for AllowingAdapter {}

impl SystemOps for AllowingAdapter {
    crate::adapter::guarded_interaction_lease!();
}

#[derive(Default)]
struct CapturingAdapter {
    global_policy: Mutex<Option<crate::InteractionPolicy>>,
    app_policy: Mutex<Option<crate::InteractionPolicy>>,
    dispatches: std::sync::atomic::AtomicUsize,
    expect_scoped_windows: bool,
}

impl ObservationOps for CapturingAdapter {
    fn observe_tree(
        &self,
        root: crate::locator::live_locator::ObservationRoot<'_>,
        _request: &crate::locator::live_locator::ObservationRequest,
    ) -> Result<crate::locator::live_locator::ObservedTree, AdapterError> {
        assert_eq!(self.dispatches.load(std::sync::atomic::Ordering::SeqCst), 1);
        crate::adapter::observed_tree(&root, saved_tree())
    }

    fn get_tree(
        &self,
        _window: &crate::WindowInfo,
        _opts: &crate::adapter::TreeOptions,
        _deadline: crate::Deadline,
    ) -> Result<crate::AccessibilityNode, AdapterError> {
        Ok(saved_tree())
    }

    fn list_apps(&self, _deadline: crate::Deadline) -> Result<Vec<crate::AppInfo>, AdapterError> {
        Ok(vec![crate::AppInfo {
            name: "Editor".into(),
            pid: crate::ProcessId::new(42),
            bundle_id: Some("com.example.Editor".into()),
            process_instance: Some("generation-1".into()),
            presentation: None,
        }])
    }

    fn list_windows(
        &self,
        filter: &crate::WindowFilter,
        _deadline: crate::Deadline,
    ) -> Result<Vec<crate::WindowInfo>, AdapterError> {
        if self.expect_scoped_windows {
            assert_eq!(filter.app.as_deref(), Some("Editor"));
        }
        Ok(vec![crate::WindowInfo {
            id: "w-42".into(),
            title: "Editor".into(),
            app: "Editor".into(),
            pid: crate::ProcessId::new(42),
            process_instance: Some("generation-1".into()),
            bounds: None,
            state: crate::WindowState {
                visible: Some(true),
                ..Default::default()
            },
        }])
    }
}

impl ActionOps for CapturingAdapter {
    fn execute_action(
        &self,
        _handle: &NativeHandle,
        request: ActionRequest,
        _lease: &crate::InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        *self.global_policy.lock().unwrap() = Some(request.policy);
        Ok(ActionResult::delivered_unverified("PressKey"))
    }
}

impl InputOps for CapturingAdapter {}

impl SystemOps for CapturingAdapter {
    fn is_blocked_combo(&self, combo: &KeyCombo) -> bool {
        combo.key == "q"
    }

    crate::adapter::guarded_interaction_lease!();
    crate::adapter::exact_window_focus!();

    fn press_key_for_app(
        &self,
        _process: crate::ProcessIdentity,
        _combo: &KeyCombo,
        policy: crate::InteractionPolicy,
        _lease: &crate::InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        self.dispatches
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.app_policy.lock().unwrap() = Some(policy);
        Ok(ActionResult::delivered_unverified("PressKey"))
    }
}

fn args(combo: &str, force: bool) -> PressArgs {
    PressArgs {
        combo: combo.to_owned(),
        app: None,
        window_id: None,
        force,
    }
}

#[test]
fn adapter_blocked_combo_is_refused_when_not_forced() {
    let err = execute(
        args("cmd+q", false),
        &BlockingAdapter,
        &CommandContext::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "POLICY_DENIED");
    assert!(
        err.to_string().contains("--force"),
        "the refusal must tell the caller how to override, got: {err}"
    );
}

#[test]
fn force_bypasses_the_adapter_block() {
    let mut request = args("cmd+q", true);
    request.app = Some("Editor".into());
    execute(
        request,
        &CapturingAdapter::default(),
        &CommandContext::default(),
    )
    .unwrap();
}

#[test]
fn core_blocks_nothing_by_default() {
    let combo = super::parse_combo_normalized("cmd+q").unwrap();
    super::ensure_combo_allowed(&combo, "cmd+q", false, &AllowingAdapter).unwrap();
}

#[test]
fn unscoped_press_never_falls_back_to_global_input() {
    let adapter = CapturingAdapter::default();
    for headed in [false, true] {
        for force in [false, true] {
            let error = execute(
                args("a", force),
                &adapter,
                &CommandContext::default().with_headed(headed),
            )
            .unwrap_err();
            assert_eq!(error.code(), "POLICY_DENIED");
        }
    }
    assert!(adapter.global_policy.lock().unwrap().is_none());
    assert!(adapter.app_policy.lock().unwrap().is_none());
    let _home = crate::refs::refs_test_support::HomeGuard::new();
    let context = CommandContext::default()
        .with_agent_id(Some("editor".into()))
        .unwrap();
    let mut request = args("a", false);
    request.app = Some("Editor".into());
    execute(request, &adapter, &context).unwrap();
    execute(args("a", false), &adapter, &context).unwrap();
    assert_eq!(
        adapter.dispatches.load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    assert!(adapter.global_policy.lock().unwrap().is_none());
}

#[test]
fn app_targeted_press_threads_focus_authorization_to_adapter() {
    let adapter = CapturingAdapter::default();
    let mut request = args("a", false);
    request.app = Some("Editor".into());

    execute(request, &adapter, &CommandContext::default()).unwrap();
    assert_eq!(
        *adapter.app_policy.lock().unwrap(),
        Some(crate::InteractionPolicy::headless())
    );

    let mut request = args("a", false);
    request.app = Some("Editor".into());
    execute(
        request,
        &adapter,
        &CommandContext::default().with_headed(true),
    )
    .unwrap();
    assert_eq!(
        *adapter.app_policy.lock().unwrap(),
        Some(crate::InteractionPolicy::headed())
    );
}

fn saved_tree() -> crate::AccessibilityNode {
    crate::AccessibilityNode {
        ref_id: None,
        role: "window".into(),
        identity: crate::NodeIdentity {
            retained_object: None,
            name: Some("Saved!".into()),
            ..Default::default()
        },
        presentation: Default::default(),
        children_count: None,
        subtree_truncated: false,
        children: vec![],
    }
}

#[test]
fn press_waits_in_requested_app_without_repeating_delivery() {
    let _home = crate::refs::refs_test_support::HomeGuard::new();
    for (query, gone, expected_error) in [
        (":Saved!", false, None),
        (":Missing", true, None),
        (":Missing", false, Some("TIMEOUT")),
        ("", false, Some("INVALID_ARGS")),
    ] {
        let adapter = CapturingAdapter {
            expect_scoped_windows: true,
            ..Default::default()
        };
        let mut request = args("a", false);
        request.app = Some("Editor".into());
        let context =
            CommandContext::default().with_wait_selector(Some(crate::context::WaitSelector {
                query_raw: query.into(),
                gone,
                timeout_ms: 20,
            }));
        let result = execute(request, &adapter, &context);
        if let Some(code) = expected_error {
            let error = result.unwrap_err();
            assert_eq!(error.code(), code);
            if code == "TIMEOUT" {
                let crate::AppError::Adapter(error) = error else {
                    panic!("adapter error")
                };
                assert_eq!(
                    error.disposition,
                    crate::DeliverySemantics::delivered_unverified()
                );
                assert_eq!(
                    error.details.unwrap()["after_action"]["disposition"]["delivery"],
                    "delivered_unverified"
                );
            }
        } else {
            let value = result.unwrap();
            assert_eq!(value["matched_selector"], query);
            assert_eq!(
                value["after_action"]["disposition"]["delivery"],
                "delivered_unverified"
            );
        }
        assert_eq!(
            adapter.dispatches.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(expected_error != Some("INVALID_ARGS"))
        );
    }
}
