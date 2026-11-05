//! Which keysyms the virtual keyboard can type, and the refusal for the rest.

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};
use serde_json::json;

use crate::input::keyboard::keysym_step::KeysymStep;

const UNICODE_KEYSYM_BASE: u32 = 0x0100_0000;
const LAST_UNICODE_KEYSYM: u32 = UNICODE_KEYSYM_BASE + 0x10_ffff;
const LAST_KEYSYM: u32 = 0x1fff_ffff;
const C0_CONTROLS: std::ops::RangeInclusive<u32> = 0x00..=0x1f;
const C1_CONTROLS: std::ops::RangeInclusive<u32> = 0x7f..=0x9f;

fn is_control(code_point: u32) -> bool {
    C0_CONTROLS.contains(&code_point) || C1_CONTROLS.contains(&code_point)
}

/// Whether `keysym` can be typed: every keysym that is not a control
/// character. Anything outside the base layout gets an extra key, so Thai,
/// Latin-1 and emoji all qualify; `\n` and `\t` already map to Return and Tab.
pub(crate) fn typable(keysym: u32) -> bool {
    if keysym == 0 || keysym > LAST_KEYSYM {
        return false;
    }
    if (UNICODE_KEYSYM_BASE..=LAST_UNICODE_KEYSYM).contains(&keysym) {
        return !is_control(keysym - UNICODE_KEYSYM_BASE);
    }
    if keysym > LAST_UNICODE_KEYSYM && keysym < 0x1000_0000 {
        return false;
    }
    !is_control(keysym)
}

/// Refuses the whole delivery, before any key, when any step is not typable.
pub(crate) fn refuse_untypable(chunks: &[Vec<KeysymStep>]) -> Result<(), AdapterError> {
    if chunks.iter().flatten().all(|key| typable(key.keysym)) {
        return Ok(());
    }
    Err(AdapterError::new(
        ErrorCode::ActionNotSupported,
        "A character in the text is outside the layout of the Wayland session keyboard",
    )
    .with_suggestion("Use the semantic set-value on the element instead of typing the text")
    .with_details(json!({
        "physical_delivery_started": false,
        "cause": "character_outside_layout",
    }))
    .with_disposition(DeliverySemantics::not_delivered()))
}

#[cfg(test)]
#[path = "typable_tests.rs"]
mod tests;
