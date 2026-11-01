use super::*;
use crate::system::window::foreign::foreign_flags::ToplevelFlags;

fn record(id: u32, title: &str, app_id: &str) -> ToplevelRecord {
    ToplevelRecord {
        id,
        title: title.to_owned(),
        app_id: app_id.to_owned(),
        flags: ToplevelFlags::default(),
    }
}

fn thunar() -> ProcessNames {
    ProcessNames::from_names(vec!["thunar".to_owned()])
}

#[test]
fn one_related_window_with_the_title_is_proven() {
    let records = [record(1, "Home", "thunar"), record(2, "Other", "thunar")];
    assert_eq!(prove("Home", &thunar(), &records), Proof::One(1));
}

#[test]
fn two_related_windows_with_the_same_title_are_several() {
    let records = [record(1, "Home", "thunar"), record(2, "Home", "thunar")];
    assert_eq!(
        prove("Home", &thunar(), &records),
        Proof::Several(vec![1, 2])
    );
}

#[test]
fn a_title_nobody_has_is_no_window() {
    let records = [record(1, "Home", "thunar")];
    assert_eq!(prove("Away", &thunar(), &records), Proof::NoWindow);
}

#[test]
fn no_records_at_all_is_no_window() {
    assert_eq!(prove("Home", &thunar(), &[]), Proof::NoWindow);
}

#[test]
fn an_empty_title_is_no_window() {
    let records = [record(1, "", "thunar")];
    assert_eq!(prove("", &thunar(), &records), Proof::NoWindow);
}

#[test]
fn a_title_held_only_by_another_program_is_unrelated() {
    let records = [record(1, "Home", "kitty"), record(2, "Home", "foot")];
    assert_eq!(
        prove("Home", &thunar(), &records),
        Proof::Unrelated(vec!["kitty".to_owned(), "foot".to_owned()])
    );
}

#[test]
fn an_unrelated_window_with_the_same_title_does_not_make_a_related_one_ambiguous() {
    let records = [record(1, "Home", "kitty"), record(2, "Home", "thunar")];
    assert_eq!(prove("Home", &thunar(), &records), Proof::One(2));
}

#[test]
fn titles_are_compared_exactly() {
    let records = [record(1, "Home ", "thunar")];
    assert_eq!(prove("Home", &thunar(), &records), Proof::NoWindow);
}
