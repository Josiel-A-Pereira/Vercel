use super::GlobalFocus;

fn focus(window: &str, element: &str) -> GlobalFocus {
    GlobalFocus {
        window: window.to_owned(),
        element: element.to_owned(),
    }
}

#[test]
fn focus_identity_distinguishes_element_changes_from_window_activation() {
    let before = focus("finder", "search-field");
    assert_eq!(
        focus("finder", "search-field").effects_from(&before),
        (false, false)
    );
    assert_eq!(
        focus("finder", "file-list").effects_from(&before),
        (true, false)
    );
    assert_eq!(
        focus("text-edit", "body").effects_from(&before),
        (true, true)
    );
}
