use super::FocusEffect;
use super::select_text_endpoints::resolve_endpoint;
use super::select_text_focus::{focus_changed_before_write, with_focus_evidence};
use super::select_text_target::{selection_endpoints, selection_unavailable};
use super::select_text_verify::exact_since_after;
use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};
use std::time::{Duration, Instant};

#[test]
fn either_focus_or_activation_is_a_side_effect() {
    assert!(
        !FocusEffect {
            focus_changed: false,
            activated: false,
        }
        .changed()
    );
    assert!(
        FocusEffect {
            focus_changed: true,
            activated: false,
        }
        .changed()
    );
    assert!(
        FocusEffect {
            focus_changed: false,
            activated: true,
        }
        .changed()
    );
}

fn scalar_prefix(
    text: &'static str,
) -> impl FnMut(usize) -> Result<String, agent_desktop_core::AdapterError> {
    move |count| Ok(text.chars().take(count).collect())
}

fn utf16_prefix(
    text: &'static str,
) -> impl FnMut(usize) -> Result<String, agent_desktop_core::AdapterError> {
    move |count| {
        let units: Vec<u16> = text.encode_utf16().take(count).collect();
        Ok(String::from_utf16_lossy(&units))
    }
}

fn byte_prefix(
    text: &'static str,
) -> impl FnMut(usize) -> Result<String, agent_desktop_core::AdapterError> {
    move |count| Ok(String::from_utf8_lossy(&text.as_bytes()[..count.min(text.len())]).into_owned())
}

fn grapheme_prefix(
    text: &'static str,
) -> impl FnMut(usize) -> Result<String, agent_desktop_core::AdapterError> {
    move |count| {
        Ok(match count {
            0 => String::new(),
            1 => text.chars().take(2).collect(),
            _ => text.to_owned(),
        })
    }
}

#[test]
fn scalar_counting_provider_finds_scalar_offset() {
    let count = resolve_endpoint("a😀", scalar_prefix("a😀b")).expect("read");
    assert_eq!(count, Some(2));
}

#[test]
fn utf16_counting_provider_finds_code_unit_offset() {
    let count = resolve_endpoint("a😀", utf16_prefix("a😀b")).expect("read");
    assert_eq!(count, Some(3));
}

#[test]
fn byte_counting_provider_finds_utf8_offset() {
    let count = resolve_endpoint("a😀", byte_prefix("a😀b")).expect("read");
    assert_eq!(count, Some(5));
}

#[test]
fn grapheme_counting_provider_falls_back_to_measured_units() {
    let count = resolve_endpoint("e\u{301}", grapheme_prefix("e\u{301}x")).expect("read");
    assert_eq!(count, Some(1));
}

#[test]
fn empty_prefix_is_zero_without_reading_the_provider() {
    let reads = std::cell::Cell::new(0);
    let count = resolve_endpoint("", |_| {
        reads.set(reads.get() + 1);
        Ok(String::new())
    })
    .expect("read");
    assert_eq!(count, Some(0));
    assert_eq!(reads.get(), 0);
}

#[derive(Debug, PartialEq, Eq)]
struct RangeProbe(u8);

#[test]
fn nonzero_start_uses_the_prefix_end_endpoint() {
    let start = RangeProbe(1);
    let end = RangeProbe(2);
    let [
        (start_target, start_source, start_source_endpoint),
        (end_target, end_source, end_source_endpoint),
    ] = selection_endpoints(&start, &end);

    assert_eq!(
        start_target,
        uiautomation::types::TextPatternRangeEndpoint::Start
    );
    assert_eq!(start_source, &start);
    assert_eq!(
        start_source_endpoint,
        uiautomation::types::TextPatternRangeEndpoint::End
    );
    assert_eq!(
        end_target,
        uiautomation::types::TextPatternRangeEndpoint::End
    );
    assert_eq!(end_source, &end);
    assert_eq!(
        end_source_endpoint,
        uiautomation::types::TextPatternRangeEndpoint::End
    );
}

#[test]
fn unsupported_selection_is_not_delivered() {
    let error = selection_unavailable();

    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}

#[test]
fn unreadable_post_write_focus_reports_unknown_evidence() {
    let error = with_focus_evidence(
        AdapterError::new(ErrorCode::ActionFailed, "focus unavailable")
            .with_disposition(DeliverySemantics::delivered_unverified()),
        None,
    );
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    let details = error.details.expect("focus evidence details");

    assert_eq!(details["focus_changed"], serde_json::Value::Null);
    assert_eq!(details["activated"], serde_json::Value::Null);
    assert_eq!(details["focus_evidence"], "unknown");
}

#[test]
fn observed_post_write_focus_reports_current_evidence() {
    let error = with_focus_evidence(
        AdapterError::new(ErrorCode::ActionFailed, "selection failed")
            .with_disposition(DeliverySemantics::delivered_unverified()),
        Some(FocusEffect {
            focus_changed: true,
            activated: false,
        }),
    );
    let details = error.details.expect("focus evidence details");

    assert_eq!(details["focus_changed"], true);
    assert_eq!(details["activated"], false);
    assert_eq!(details["focus_evidence"], "observed");
}

#[test]
fn focus_change_before_the_setter_is_not_delivered() {
    let error = focus_changed_before_write(FocusEffect {
        focus_changed: true,
        activated: false,
    });

    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    let details = error.details.expect("focus change details");
    assert_eq!(details["focus_changed"], true);
    assert_eq!(details["activated"], false);
}

#[test]
fn first_exact_is_unbounded_until_readback_arrives() {
    let started = Instant::now();

    let pending = exact_since_after(None, started + Duration::from_secs(5), false, 0)
        .expect("a delayed readback remains pending");
    assert_eq!(pending, None);

    let exact = exact_since_after(pending, started + Duration::from_secs(6), true, 1)
        .expect("the first exact readback starts the stability window");
    assert_eq!(exact, Some(started + Duration::from_secs(6)));
}

#[test]
fn stability_window_boundary_and_lost_mismatch_are_explicit() {
    let started = Instant::now();
    let exact = exact_since_after(None, started, true, 1).expect("first exact readback");

    assert!(
        (started + Duration::from_millis(99)).duration_since(exact.expect("exact timestamp"))
            < Duration::from_millis(100)
    );
    assert!(
        (started + Duration::from_millis(100)).duration_since(exact.expect("exact timestamp"))
            >= Duration::from_millis(100)
    );

    let error = exact_since_after(exact, started + Duration::from_millis(20), false, 2)
        .expect_err("a mismatch after exactness is loss");
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert_eq!(error.details.expect("details")["selection_count"], 2);
}
