//! The `xkb_keymap` text the virtual keyboard uploads: the US base layout at
//! its real keycodes plus one single-level key per extra keysym.

use std::fmt::Write;

use super::base_key::BASE_KEYS;
use super::extras::{FIRST_EXTRA_XKB, LAST_XKB};
use super::locate::xkb_of;
use super::modifier_mask::MODIFIER_KEYS;

/// First keycode the keymap declares; xkb reserves everything below it.
pub(super) const MINIMUM_XKB: u32 = 8;

fn key_name(xkb: u32) -> String {
    format!("<K{xkb:03}>")
}

fn symbols(out: &mut String, extras: &[u32]) {
    for key in BASE_KEYS {
        let name = key_name(xkb_of(key.evdev));
        let _ = match key.shifted {
            Some(shifted) => writeln!(
                out,
                "    key {name} {{ [ 0x{:x}, 0x{shifted:x} ] }};",
                key.plain
            ),
            None => writeln!(out, "    key {name} {{ [ 0x{:x} ] }};", key.plain),
        };
    }
    for (at, keysym) in extras.iter().enumerate() {
        let xkb = FIRST_EXTRA_XKB + u32::try_from(at).unwrap_or(0);
        let _ = writeln!(out, "    key {} {{ [ 0x{keysym:x} ] }};", key_name(xkb));
    }
    for (keysym, _, modifier) in MODIFIER_KEYS {
        if let Some(key) = BASE_KEYS.iter().find(|key| key.plain == keysym) {
            let _ = writeln!(
                out,
                "    modifier_map {modifier} {{ {} }};",
                key_name(xkb_of(key.evdev))
            );
        }
    }
}

fn keycodes(out: &mut String, extras: &[u32]) {
    let _ = writeln!(out, "  minimum = {MINIMUM_XKB};\n  maximum = {LAST_XKB};");
    for key in BASE_KEYS {
        let xkb = xkb_of(key.evdev);
        let _ = writeln!(out, "  {} = {xkb};", key_name(xkb));
    }
    for at in 0..extras.len() {
        let xkb = FIRST_EXTRA_XKB + u32::try_from(at).unwrap_or(0);
        let _ = writeln!(out, "  {} = {xkb};", key_name(xkb));
    }
}

/// The keymap text for the given extra keysyms, assigned to xkb keycodes from
/// [`FIRST_EXTRA_XKB`] in order. It carries no trailing NUL; the upload adds
/// it. Every keysym is printed in hex so that nothing depends on the
/// compositor's keysym name table.
pub(super) fn keymap_text(extras: &[u32]) -> String {
    let mut out = String::from("xkb_keymap {\nxkb_keycodes \"agent\" {\n");
    keycodes(&mut out, extras);
    out.push_str("};\nxkb_types \"agent\" { include \"complete\" };\n");
    out.push_str("xkb_compatibility \"agent\" { include \"complete\" };\n");
    out.push_str("xkb_symbols \"agent\" {\n");
    symbols(&mut out, extras);
    out.push_str("};\n};\n");
    out
}

#[cfg(test)]
#[path = "keymap_text_tests.rs"]
mod tests;
