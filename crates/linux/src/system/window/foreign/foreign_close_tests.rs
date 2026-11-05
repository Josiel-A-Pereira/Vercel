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
    read_standing(records, &titles(frames), &thunar(), known, None)
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
fn two_related_records_remain_ambiguous_even_with_two_frames() {
    let records = [record(1, "Home", "thunar"), record(2, "Home", "thunar")];
    let standing = read(&records, &["Home", "Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 2);
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
    assert_eq!(standing.contested, 2);
}

#[test]
fn contested_count_includes_all_related_candidates() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Home", "thunar"),
        record(3, "Home", "thunar"),
        record(4, "Home", "thunar"),
    ];
    let standing = read(&records, &["Home", "Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 4);
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
    assert_eq!(standing.contested, 2);
}

#[test]
fn a_same_title_record_of_an_unrelated_app_id_leaves_identity_unproven() {
    let records = [record(1, "Home", "thunar"), record(2, "Home", "kitty")];
    let standing = read(&records, &["Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.unproven, vec!["thunar", "kitty"]);
}

#[test]
fn a_title_with_no_frame_and_no_known_ids_is_not_listed() {
    let standing = read(&[record(2, "Save changes?", "thunar")], &["Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_late_mapped_toplevel_is_a_dialog_when_known_is_not_empty() {
    let records = [
        record(1, "Home", "thunar"),
        record(2, "Save changes?", "thunar"),
    ];
    let owners = HashMap::from([(1, 7), (2, 7)]);
    let standing = read_standing(&records, &titles(&["Home"]), &thunar(), &[1], Some(&owners));
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
    let standing = read_standing(&records, &frames, &thunar(), &[], None);
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
    let owners = HashMap::from([(1, 7), (2, 7)]);
    let standing = read_standing(&records, &frames, &thunar(), &[], Some(&owners));
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
    let owners = HashMap::from([(1, 7), (9, 7)]);
    let standing = read_standing(&records, &titles(&["Doc"]), &thunar(), &[1], Some(&owners));
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
    let owners = HashMap::from([(1, 7), (2, 7), (9, 7)]);
    let standing = read_standing(
        &records,
        &titles(&["Doc", "Doc"]),
        &thunar(),
        &[1, 2],
        Some(&owners),
    );
    assert_eq!(ids(&standing), vec![1, 2, 9]);
    assert_eq!(standing.contested, 0);
}

#[test]
fn a_same_title_window_of_another_program_is_unproven() {
    let records = [
        record(1, "Home", "zzqqxx"),
        record(2, "Home", "thunar-viewer"),
    ];
    let standing = read(&records, &["Home"], &[]);
    assert!(standing.windows.is_empty());
    assert_eq!(standing.unproven, vec!["zzqqxx", "thunar-viewer"]);
}

#[test]
fn a_dialog_frame_without_a_related_toplevel_blocks_the_close() {
    let records = [
        record(1, "Doc", "thunar"),
        record(2, "Save As", "unrelated"),
    ];
    let frames = [titles(&["Doc"]).remove(0), prompt("Save As")];
    let standing = read_standing(&records, &frames, &thunar(), &[], None);
    assert_eq!(standing.dialogs, vec!["Save As"]);
}

#[test]
fn a_late_window_of_another_pid_is_not_a_target_prompt() {
    let records = [record(1, "Doc", "thunar"), record(9, "Save", "thunar")];
    let owners = HashMap::from([(1, 7), (9, 8)]);
    let standing = read_standing(&records, &titles(&["Doc"]), &thunar(), &[1], Some(&owners));
    assert_eq!(ids(&standing), vec![1]);
    assert!(standing.dialogs.is_empty());
    assert_eq!(standing.windows[0].kind, WindowKind::Normal);
}

#[test]
fn a_late_related_toplevel_with_no_owner_entry_is_unproven_in_the_mapped_branch() {
    let records = [record(1, "Doc", "thunar"), record(9, "Save", "thunar")];
    let owners = HashMap::from([(1, 7)]);
    let standing = read_standing(&records, &titles(&["Doc"]), &thunar(), &[1], Some(&owners));
    assert_eq!(ids(&standing), vec![1]);
    assert_eq!(standing.unproven, vec!["thunar"]);
}
