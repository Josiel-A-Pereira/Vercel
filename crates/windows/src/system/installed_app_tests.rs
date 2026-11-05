#[cfg(target_os = "windows")]
mod windows_only {
    use super::super::find_installed_app;

    #[test]
    fn a_bare_name_in_any_letter_case_is_named_by_its_image_file() {
        for spelling in ["cmd", "CMD", "cmd.exe", "Cmd.EXE"] {
            let app = find_installed_app(spelling)
                .unwrap_or_else(|| panic!("{spelling} names System32's cmd.exe"));
            assert_eq!(app.name, "cmd.exe", "{spelling}");
            assert_eq!(app.bundle_id, None);
        }
    }

    #[test]
    fn nothing_installed_answers_to_an_unknown_or_unsafe_identifier() {
        for identifier in ["no-such-program-zq9", "", r"..\cmd", r"dir\cmd"] {
            assert_eq!(find_installed_app(identifier), None, "{identifier:?}");
        }
    }
}
