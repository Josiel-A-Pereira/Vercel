use super::*;

fn names(list: &[&str]) -> ProcessNames {
    ProcessNames::from_names(list.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn thunar_matches_its_own_app_id() {
    assert!(names(&["thunar", "thunar"]).relates("thunar"));
}

#[test]
fn pavucontrol_matches_its_reverse_domain_app_id() {
    let process = names(&["pavucontrol", "pavucontrol"]);
    assert!(process.relates("org.pulseaudio.pavucontrol"));
}

#[test]
fn a_gtk_script_matches_through_its_script_name() {
    let process = names(&["python3.14", "python3", "python3.14", "gtkapp.py"]);
    assert!(process.relates("gtkapp.py"));
}

#[test]
fn a_gtk_script_without_its_script_name_does_not_match() {
    let process = names(&["python3.14", "python3", "python3.14"]);
    assert!(!process.relates("gtkapp.py"));
}

#[test]
fn kitty_does_not_relate_to_thunar() {
    assert!(!names(&["thunar"]).relates("kitty"));
    assert!(!names(&["kitty"]).relates("thunar"));
}

#[test]
fn an_empty_app_id_relates_to_nothing() {
    assert!(!names(&["thunar"]).relates(""));
    assert!(!names(&["thunar"]).relates("..."));
}

#[test]
fn case_and_punctuation_do_not_matter() {
    assert!(names(&["Firefox-ESR"]).relates("firefox_esr"));
}

#[test]
fn a_short_name_is_not_matched_by_containment() {
    assert!(!names(&["foo"]).relates("org.example.foo"));
    assert!(!names(&["org.example.foo"]).relates("foo"));
}

#[test]
fn a_short_name_still_matches_when_equal() {
    assert!(names(&["foo"]).relates("foo"));
    assert!(names(&["gimp"]).relates("org.gimp.gimp"));
}

#[test]
fn a_process_with_no_names_relates_to_nothing() {
    assert!(!ProcessNames::default().relates("thunar"));
}

#[test]
fn the_names_of_this_process_include_the_app_name_and_its_comm() {
    let process = ProcessNames::of(std::process::id(), "Probe Application");
    assert!(process.relates("probe application"));
    assert!(process.names.len() > 1);
}

#[test]
fn an_unreadable_process_keeps_only_the_app_name() {
    let process = ProcessNames::of(u32::MAX, "thunar");
    assert_eq!(process.names, vec!["thunar".to_owned()]);
}

#[test]
fn interpreters_are_recognised_by_prefix() {
    assert!(is_interpreter("python3.14"));
    assert!(is_interpreter("bash"));
    assert!(is_interpreter("node"));
    assert!(!is_interpreter("thunar"));
}
