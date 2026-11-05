use std::ops::Range;

use super::{
    STABILITY, SelectionProgress, SelectionRead, is_secure_text, observe_selection,
    selection_matches,
};
use std::time::Instant;

#[test]
fn selected_text_requires_one_matching_range_in_both_accessibility_attributes() {
    let expected = 4..12;
    let exact = SelectionRead {
        count: Some(1),
        only_range: Some(expected.clone()),
    };

    assert!(selection_matches(&exact, &expected, &expected));
    assert!(!selection_matches(
        &SelectionRead {
            count: Some(2),
            only_range: None,
        },
        &expected,
        &expected,
    ));
    assert!(!selection_matches(
        &SelectionRead {
            count: Some(0),
            only_range: None,
        },
        &expected,
        &expected,
    ));
    assert!(!selection_matches(
        &SelectionRead {
            count: Some(1),
            only_range: Some(5..12),
        },
        &expected,
        &expected,
    ));
}

#[test]
fn absent_selection_list_uses_the_primary_range_for_verification() {
    let expected = 4..12;
    let absent = SelectionRead {
        count: None,
        only_range: None,
    };

    assert!(selection_matches(&absent, &expected, &expected));
    assert!(!selection_matches(&absent, &(5..12), &expected));
}

#[test]
fn empty_text_selection_verifies_as_a_single_caret_range() {
    let caret = 7..7;
    let selected = SelectionRead {
        count: Some(1),
        only_range: Some(caret.clone()),
    };

    assert!(selection_matches(&selected, &caret, &caret));
    assert_eq!(caret, Range { start: 7, end: 7 });
}

#[test]
fn secure_text_is_rejected_from_either_native_role_attribute() {
    assert!(is_secure_text(Some("AXSecureTextField"), None));
    assert!(is_secure_text(
        Some("AXTextField"),
        Some("AXSecureTextField")
    ));
    assert!(!is_secure_text(Some("AXTextField"), Some("AXSearchField")));
}

#[test]
fn asynchronous_selection_may_appear_once_but_cannot_clear_afterward() {
    let start = Instant::now();
    let mut exact_since = None;

    assert_eq!(
        observe_selection(false, &mut exact_since, start),
        SelectionProgress::Waiting
    );
    assert_eq!(
        observe_selection(true, &mut exact_since, start),
        SelectionProgress::Waiting
    );
    assert_eq!(
        observe_selection(false, &mut exact_since, start + STABILITY),
        SelectionProgress::Cleared
    );
    assert_eq!(
        observe_selection(true, &mut exact_since, start + STABILITY),
        SelectionProgress::Stable
    );
}
