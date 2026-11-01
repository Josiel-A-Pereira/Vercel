use std::io::Cursor;

use super::*;

#[test]
fn a_dash_reads_the_batch_from_stdin() {
    let json = r#"[{"command":"version"}]"#;
    assert_eq!(resolve("-", Cursor::new(json)).unwrap(), json);
}

#[test]
fn any_other_argument_is_the_batch_and_stdin_is_left_unread() {
    let mut stdin = Cursor::new("unread");
    let json = r#"[{"command":"version"}]"#;
    assert_eq!(resolve(json, &mut stdin).unwrap(), json);
    assert_eq!(stdin.position(), 0);
}

#[test]
fn stdin_reading_stops_one_byte_past_the_input_limit() {
    let oversized = " ".repeat(MAX_BATCH_JSON_BYTES + 100);
    let read = resolve("-", Cursor::new(oversized)).unwrap();
    assert_eq!(read.len(), MAX_BATCH_JSON_BYTES + 1);
}

#[test]
fn stdin_that_is_not_utf8_is_refused_before_parsing() {
    let error = resolve("-", Cursor::new(vec![0xff, 0xfe])).unwrap_err();
    assert!(error.to_string().contains("must be UTF-8"));
}
