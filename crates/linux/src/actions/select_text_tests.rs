use agent_desktop_core::{DeliverySemantics, ErrorCode};

use super::{ensure_focus_available, ensure_safe_role, ensure_text_target};
use crate::actions::select_text_focus::GlobalFocus;
use crate::tree::live::test_support::read_of;
use crate::tree::vocabulary::role_numbers::{ENTRY, PASSWORD_TEXT};
use crate::tree::{Field, RawNode};

fn raw_with_states(states: Field<Vec<u32>>) -> RawNode {
    let mut node = read_of(ENTRY, "editor", &[]).node;
    node.traits.states = states;
    node.traits.interfaces = Field::Present(vec!["org.a11y.atspi.Text".to_owned()]);
    node
}

fn focus() -> GlobalFocus {
    GlobalFocus {
        window: "text-edit".to_owned(),
        element: "editor".to_owned(),
    }
}

#[test]
fn text_selection_requires_a_known_non_password_role() {
    for role in [None, Some(u32::MAX), Some(PASSWORD_TEXT)] {
        let error = ensure_safe_role(role).expect_err("unknown and password roles are refused");
        assert_eq!(error.code, ErrorCode::ActionNotSupported);
    }
    assert!(ensure_safe_role(Some(ENTRY)).is_ok());
}

#[test]
fn text_selection_refuses_unknown_state_before_reading_text() {
    for states in [
        Field::Absent,
        Field::Failed("states unavailable".to_owned()),
    ] {
        let error = ensure_text_target(&raw_with_states(states))
            .expect_err("unknown state may indicate a secure field");
        assert_eq!(error.code, ErrorCode::ActionNotSupported);
        assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    }
}

#[test]
fn text_selection_refuses_when_global_focus_is_unknown() {
    let error = ensure_focus_available(None).expect_err("unknown focus is refused");
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(ensure_focus_available(Some(&focus())).is_ok());
}
