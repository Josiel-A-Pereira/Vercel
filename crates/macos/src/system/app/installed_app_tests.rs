use super::find_installed_app;

#[test]
fn a_bundle_id_resolves_to_the_installed_app_and_its_name() {
    let finder = find_installed_app("com.apple.finder")
        .expect("lookup")
        .expect("Finder is always installed");
    assert_eq!(finder.bundle_id.as_deref(), Some("com.apple.finder"));
    assert!(!finder.name.is_empty());
}

#[test]
fn an_app_name_in_any_letter_case_resolves_to_its_bundle_id() {
    for spelling in ["Safari", "safari"] {
        let safari = find_installed_app(spelling)
            .expect("lookup")
            .unwrap_or_else(|| panic!("{spelling} names the installed Safari"));
        assert_eq!(
            safari.bundle_id.as_deref(),
            Some("com.apple.Safari"),
            "{spelling}"
        );
    }
}

#[test]
fn nothing_installed_answers_to_an_unknown_identifier() {
    for identifier in [
        "NoSuchApplicationZq9",
        "com.example.no-such-app-zq9",
        "",
        "../Safari",
    ] {
        assert_eq!(
            find_installed_app(identifier).expect("lookup"),
            None,
            "{identifier:?}"
        );
    }
}
