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

#[test]
fn one_bad_client_entry_is_skipped_and_the_rest_are_kept() {
    let found = parse(
        br#"[{"address":"0x1","pid":-1},{"pid":5},{"address":"0x2","pid":9},{"address":"0x3","pid":"x"},{"address":"0x4","pid":0}]"#,
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found.get(&2), Some(&9));
}

#[test]
fn a_signature_that_could_leave_the_runtime_directory_is_refused() {
    let runtime = OsStr::new("/run/user/1000");
    for bad in ["", ".", "..", "../x", "a/b", "/etc"] {
        assert!(socket_path(runtime, OsStr::new(bad)).is_none(), "{bad:?}");
    }
    let good = socket_path(runtime, OsStr::new("abc_123")).unwrap();
    assert_eq!(
        good,
        PathBuf::from("/run/user/1000/hypr/abc_123/.socket.sock")
    );
}

#[test]
fn a_connect_to_a_missing_socket_or_past_the_deadline_gives_nothing() {
    let missing = PathBuf::from("/nonexistent-agent-desktop/hypr/x/.socket.sock");
    assert!(connect_by(missing.clone(), Deadline::detached_after(1000).unwrap()).is_none());
    let expired = Deadline::detached_after(1).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(connect_by(missing, expired).is_none());
}
