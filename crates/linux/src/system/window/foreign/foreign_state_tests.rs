use super::*;
use crate::system::window::foreign::foreign_flags::ToplevelFlags;
use crate::system::window::foreign::foreign_pending::Pending;

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn previous() -> ToplevelRecord {
    ToplevelRecord {
        id: 9,
        title: "Home".to_owned(),
        app_id: "thunar".to_owned(),
        flags: ToplevelFlags {
            activated: true,
            ..ToplevelFlags::default()
        },
    }
}

fn full_batch() -> Pending {
    Pending {
        title: Some("Docs".to_owned()),
        app_id: Some("kitty".to_owned()),
        flags: Some(ToplevelFlags::from_state_words(&words(&[0, 1]))),
    }
}

#[test]
fn a_first_batch_becomes_the_record_with_the_handle_id() {
    let record = full_batch().settle(4, None);
    assert_eq!(record.id, 4);
    assert_eq!(record.title, "Docs");
    assert_eq!(record.app_id, "kitty");
    assert!(record.flags.maximized && record.flags.minimized);
    assert!(!record.flags.activated && !record.flags.fullscreen);
}

#[test]
fn a_first_batch_with_nothing_in_it_is_an_empty_record() {
    let record = Pending::default().settle(4, None);
    assert_eq!(record.title, "");
    assert_eq!(record.app_id, "");
    assert_eq!(record.flags, ToplevelFlags::default());
}

#[test]
fn a_partial_batch_keeps_the_values_it_did_not_mention() {
    let mut pending = Pending {
        title: Some("Renamed".to_owned()),
        ..Pending::default()
    };
    let record = pending.settle(9, Some(&previous()));
    assert_eq!(record.title, "Renamed");
    assert_eq!(record.app_id, "thunar");
    assert!(record.flags.activated);
}

#[test]
fn a_state_event_replaces_every_flag_at_once() {
    let mut pending = Pending {
        flags: Some(ToplevelFlags::from_state_words(&[])),
        ..Pending::default()
    };
    let record = pending.settle(9, Some(&previous()));
    assert!(!record.flags.activated);
    assert_eq!(record.title, "Home");
}

#[test]
fn settling_clears_what_was_held_so_the_next_batch_starts_empty() {
    let mut pending = full_batch();
    pending.settle(9, None);
    assert!(pending.title.is_none() && pending.app_id.is_none() && pending.flags.is_none());
    let again = pending.settle(9, Some(&previous()));
    assert_eq!(again, previous());
}

#[test]
fn state_words_decode_each_known_value_and_skip_the_rest() {
    let mut bytes = words(&[0, 1, 2, 3, 99]);
    bytes.extend([1, 0]);
    let flags = ToplevelFlags::from_state_words(&bytes);
    assert!(flags.maximized && flags.minimized && flags.activated && flags.fullscreen);
    let only_unknown = ToplevelFlags::from_state_words(&words(&[4, 5]));
    assert_eq!(only_unknown, ToplevelFlags::default());
}

#[test]
fn a_fresh_state_holds_no_toplevels_and_forgets_unknown_ids_quietly() {
    let mut state = ForeignState::default();
    assert!(state.records().is_empty());
    assert!(state.handle_of(3).is_none());
    assert!(state.roster.tracked_mut(3).is_none());
    state.roster.settle(3);
    state.forget(3);
    assert!(state.records().is_empty());
}

#[test]
fn a_sync_callback_sets_the_shared_flag() {
    let mut state = ForeignState::default();
    assert!(!*state.sync_done());
    *state.sync_done() = true;
    assert!(*state.sync_done());
}
