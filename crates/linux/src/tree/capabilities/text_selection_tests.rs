use std::ops::Range;

use agent_desktop_core::{AdapterError, ErrorCode};

use super::{
    MAX_TEXT_BYTES, MAX_TEXT_CHARACTERS, TEXT_CHUNK_CHARACTERS, TEXT_INTERFACE, TextSource,
    checked_state, selection_utf16, selection_when_readable,
};

const MIXED: &str = "a\u{1F600}e\u{301}b";

#[derive(Default)]
struct Fake {
    caret: Option<usize>,
    selections: Option<Vec<Range<usize>>>,
    count: Option<usize>,
    text: Option<String>,
    text_reads: usize,
    gone: bool,
}

impl Fake {
    fn over(text: &str, caret: usize) -> Self {
        Self {
            caret: Some(caret),
            selections: Some(Vec::new()),
            count: Some(text.chars().count()),
            text: Some(text.to_owned()),
            ..Self::default()
        }
    }
}

impl TextSource for Fake {
    fn caret(&mut self) -> Result<Option<usize>, AdapterError> {
        if self.gone {
            return Err(AdapterError::new(ErrorCode::AppNotFound, "gone"));
        }
        Ok(self.caret)
    }

    fn selections(&mut self) -> Result<Option<Vec<Range<usize>>>, AdapterError> {
        Ok(self.selections.clone())
    }

    fn character_count(&mut self) -> Result<Option<usize>, AdapterError> {
        Ok(self.count)
    }

    fn text(&mut self, start: usize, end: usize) -> Result<Option<String>, AdapterError> {
        self.text_reads += 1;
        Ok(self.text.as_ref().map(|text| {
            text.chars()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect()
        }))
    }
}

fn interfaces(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn utf16_selection_and_refusal_boundaries() {
    let mut fake = Fake::over(MIXED, 3);
    assert_eq!(selection_utf16(&mut fake).expect("answers"), Some(4..4));
    fake.selections = Some(vec![Range { start: 2, end: 1 }]);
    assert_eq!(selection_utf16(&mut fake).expect("answers"), Some(1..3));

    fake.selections = Some(Vec::new());
    fake.caret = None;
    assert_eq!(selection_utf16(&mut fake).expect("answers"), None);
    fake.caret = Some(3);
    fake.selections = None;
    assert_eq!(selection_utf16(&mut fake).expect("answers"), None);

    fake.selections = Some(Vec::new());
    fake.count = Some(MAX_TEXT_CHARACTERS + 1);
    fake.text_reads = 0;
    assert_eq!(selection_utf16(&mut fake).expect("answers"), None);
    assert_eq!(fake.text_reads, 0);
    fake.count = Some(MAX_TEXT_CHARACTERS);
    assert_eq!(selection_utf16(&mut fake).expect("answers"), Some(4..4));
    fake.caret = Some(9);
    assert_eq!(selection_utf16(&mut fake).expect("answers"), None);

    fake.gone = true;
    let error = selection_utf16(&mut fake).expect_err("a gone node is refused");
    assert_eq!(error.code, ErrorCode::AppNotFound);
    assert_eq!(error.message, "gone");

    let interfaces = interfaces(&[TEXT_INTERFACE]);
    let mut fake = Fake::over("hunter2", 7);
    assert_eq!(
        selection_when_readable(true, Some(&interfaces), &mut fake).expect("secure refusal"),
        None
    );
    assert_eq!(
        selection_when_readable(false, None, &mut fake).expect("unknown interface refusal"),
        None
    );
    assert_eq!(
        selection_when_readable(false, Some(&["org.a11y.atspi.Value".to_owned()]), &mut fake,)
            .expect("non-text interface refusal"),
        None
    );
    assert_eq!(fake.text_reads, 0);
    assert_eq!(
        selection_when_readable(false, Some(&interfaces), &mut fake).expect("answers"),
        Some(7..7)
    );
    assert!(fake.text_reads > 0);
}

#[test]
fn complete_text_reads_bounded_chunks_and_preserves_all_selections() {
    let document = format!("{}😀", "a".repeat(TEXT_CHUNK_CHARACTERS + 3));
    let mut fake = Fake::over(&document, 2);
    fake.selections = Some(vec![4..8, 10..12]);
    let state = checked_state(&mut fake)
        .expect("the provider answers")
        .expect("the complete document is available");
    assert_eq!(state.text, document);
    assert_eq!(state.selections, [4..8, 10..12]);
    assert_eq!(fake.text_reads, 2);
}

#[test]
fn complete_text_rejects_utf8_bytes_over_the_limit() {
    let mut fake = Fake::over(&"é".repeat(MAX_TEXT_BYTES / 2 + 1), 0);
    let error = checked_state(&mut fake).expect_err("oversize text is refused");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(
        error.disposition,
        agent_desktop_core::DeliverySemantics::not_delivered()
    );
    assert!(fake.text_reads < MAX_TEXT_BYTES / TEXT_CHUNK_CHARACTERS);
}

#[test]
fn incomplete_provider_chunks_and_out_of_bounds_selections_are_not_complete() {
    let mut fake = Fake::over("short", 2);
    fake.count = Some(6);
    assert!(checked_state(&mut fake).expect("read answers").is_none());
    fake.count = Some(5);
    fake.selections = Some(std::iter::once(4..6).collect());
    assert!(checked_state(&mut fake).expect("read answers").is_none());
}
