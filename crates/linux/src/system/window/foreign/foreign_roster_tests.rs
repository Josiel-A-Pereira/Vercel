use super::*;

fn shown(roster: &mut Roster<u32>, handle: u32, title: &str) -> u32 {
    let serial = roster.announce(handle);
    if let Some(entry) = roster.tracked_mut(serial) {
        entry.pending.title = Some(title.to_owned());
    }
    roster.settle(serial);
    serial
}

#[test]
fn a_reused_protocol_id_gets_a_new_serial_and_never_reaches_the_closed_toplevel() {
    let mut roster = Roster::<u32>::default();
    let closed = shown(&mut roster, 7, "Old");
    assert_eq!(roster.forget(closed), Some(7));
    let reused = shown(&mut roster, 7, "New");
    assert_ne!(closed, reused);
    assert!(roster.handle_of(closed).is_none());
    assert_eq!(roster.handle_of(reused), Some(&7));
    let titles: Vec<_> = roster
        .records()
        .into_iter()
        .map(|each| each.title)
        .collect();
    assert_eq!(titles, ["New"]);
    assert!(roster.forget(closed).is_none());
    assert_eq!(roster.records().len(), 1);
}

#[test]
fn a_handle_is_found_by_equality_and_an_unannounced_one_is_not() {
    let mut roster = Roster::<u32>::default();
    let first = roster.announce(3);
    assert_eq!(roster.serial_of(&3), Some(first));
    assert_eq!(roster.serial_of(&4), None);
}

#[test]
fn a_toplevel_without_its_first_done_has_no_handle_to_send_to() {
    let mut roster = Roster::<u32>::default();
    let serial = roster.announce(5);
    assert!(roster.handle_of(serial).is_none());
    assert!(roster.shown_handles().is_empty());
    roster.settle(serial);
    assert_eq!(roster.shown_handles(), vec![(serial, &5)]);
}
