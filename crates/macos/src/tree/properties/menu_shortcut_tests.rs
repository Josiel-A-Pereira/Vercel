use super::imp::{format_shortcut, virtual_key_name};

#[test]
fn command_only_letter_uses_lowercase_char() {
    assert_eq!(
        format_shortcut("c", Some(0), None).as_deref(),
        Some("cmd+c")
    );
}

#[test]
fn modifier_bits_map_to_words_in_fixed_order() {
    let ctrl_option_shift_cmd = 4 | 2 | 1;
    assert_eq!(
        format_shortcut("p", Some(ctrl_option_shift_cmd), None).as_deref(),
        Some("ctrl+opt+shift+cmd+p")
    );
}

#[test]
fn no_command_bit_suppresses_the_command_word() {
    assert_eq!(format_shortcut("1", Some(8), None).as_deref(), Some("1"));
    assert_eq!(
        format_shortcut("1", Some(8 | 2), None).as_deref(),
        Some("opt+1")
    );
}

#[test]
fn virtual_key_name_wins_over_the_character() {
    assert_eq!(
        format_shortcut("", Some(0), Some(36)).as_deref(),
        Some("cmd+return")
    );
    assert_eq!(
        format_shortcut("x", Some(0), Some(121)).as_deref(),
        Some("cmd+pagedown")
    );
}

#[test]
fn uppercase_char_is_lowered() {
    assert_eq!(
        format_shortcut("Z", Some(0), None).as_deref(),
        Some("cmd+z")
    );
}

#[test]
fn nothing_bound_yields_none() {
    assert_eq!(format_shortcut("", None, None), None);
    assert_eq!(format_shortcut("", Some(0), None), None);
    assert_eq!(format_shortcut("", None, Some(200)), None);
}

#[test]
fn virtual_key_table_matches_the_press_vocabulary() {
    assert_eq!(virtual_key_name(0), Some("a"));
    assert_eq!(virtual_key_name(36), Some("return"));
    assert_eq!(virtual_key_name(48), Some("tab"));
    assert_eq!(virtual_key_name(49), Some("space"));
    assert_eq!(virtual_key_name(51), Some("delete"));
    assert_eq!(virtual_key_name(53), Some("escape"));
    assert_eq!(virtual_key_name(117), Some("forwarddelete"));
    assert_eq!(virtual_key_name(121), Some("pagedown"));
    assert_eq!(virtual_key_name(122), Some("f1"));
    assert_eq!(virtual_key_name(123), Some("left"));
    assert_eq!(virtual_key_name(127), None);
}
