use super::*;

fn step(keysym: u32) -> KeysymStep {
    KeysymStep { keysym, down: true }
}

#[test]
fn control_characters_are_refused_in_every_keysym_form() {
    for code in [0x00_u32, 0x07, 0x0d, 0x1b, 0x1f, 0x7f, 0x85, 0x9f] {
        assert!(!typable(0x0100_0000 + code), "U+{code:04X} in Unicode form");
    }
    assert!(!typable(0x7f));
    assert!(!typable(0x1b));
    assert!(!typable(0));
}

#[test]
fn thai_latin_one_and_emoji_are_typable() {
    assert!(typable(0x0100_0e01));
    assert!(typable(0x0100_0e31));
    assert!(typable(0xe9));
    assert!(typable(0xa0));
    assert!(typable(0x0100_0000 + 0x1f600));
    assert!(typable(0x20));
    assert!(typable(0x7e));
}

#[test]
fn named_keys_are_typable() {
    assert!(typable(0xff0d));
    assert!(typable(0xff09));
    assert!(typable(0xffbe));
    assert!(typable(0xffe3));
}

#[test]
fn keysyms_outside_the_keysym_space_are_refused() {
    assert!(!typable(0x0111_0000));
    assert!(!typable(0x2000_0000));
}

#[test]
fn one_untypable_step_refuses_every_chunk_before_any_key() {
    let chunks = vec![vec![step(0x61)], vec![step(0x68), step(0x0100_0007)]];
    let error = refuse_untypable(&chunks).expect_err("the control character refuses");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    let details = error.details.as_ref().expect("details");
    assert_eq!(details["physical_delivery_started"], false);
    assert_eq!(details["cause"], "character_outside_layout");
    assert!(refuse_untypable(&[vec![step(0x61), step(0x0100_0e01)]]).is_ok());
}
