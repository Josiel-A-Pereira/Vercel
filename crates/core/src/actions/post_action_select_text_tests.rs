use super::*;
use crate::{ActionOps, Deadline, InputOps, ObservationOps, SystemOps, TextSelection};
use serde_json::json;

struct Adapter(ActionResult);

impl ObservationOps for Adapter {}
impl InputOps for Adapter {}
impl SystemOps for Adapter {}
impl ActionOps for Adapter {
    fn execute_action(
        &self,
        _: &NativeHandle,
        _: ActionRequest,
        _: &InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        Ok(self.0.clone())
    }
}

fn execute(result: ActionResult) -> Result<ActionResult, AdapterError> {
    let adapter = Adapter(result);
    let request = ActionRequest::headless(Action::SelectText(TextSelection::Range {
        start: 2,
        end: 5,
    }));
    let lease = InteractionLease::guarded(Deadline::after(1000).unwrap(), ()).unwrap();
    execute_verified_action(&adapter, &NativeHandle::null(), request, &lease)
}

#[test]
fn confirmed_existing_selection_succeeds_without_delivery() {
    let result = ActionResult::satisfied_without_delivery("select-text");
    let observed = execute(result).unwrap();
    assert_eq!(observed.disposition(), DeliverySemantics::not_delivered());
}

#[test]
fn delivered_but_unverified_selection_fails() {
    let result = ActionResult::delivered_unverified("select-text").with_details(json!({
        "verification_scope": "text_selection",
    }));
    let error = execute(result).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
}
