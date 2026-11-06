use super::*;
use crate::input::wayland::virtual_keyboard::extras::FIRST_EXTRA_XKB;

#[test]
fn shift_level_symbols_are_found_on_their_base_key() {
    assert_eq!(
        locate_base(0x61),
        Some(Located {
            evdev: 30,
            shift: false
        })
    );
    assert_eq!(
        locate_base(0x41),
        Some(Located {
            evdev: 30,
            shift: true
        })
    );
    assert_eq!(
        locate_base(u32::from(b'?')),
        Some(Located {
            evdev: 53,
            shift: true
        })
    );
}

#[test]
fn thai_and_latin_one_are_not_on_the_base_layout() {
    assert_eq!(locate_base(0x0100_0e01), None);
    assert_eq!(locate_base(0xe9), None);
}

#[test]
fn every_base_keysym_resolves_to_its_own_key_and_stays_below_the_extras() {
    for key in BASE_KEYS {
        assert_eq!(
            locate_base(key.plain).map(|found| found.evdev),
            Some(key.evdev)
        );
        if let Some(shifted) = key.shifted {
            let found = locate_base(shifted).expect("the shifted symbol is on the base layout");
            assert_eq!((found.evdev, found.shift), (key.evdev, true));
        }
        assert!(
            xkb_of(key.evdev) < FIRST_EXTRA_XKB,
            "evdev {} collides with the extras",
            key.evdev
        );
    }
}

#[test]
fn base_evdev_codes_are_unique() {
    let mut seen: Vec<u32> = BASE_KEYS.iter().map(|key| key.evdev).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), BASE_KEYS.len());
}

#[test]
fn extra_keysyms_are_found_on_their_slot() {
    let mut extras = Extras::default();
    extras.replace(vec![0x0100_0e01, 0xe9]);
    assert_eq!(
        locate(0xe9, &extras),
        Some(Located {
            evdev: FIRST_EXTRA_XKB + 1 - 8,
            shift: false
        })
    );
    assert_eq!(locate(0x0100_0e02, &extras), None);
    assert_eq!(locate(0x62, &extras).map(|found| found.evdev), Some(48));
}
