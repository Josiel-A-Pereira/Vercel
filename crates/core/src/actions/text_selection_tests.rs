use super::*;

#[test]
fn resolves_overlapping_matches_and_unicode_offsets() {
    let selection = TextSelection::Match {
        text: "ana".into(),
        nth: Some(2),
    };
    let resolved = selection.resolve("😀 banana café").unwrap();
    assert_eq!(resolved.range, 5..8);
    assert_eq!(resolved.utf16_range, 6..9);
    assert_eq!(resolved.text, "ana");

    let range = TextSelection::Range { start: 0, end: 2 }
        .resolve("😀e\u{301}")
        .unwrap();
    assert_eq!(range.range, 0..2);
    assert_eq!(range.utf16_range, 0..3);
    assert_eq!(range.text, "😀e");

    let caret = TextSelection::Range { start: 3, end: 3 }
        .resolve("😀e\u{301}")
        .unwrap();
    assert_eq!(caret.utf16_range, 4..4);
    assert_eq!(caret.text, "");
}

#[test]
fn rejects_ambiguous_missing_invalid_and_oversize_selections() {
    let matching = |nth| TextSelection::Match {
        text: "aa".into(),
        nth,
    };
    assert_eq!(
        matching(None).resolve("aaa").unwrap_err().code,
        ErrorCode::AmbiguousTarget
    );
    assert_eq!(
        matching(Some(3)).resolve("aaa").unwrap_err().code,
        ErrorCode::ElementNotFound
    );
    assert_eq!(
        matching(Some(0)).resolve("aaa").unwrap_err().code,
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        TextSelection::Match {
            text: "".into(),
            nth: None
        }
        .resolve("a")
        .unwrap_err()
        .code,
        ErrorCode::InvalidArgs
    );
    for range in [
        TextSelection::Range { start: 2, end: 1 },
        TextSelection::Range { start: 0, end: 4 },
    ] {
        assert_eq!(
            range.resolve("abc").unwrap_err().code,
            ErrorCode::InvalidArgs
        );
    }
    assert!(
        TextSelection::Range { start: 0, end: 1 }
            .resolve(&"a".repeat(65_536))
            .is_ok()
    );
    assert_eq!(
        TextSelection::Range { start: 0, end: 1 }
            .resolve(&"a".repeat(65_537))
            .unwrap_err()
            .code,
        ErrorCode::ActionNotSupported
    );
}

#[test]
fn selection_json_rejects_unknown_fields() {
    let value = serde_json::json!({"kind":"match","text":"a","nth":2});
    let selection: TextSelection = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(selection).unwrap(), value);
    assert!(
        serde_json::from_value::<TextSelection>(
            serde_json::json!({"kind":"range","start":0,"end":1,"extra":true})
        )
        .is_err()
    );
}
