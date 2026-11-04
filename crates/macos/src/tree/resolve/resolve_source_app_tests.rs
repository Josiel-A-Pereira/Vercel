use super::*;

fn names(values: &[&str]) -> Result<Vec<String>, AdapterError> {
    Ok(values.iter().map(|value| (*value).to_owned()).collect())
}

fn unexpected_names() -> Result<Vec<String>, AdapterError> {
    panic!("name evidence must not be read on this path")
}

#[test]
fn blank_title_uses_inventory_name() {
    for title in [None, Some(""), Some(" \t\n")] {
        assert!(
            source_app_matches("Fixture", title, || names(&["fIXTURE"]), unexpected_names).unwrap()
        );
    }
}

#[test]
fn blank_title_rejects_different_or_missing_inventory_name() {
    for inventory in [vec![], vec!["Other".into()]] {
        assert!(
            !source_app_matches("Fixture", Some(" "), || Ok(inventory), unexpected_names).unwrap()
        );
    }
}

#[test]
fn blank_title_matches_saved_cg_owner_names_for_same_process() {
    let records = [
        (7, Some("instance"), "CG Owner"),
        (7, Some("instance"), "CG Alias"),
        (8, Some("instance"), "Other PID"),
        (7, Some("reused-pid"), "Other Instance"),
    ];
    for (expected, matches) in [
        ("CG Owner", true),
        ("CG Alias", true),
        ("Workspace Display Name", false),
        ("Other PID", false),
        ("Other Instance", false),
    ] {
        assert_eq!(
            source_app_matches(
                expected,
                None,
                || Ok(source_names_for_process(7, Some("instance"), records)),
                unexpected_names,
            )
            .unwrap(),
            matches
        );
    }
}

#[test]
fn matching_title_never_reads_inventory_or_native_names() {
    assert!(
        source_app_matches(
            "Fixture",
            Some("fIXTURE"),
            unexpected_names,
            unexpected_names,
        )
        .unwrap()
    );
}

#[test]
fn readable_title_accepts_same_process_native_alias() {
    assert!(
        source_app_matches(
            "Brave Browser",
            Some("Brave"),
            || Ok(source_names_for_process(
                7,
                Some("instance"),
                [(7, Some("instance"), "Brave Browser")],
            )),
            || names(&["Brave Browser", "Brave"]),
        )
        .unwrap()
    );
}

#[test]
fn readable_title_rejects_unrelated_title_even_when_inventory_matches() {
    assert!(
        !source_app_matches(
            "Brave Browser",
            Some("Other"),
            || names(&["Brave Browser"]),
            || names(&["Brave Browser", "Brave"]),
        )
        .unwrap()
    );
}

#[test]
fn native_alias_requires_saved_inventory_name() {
    for inventory in [vec![], vec!["Other".into()]] {
        assert!(
            !source_app_matches(
                "Brave Browser",
                Some("Brave"),
                || Ok(inventory),
                || names(&["Brave"]),
            )
            .unwrap()
        );
    }
}

#[test]
fn readable_title_requires_native_alias_evidence() {
    for native in [vec![], vec!["Other".into()]] {
        assert!(
            !source_app_matches("Brave Browser", Some("Brave"), unexpected_names, || Ok(
                native
            ),)
            .unwrap()
        );
    }
}

#[test]
fn native_alias_rejects_other_process_inventory() {
    for (pid, instance) in [(8, Some("instance")), (7, Some("reused-pid")), (7, None)] {
        assert!(
            !source_app_matches(
                "Brave Browser",
                Some("Brave"),
                || Ok(source_names_for_process(
                    7,
                    Some("instance"),
                    [(pid, instance, "Brave Browser")],
                )),
                || names(&["Brave"]),
            )
            .unwrap()
        );
    }
    assert!(source_names_for_process(7, None, [(7, None, "Brave Browser")]).is_empty());
}

#[test]
fn inventory_read_error_propagates() {
    for title in [None, Some("Brave")] {
        let error = source_app_matches(
            "Brave Browser",
            title,
            || Err(AdapterError::timeout("inventory")),
            || names(&["Brave"]),
        )
        .unwrap_err();
        assert_eq!(error.code, agent_desktop_core::ErrorCode::Timeout);
        assert_eq!(error.message, "inventory");
    }
}

#[test]
fn native_names_read_error_propagates() {
    let error = source_app_matches("Brave Browser", Some("Brave"), unexpected_names, || {
        Err(AdapterError::timeout("native names"))
    })
    .unwrap_err();
    assert_eq!(error.code, agent_desktop_core::ErrorCode::Timeout);
    assert_eq!(error.message, "native names");
}

#[test]
fn missing_native_application_returns_no_aliases() {
    assert!(
        crate::system::appkit_bridge::application_names(i32::MAX)
            .unwrap()
            .is_empty()
    );
}
