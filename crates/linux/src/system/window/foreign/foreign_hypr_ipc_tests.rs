use super::*;
use std::io::{Cursor, Error, ErrorKind};

#[test]
fn reads_fragmented_json_until_eof_and_matches_numeric_addresses() {
    let mut source = Cursor::new(br#"[{"address":"0x000ab","pid":7},{"address":"0xF0","pid":8}]"#);
    let mut reads = 0;
    let found = read_reply(
        Deadline::detached_after(1000).unwrap(),
        |buffer, remaining| {
            assert!(!remaining.is_zero());
            reads += 1;
            source.read(&mut buffer[..3])
        },
    )
    .unwrap();
    assert!(reads > 2);
    assert_eq!(found.get(&0xab), Some(&7));
    assert_eq!(found.get(&0xf0), Some(&8));
}

#[test]
fn timed_out_reply_and_duplicate_addresses_are_rejected() {
    assert!(parse(br#"[{"address":"0x1","pid":1},{"address":"0x1","pid":2}]"#).is_none());
    let result = read_reply(Deadline::detached_after(30).unwrap(), |_, remaining| {
        assert!(!remaining.is_zero());
        Err(Error::new(ErrorKind::TimedOut, "silent"))
    });
    assert!(result.is_none());
}
