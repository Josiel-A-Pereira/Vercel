use super::*;
use crate::system::window::foreign::foreign_fake::{record, thunar};

fn titles(frames: &[&str]) -> Vec<Frame> {
    frames
        .iter()
        .map(|title| Frame {
            title: (*title).to_owned(),
            prompt: false,
        })
        .collect()
}

fn prompt(title: &str) -> Frame {
    Frame {
        title: title.to_owned(),
        prompt: true,
    }
}

fn ids(standing: &ForeignStanding) -> Vec<u32> {
    standing.windows.iter().map(|window| window.id).collect()
}

fn read(records: &[ToplevelRecord], frames: &[&str], known: &[u32]) -> ForeignStanding {
    read_standing(records, &titles(frames), &thunar(), known)
}

#[test]
fn one_frame_and_one_record_with_the_title_is_listed_as_a_normal_window() {
    let standing = read(&[record(1, "Home", "thunar")], &["Home"], &[]);
    assert_eq!(standing.contested, 0);
    assert_eq!(standing.windows.len(), 1);
    let window = &standing.windows[0];
    assert_eq!((window.id, window.title.as_str()), (1, "Home"));
    assert_eq!(window.kind, WindowKind::Normal);
    assert!(!window.transient);
}

#[test]
fn as_many_records_as_frames_are_all_listed() {
    let records = [record(1, "Home", "thunar"), record(2, "Home", "thunar")];
    let standing = read(&records, &["Home", "Home"], &[]);
    assert_eq!(ids(&standing), vec![1, 2]);
    assert_eq!(standing.contested, 0);
}

#[test]
fn fewer_records_than_frames_are_all_listed() {
    let standing = read(&[record(1, "Home", "thunar")], &["Home", "Home"], &[]);
    assert_eq!(ids(&standing), vec![1]);
    assert_eq!(standing.contested, 0);
}

#[test]
fn more_records_than_frames_are_contested_and_none_is_listed() {
    let records = [record(1, "Home", "thunar"), record(2, "Home", "thunar")];
    let standing = read(&records, &["Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 1);
}

#[test]
fn the_contested_count_is_the_surplus_of_records_over_frames() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Home", "thunar"),
        record(3, "Home", "thunar"),
        record(4, "Home", "thunar"),
    ];
    let standing = read(&records, &["Home", "Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 2);
}

#[test]
fn a_contested_title_does_not_hide_a_proven_one() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Home", "thunar"),
        record(3, "Docs", "thunar"),
    ];
    let standing = read(&records, &["Home", "Docs"], &[]);
    assert_eq!(ids(&standing), vec![3]);
    assert_eq!(standing.contested, 1);
}

#[test]
fn records_of_unrelated_app_ids_are_ignored() {
    let records = [record(1, "Home", "thunar"), record(2, "Home", "kitty")];
    let standing = read(&records, &["Home"], &[]);
    assert_eq!(ids(&standing), vec![1]);
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_title_with_no_frame_and_no_known_ids_is_not_listed() {
    let standing = read(&[record(2, "Save changes?", "thunar")], &["Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_late_unmatched_related_toplevel_is_a_dialog_when_known_is_not_empty() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Save changes?", "thunar"),
    ];
    let standing = read(&records, &["Home"], &[1]);
    assert_eq!(ids(&standing), vec![1, 2]);
    assert_eq!(standing.windows[0].kind, WindowKind::Normal);
    assert_eq!(standing.windows[1].kind, WindowKind::Dialog);
    assert!(!standing.windows[1].transient);
}

#[test]
fn an_unmatched_toplevel_whose_id_is_known_is_not_a_dialog() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Save changes?", "thunar"),
    ];
    let standing = read(&records, &["Home"], &[1, 2]);
    assert_eq!(ids(&standing), vec![1]);
}

#[test]
fn an_unmatched_toplevel_of_another_program_is_never_a_dialog() {
    let records = [record(1, "Home", "thunar"), record(2, "Save?", "kitty")];
    let standing = read(&records, &["Home"], &[1]);
    assert_eq!(ids(&standing), vec![1]);
}

#[test]
fn nothing_open_and_no_frames_is_an_empty_standing() {
    let standing = read(&[], &[], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_frame_the_toolkit_presents_as_a_prompt_makes_its_toplevel_a_dialog() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Save changes?", "thunar"),
    ];
    let mut frames = titles(&["Home"]);
    frames.push(prompt("Save changes?"));
    let standing = read_standing(&records, &frames, &thunar(), &[]);
    assert_eq!(ids(&standing), vec![1, 2]);
    assert_eq!(standing.windows[0].kind, WindowKind::Normal);
    assert_eq!(standing.windows[1].kind, WindowKind::Dialog);
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_prompt_sharing_its_windows_title_lists_every_toplevel_of_that_title_as_a_dialog() {
    let records = [record(1, "Doc", "thunar"), record(2, "Doc", "thunar")];
    let mut frames = titles(&["Doc"]);
    frames.push(prompt("Doc"));
    let standing = read_standing(&records, &frames, &thunar(), &[]);
    assert_eq!(ids(&standing), vec![1, 2]);
    assert!(
        standing
            .windows
            .iter()
            .all(|window| window.kind == WindowKind::Dialog)
    );
}

#[test]
fn a_toplevel_that_appeared_after_the_first_request_is_a_dialog_even_with_a_framed_title() {
    let records = [record(1, "Doc", "thunar"), record(9, "Doc", "thunar")];
    let standing = read(&records, &["Doc"], &[1]);
    assert_eq!(ids(&standing), vec![1, 9]);
    assert_eq!(standing.windows[0].kind, WindowKind::Normal);
    assert_eq!(standing.windows[1].kind, WindowKind::Dialog);
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_late_toplevel_does_not_make_a_known_one_contested() {
    let records = [
        record(1, "Doc", "thunar"),
        record(2, "Doc", "thunar"),
        record(9, "Doc", "thunar"),
    ];
    let standing = read(&records, &["Doc", "Doc"], &[1, 2]);
    assert_eq!(ids(&standing), vec![1, 2, 9]);
    assert_eq!(standing.contested, 0);
}
