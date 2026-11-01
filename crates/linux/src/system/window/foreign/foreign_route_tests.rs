use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};

use super::reached;

#[test]
fn a_session_without_a_manager_yields_no_route() {
    assert!(reached::<()>(Ok(None)).is_none());
}

#[test]
fn an_opened_link_is_passed_through() {
    assert!(matches!(reached(Ok(Some(7_u8))), Some(Ok(7))));
}

#[test]
fn a_link_that_misses_its_deadline_is_a_failure_with_nothing_delivered() {
    let error = reached::<()>(Err(AdapterError::new(ErrorCode::Timeout, "late")))
        .expect("a route answer")
        .expect_err("a failure");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}
