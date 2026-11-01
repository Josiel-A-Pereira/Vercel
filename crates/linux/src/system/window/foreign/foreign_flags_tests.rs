use super::*;

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn an_empty_array_sets_nothing() {
    assert_eq!(
        ToplevelFlags::from_state_words(&[]),
        ToplevelFlags::default()
    );
}

#[test]
fn every_known_word_sets_its_flag() {
    let flags = ToplevelFlags::from_state_words(&words(&[0, 1, 2, 3]));
    assert!(flags.maximized && flags.minimized && flags.activated && flags.fullscreen);
}

#[test]
fn one_word_sets_only_its_flag() {
    let flags = ToplevelFlags::from_state_words(&words(&[2]));
    assert!(flags.activated);
    assert!(!flags.maximized && !flags.minimized && !flags.fullscreen);
}

#[test]
fn unknown_words_are_ignored() {
    let flags = ToplevelFlags::from_state_words(&words(&[7, 1, 4096]));
    assert!(flags.minimized);
    assert!(!flags.maximized && !flags.activated && !flags.fullscreen);
}

#[test]
fn words_are_read_little_endian() {
    let flags = ToplevelFlags::from_state_words(&[2, 0, 0, 0]);
    assert!(flags.activated);
    let big_endian = ToplevelFlags::from_state_words(&[0, 0, 0, 2]);
    assert_eq!(big_endian, ToplevelFlags::default());
}

#[test]
fn an_odd_length_array_drops_the_trailing_bytes() {
    let mut bytes = words(&[1]);
    bytes.extend([2, 0]);
    let flags = ToplevelFlags::from_state_words(&bytes);
    assert!(flags.minimized);
    assert!(!flags.activated);
}

#[test]
fn an_array_shorter_than_a_word_sets_nothing() {
    assert_eq!(
        ToplevelFlags::from_state_words(&[1, 0, 0]),
        ToplevelFlags::default()
    );
}
