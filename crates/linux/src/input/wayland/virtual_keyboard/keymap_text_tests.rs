use super::*;
use std::collections::HashSet;
use std::io::Write as _;
use std::process::{Command, Stdio};

fn declarations(text: &str) -> Vec<(String, u32)> {
    text.lines()
        .filter_map(|line| {
            let (name, number) = line.trim().split_once(" = ")?;
            let number = number.strip_suffix(';')?.parse().ok()?;
            name.starts_with("<K").then(|| (name.to_owned(), number))
        })
        .collect()
}

fn thai_and_latin() -> Vec<u32> {
    vec![0x0100_0e01, 0xe9, 0x0100_0000 + 0x1f600]
}

#[test]
fn every_keycode_is_within_the_declared_range() {
    let text = keymap_text(&thai_and_latin());
    assert!(text.contains(&format!("minimum = {MINIMUM_XKB};")));
    assert!(text.contains(&format!("maximum = {LAST_XKB};")));
    let declared = declarations(&text);
    assert!(declared.len() > BASE_KEYS.len());
    for (name, number) in declared {
        assert!(
            (MINIMUM_XKB..=LAST_XKB).contains(&number),
            "{name} = {number}"
        );
    }
}

#[test]
fn key_names_and_keycodes_are_unique() {
    let declared = declarations(&keymap_text(&thai_and_latin()));
    let names: HashSet<&String> = declared.iter().map(|(name, _)| name).collect();
    let numbers: HashSet<u32> = declared.iter().map(|(_, number)| *number).collect();
    assert_eq!(names.len(), declared.len());
    assert_eq!(numbers.len(), declared.len());
}

#[test]
fn every_keysym_is_printed_in_hex() {
    let text = keymap_text(&thai_and_latin());
    for line in text
        .lines()
        .filter(|line| line.trim_start().starts_with("key <"))
    {
        let inside = line
            .split('[')
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("a symbol list");
        for symbol in inside.split(',') {
            assert!(symbol.trim().starts_with("0x"), "{line}");
        }
    }
    assert!(text.contains("0x1000e01"));
    assert!(text.contains("0xe9"));
    assert!(text.contains("0x101f600"));
}

#[test]
fn extras_sit_at_consecutive_keycodes_from_the_first_extra() {
    let text = keymap_text(&thai_and_latin());
    assert!(text.contains("key <K136> { [ 0x1000e01 ] };"));
    assert!(text.contains("key <K137> { [ 0xe9 ] };"));
    assert!(text.contains("key <K138> { [ 0x101f600 ] };"));
    assert!(!text.contains("<K139>"));
}

#[test]
fn the_base_layout_sits_at_real_evdev_keycodes() {
    let text = keymap_text(&[]);
    assert!(text.contains("key <K038> { [ 0x61, 0x41 ] };"));
    assert!(text.contains("key <K050> { [ 0xffe1 ] };"));
    assert!(text.contains("modifier_map Shift { <K050> };"));
    assert!(text.contains("modifier_map Control { <K037> };"));
    assert!(text.contains("modifier_map Mod1 { <K064> };"));
    assert!(text.contains("modifier_map Mod4 { <K133> };"));
}

#[test]
fn the_text_carries_no_nul() {
    assert!(!keymap_text(&thai_and_latin()).contains('\0'));
}

fn compiles(text: &str) -> Option<Result<(), String>> {
    let spawned = Command::new("xkbcli")
        .args(["compile-keymap", "--keymap"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => return Some(Err(error.to_string())),
    };
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin
        .write_all(text.as_bytes())
        .expect("the keymap is written");
    drop(stdin);
    let output = child.wait_with_output().expect("xkbcli finishes");
    Some(if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    })
}

#[test]
fn xkbcli_compiles_the_base_and_an_extended_keymap() {
    for extras in [Vec::new(), thai_and_latin()] {
        match compiles(&keymap_text(&extras)) {
            None => println!("xkbcli is not on PATH: keymap compile check skipped"),
            Some(outcome) => outcome.expect("xkbcli compiles the keymap text"),
        }
    }
}

#[test]
fn xkbcli_compiles_a_full_set_of_extras() {
    let extras: Vec<u32> = (0..120).map(|at| 0x0100_0e01 + at).collect();
    match compiles(&keymap_text(&extras)) {
        None => println!("xkbcli is not on PATH: keymap compile check skipped"),
        Some(outcome) => outcome.expect("xkbcli compiles 120 extras"),
    }
}
