use super::{macos_major, parse_major};

#[test]
fn the_major_version_is_read_from_the_product_version() {
    assert_eq!(parse_major("27.0.1\0"), Some(27));
    assert_eq!(parse_major("15.6"), Some(15));
    assert_eq!(parse_major("garbage"), None);
    assert!(macos_major().is_some_and(|major| major >= 11));
}
