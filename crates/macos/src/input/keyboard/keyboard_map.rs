use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode, KeyCombo, Modifier};

use crate::input::keyboard::keyboard_layout::LayoutLookup;

const PUNCTUATION: [(char, &str); 11] = [
    (',', "comma"),
    ('.', "period"),
    ('/', "slash"),
    (';', "semicolon"),
    ('\'', "quote"),
    ('[', "leftbracket"),
    (']', "rightbracket"),
    ('\\', "backslash"),
    ('-', "minus"),
    ('=', "equal"),
    ('`', "grave"),
];

pub(crate) fn punctuation(key: &str) -> Option<char> {
    PUNCTUATION
        .iter()
        .find(|(symbol, name)| *name == key || key.chars().eq(std::iter::once(*symbol)))
        .map(|(symbol, _)| *symbol)
}

/// Returns the virtual key code to send for `combo`. A letter or punctuation
/// key is the one that types it in the active keyboard layout, read with
/// Command held when the combo holds Command, because macOS matches Command
/// shortcuts against that key map. A letter keeps its US ANSI key when the
/// layout cannot be read or has no key for it.
pub(crate) fn key_code(combo: &KeyCombo) -> Result<u16, AdapterError> {
    let command_held = combo.modifiers.contains(&Modifier::Meta);
    code_in_layout(&combo.key, |symbol| {
        crate::input::keyboard::keyboard_layout::code_for_symbol(symbol, command_held)
    })
}

fn code_in_layout(key: &str, layout: impl Fn(char) -> LayoutLookup) -> Result<u16, AdapterError> {
    if let Some(letter) = letter(key)
        && let LayoutLookup::Found(code) = layout(letter)
    {
        return Ok(code);
    }
    if let Some(symbol) = punctuation(key) {
        return layout_code(symbol, layout(symbol));
    }
    key_name_to_code(key)
}

fn letter(key: &str) -> Option<char> {
    let mut characters = key.chars();
    match (characters.next(), characters.next()) {
        (Some(letter), None) if letter.is_ascii_lowercase() => Some(letter),
        _ => None,
    }
}

fn key_name_to_code(key: &str) -> Result<u16, AdapterError> {
    let code = match key {
        "a" => 0,
        "b" => 11,
        "c" => 8,
        "d" => 2,
        "e" => 14,
        "f" => 3,
        "g" => 5,
        "h" => 4,
        "i" => 34,
        "j" => 38,
        "k" => 40,
        "l" => 37,
        "m" => 46,
        "n" => 45,
        "o" => 31,
        "p" => 35,
        "q" => 12,
        "r" => 15,
        "s" => 1,
        "t" => 17,
        "u" => 32,
        "v" => 9,
        "w" => 13,
        "x" => 7,
        "y" => 16,
        "z" => 6,
        "0" => 29,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "5" => 23,
        "6" => 22,
        "7" => 26,
        "8" => 28,
        "9" => 25,
        "return" | "enter" => 36,
        "escape" | "esc" => 53,
        "tab" => 48,
        "space" => 49,
        "delete" | "backspace" => 51,
        "forwarddelete" => 117,
        "home" => 115,
        "end" => 119,
        "pageup" => 116,
        "pagedown" => 121,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        "cmd" | "command" => 55,
        "shift" => 56,
        "alt" | "option" => 58,
        "ctrl" | "control" => 59,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        other => {
            return Err(AdapterError::new(
                ErrorCode::InvalidArgs,
                format!("Unknown key: '{other}'"),
            )
            .with_suggestion("Valid keys: a-z, 0-9, return, escape, tab, space, delete, left, right, up, down, f1-f12, and , . / ; ' [ ] \\ - = ` (or comma, period, slash, semicolon, quote, leftbracket, rightbracket, backslash, minus, equal, grave)"));
        }
    };
    Ok(code)
}

fn layout_code(symbol: char, lookup: LayoutLookup) -> Result<u16, AdapterError> {
    match lookup {
        LayoutLookup::Found(code) => Ok(code),
        LayoutLookup::NoKeyForSymbol => Err(AdapterError::new(
            ErrorCode::InvalidArgs,
            format!("No key types '{symbol}' in the active keyboard layout without Shift or Option"),
        )
        .with_suggestion(
            "This symbol needs a modifier or is missing in the active layout; nothing was sent",
        )),
        LayoutLookup::LayoutUnavailable => Err(AdapterError::new(
            ErrorCode::ActionFailed,
            format!("Could not read the active keyboard layout to resolve '{symbol}'"),
        )
        .with_disposition(DeliverySemantics::not_delivered())
        .with_suggestion(
            "Nothing was sent; the layout is readable only from the process main thread. Retry from the main thread or with a named key",
        )),
    }
}

#[cfg(test)]
mod tests {
    use agent_desktop_core::{DeliverySemantics, ErrorCode, KeyCombo, Modifier};

    use super::{LayoutLookup, key_name_to_code};

    #[test]
    fn named_key_aliases_resolve_to_same_code() {
        assert_eq!(key_name_to_code("return").unwrap(), 36);
        assert_eq!(key_name_to_code("enter").unwrap(), 36);
        assert_eq!(key_name_to_code("escape").unwrap(), 53);
        assert_eq!(key_name_to_code("esc").unwrap(), 53);
        assert_eq!(key_name_to_code("alt").unwrap(), 58);
        assert_eq!(key_name_to_code("option").unwrap(), 58);
        assert_eq!(key_name_to_code("cmd").unwrap(), 55);
        assert_eq!(key_name_to_code("command").unwrap(), 55);
        assert_eq!(key_name_to_code("ctrl").unwrap(), 59);
        assert_eq!(key_name_to_code("control").unwrap(), 59);
    }

    #[test]
    fn navigation_and_function_keys_map_to_expected_codes() {
        assert_eq!(key_name_to_code("f1").unwrap(), 122);
        assert_eq!(key_name_to_code("f12").unwrap(), 111);
        assert_eq!(key_name_to_code("tab").unwrap(), 48);
        assert_eq!(key_name_to_code("delete").unwrap(), 51);
        assert_eq!(key_name_to_code("backspace").unwrap(), 51);
        assert_eq!(key_name_to_code("left").unwrap(), 123);
        assert_eq!(key_name_to_code("up").unwrap(), 126);
    }

    #[test]
    fn punctuation_resolves_by_symbol_and_by_name() {
        for (symbol, name) in super::PUNCTUATION {
            assert_eq!(super::punctuation(&symbol.to_string()), Some(symbol));
            assert_eq!(super::punctuation(name), Some(symbol));
        }
        assert_eq!(super::punctuation(",,"), None);
    }

    #[test]
    fn punctuation_is_sent_only_when_the_active_layout_resolves_it() {
        use crate::input::keyboard::keyboard_layout::LayoutLookup;
        assert_eq!(
            super::layout_code(',', LayoutLookup::Found(43)).unwrap(),
            43
        );
        assert_eq!(
            super::layout_code('[', LayoutLookup::Found(30)).unwrap(),
            30
        );
        let err = super::layout_code('`', LayoutLookup::NoKeyForSymbol).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidArgs);
        assert!(err.message.contains('`'));
    }

    #[test]
    fn an_unreadable_layout_is_an_environment_fault_that_is_safe_to_retry() {
        use crate::input::keyboard::keyboard_layout::LayoutLookup;
        let err = super::layout_code('`', LayoutLookup::LayoutUnavailable).unwrap_err();
        assert_eq!(err.code, ErrorCode::ActionFailed);
        assert_eq!(err.disposition, DeliverySemantics::not_delivered());
        assert!(err.message.contains('`'));
    }

    #[test]
    fn unknown_key_name_returns_invalid_args_error_with_suggestion() {
        let err = key_name_to_code("hyperkey").unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidArgs);
        assert!(err.message.contains("hyperkey"));
        assert!(err.suggestion.is_some());
    }

    fn french_azerty(symbol: char) -> LayoutLookup {
        match symbol {
            'q' => LayoutLookup::Found(0),
            'w' => LayoutLookup::Found(6),
            'a' => LayoutLookup::Found(12),
            'z' => LayoutLookup::Found(13),
            'm' => LayoutLookup::Found(41),
            _ => LayoutLookup::NoKeyForSymbol,
        }
    }

    #[test]
    fn letters_are_sent_with_the_key_that_types_them_in_the_active_layout() {
        assert_eq!(super::code_in_layout("a", french_azerty).unwrap(), 12);
        assert_eq!(super::code_in_layout("q", french_azerty).unwrap(), 0);
        assert_eq!(super::code_in_layout("z", french_azerty).unwrap(), 13);
        assert_eq!(super::code_in_layout("w", french_azerty).unwrap(), 6);
        assert_eq!(super::code_in_layout("m", french_azerty).unwrap(), 41);
    }

    #[test]
    fn letters_keep_their_ansi_key_when_the_layout_cannot_place_them() {
        let unreadable = |_| LayoutLookup::LayoutUnavailable;
        let no_latin_keys = |_| LayoutLookup::NoKeyForSymbol;
        for letter in ["a", "q", "z", "w"] {
            let ansi = key_name_to_code(letter).unwrap();
            assert_eq!(super::code_in_layout(letter, unreadable).unwrap(), ansi);
            assert_eq!(super::code_in_layout(letter, no_latin_keys).unwrap(), ansi);
        }
    }

    #[test]
    fn named_keys_never_consult_the_layout() {
        let layout_must_not_be_read = |symbol| panic!("layout read for {symbol}");
        assert_eq!(
            super::code_in_layout("return", layout_must_not_be_read).unwrap(),
            36
        );
        assert_eq!(
            super::code_in_layout("delete", layout_must_not_be_read).unwrap(),
            51
        );
    }

    #[test]
    fn off_the_main_thread_a_command_letter_keeps_its_ansi_key() {
        let combo = KeyCombo {
            key: "a".into(),
            modifiers: vec![Modifier::Meta],
        };
        let code = std::thread::spawn(move || super::key_code(&combo))
            .join()
            .unwrap();
        assert_eq!(code.unwrap(), 0);
    }
}
