use super::*;
use agent_desktop_core::ErrorCode;

#[test]
fn supplements_desktop_only_window_list_with_main_and_focused_windows() {
    let windows = collect(
        |attribute| {
            Ok(if attribute == "AXWindows" {
                vec![0]
            } else {
                vec![0, 3]
            })
        },
        |attribute| Ok(Some(if attribute == "AXMainWindow" { 1 } else { 2 })),
        |candidate| Ok(*candidate != 0),
        |a, b| a == b,
    )
    .unwrap();
    assert_eq!(windows, vec![1, 2, 3]);
}

#[test]
fn duplicate_window_sources_keep_one_candidate_in_order() {
    let windows = collect(
        |_| Ok(vec![1, 2]),
        |_| Ok(Some(1)),
        |_| Ok(true),
        |a, b| a == b,
    )
    .unwrap();
    assert_eq!(windows, vec![1, 2]);
}

#[test]
fn secondary_source_permission_error_does_not_return_partial_windows() {
    let error = collect(
        |_| Ok(vec![1]),
        |_| Err(AdapterError::new(ErrorCode::PermDenied, "unavailable")),
        |_| Ok(true),
        |a, b| a == b,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::PermDenied);
}

#[test]
fn role_read_error_is_preserved() {
    let error = collect(
        |_| Ok(vec![1]),
        |_| Ok(None),
        |_| Err(AdapterError::timeout("window role")),
        |a, b| a == b,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
}

#[test]
fn primary_source_error_does_not_fall_back_to_other_windows() {
    let error = collect::<i32>(
        |_| Err(AdapterError::timeout("window inventory")),
        |_| panic!("secondary reads must not hide a primary source failure"),
        |_| Ok(true),
        |a, b| a == b,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
}

#[test]
fn empty_optional_sources_leave_the_primary_window_available() {
    let windows = collect(
        |attribute| {
            Ok(if attribute == "AXWindows" {
                vec![1]
            } else {
                vec![]
            })
        },
        |_| Ok(None),
        |_| Ok(true),
        |a, b| a == b,
    )
    .unwrap();
    assert_eq!(windows, vec![1]);
}
