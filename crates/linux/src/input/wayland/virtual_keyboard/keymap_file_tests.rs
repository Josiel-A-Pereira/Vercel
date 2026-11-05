use super::*;
use std::io::{Read, Seek, SeekFrom};

#[test]
fn the_upload_adds_exactly_one_trailing_nul() {
    let bytes = with_nul("xkb_keymap {};");
    assert_eq!(bytes.last(), Some(&0));
    assert_eq!(bytes.len(), "xkb_keymap {};".len() + 1);
    assert!(!bytes[..bytes.len() - 1].contains(&0));
}

#[test]
fn the_sealed_file_holds_the_text_and_its_size_counts_the_nul() {
    let (descriptor, size) = sealed_keymap("xkb_keymap {};").expect("the keymap file builds");
    assert_eq!(size as usize, "xkb_keymap {};".len() + 1);
    let mut file = File::from(descriptor);
    let seals = rustix::fs::fcntl_get_seals(&file).expect("seals read back");
    assert!(seals.contains(SealFlags::WRITE | SealFlags::SHRINK | SealFlags::GROW));
    file.seek(SeekFrom::Start(0)).expect("rewinds");
    let mut content = Vec::new();
    file.read_to_end(&mut content).expect("reads");
    assert_eq!(content, with_nul("xkb_keymap {};"));
    assert!(
        file.write_all(b"x").is_err(),
        "a sealed file refuses writes"
    );
}
