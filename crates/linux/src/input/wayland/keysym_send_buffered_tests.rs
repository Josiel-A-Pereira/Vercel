use super::*;
use crate::input::wayland::fake_buffering_session::BufferingSession;
use agent_desktop_core::ErrorCode;

const CONTROL_L: u32 = 0xffe3;
const KEY_A: u32 = 0x61;
const KEY_B: u32 = 0x62;

fn live() -> Deadline {
    Deadline::after(5_000).expect("a five second deadline builds")
}

fn edge(keysym: u32, down: bool) -> KeysymStep {
    KeysymStep { keysym, down }
}

fn tap(keysym: u32) -> Vec<KeysymStep> {
    vec![edge(keysym, true), edge(keysym, false)]
}

fn refuse(message: &'static str) -> AdapterError {
    AdapterError::new(ErrorCode::ActionFailed, message)
        .with_details(json!({ "cause": "not_active", "physical_delivery_started": false }))
        .with_disposition(DeliverySemantics::not_delivered())
}

#[test]
fn a_session_that_needs_no_lead_sends_no_shift_and_prepares_each_chunk_after_its_gate() {
    let first = tap(KEY_A);
    let second = tap(KEY_B);
    let mut session = BufferingSession::new();
    let log = session.log();
    let mut gate = || {
        log.borrow_mut().push("gate".into());
        Ok(())
    };
    let delivery = run_chunks(&mut session, &[&first, &second], &mut gate, live()).expect("sent");
    assert_eq!(delivery.delivered_events, 4);
    assert_eq!(
        session.events(),
        [
            "create",
            "start",
            "gate",
            "gate",
            "prepare:2",
            "gate",
            "down:61",
            "up:61",
            "gate",
            "prepare:2",
            "gate",
            "down:62",
            "up:62",
            "stop"
        ]
    );
    assert!(
        session
            .written
            .iter()
            .all(|(keysym, _)| *keysym != KEYSYM_SHIFT_L)
    );
}

#[test]
fn a_refused_gate_mid_chord_writes_nothing_and_stays_not_delivered() {
    let keys = vec![
        edge(CONTROL_L, true),
        edge(KEY_A, true),
        edge(KEY_A, false),
        edge(CONTROL_L, false),
    ];
    let mut session = BufferingSession::new();
    let log = session.log();
    let mut gate = || {
        if log.borrow().iter().any(|event| event == "down:ffe3") {
            Err(refuse(
                "The target window is not active, so no key was sent",
            ))
        } else {
            Ok(())
        }
    };
    let error = run_chunks(&mut session, &[&keys], &mut gate, live()).unwrap_err();
    assert!(session.written.is_empty());
    assert!(!session.events().iter().any(|event| event == "up:ffe3"));
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(error.message.contains("no key was sent"));
    let details = error.details.expect("details");
    assert_eq!(details["physical_delivery_started"], false);
    assert!(details.get("delivered_events").is_none());
}

#[test]
fn a_refusal_after_written_groups_counts_only_what_was_written() {
    let keys = [
        tap(KEY_A),
        vec![edge(CONTROL_L, true), edge(KEY_B, true)],
        vec![edge(KEY_B, false), edge(CONTROL_L, false)],
    ]
    .concat();
    let mut session = BufferingSession::new();
    let log = session.log();
    let mut gate = || {
        if log.borrow().iter().any(|event| event == "down:ffe3") {
            Err(refuse(
                "The target window is not active, so no key was sent",
            ))
        } else {
            Ok(())
        }
    };
    let error = run_chunks(&mut session, &[&keys], &mut gate, live()).unwrap_err();
    assert_eq!(session.written, [(KEY_A, true), (KEY_A, false)]);
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    let details = error.details.expect("details");
    assert_eq!(details["physical_delivery_started"], true);
    assert_eq!(details["delivered_events"], 2);
}

#[test]
fn the_route_probe_error_is_not_delivered_and_a_missing_manager_takes_the_mutter_path() {
    let keys = vec![tap(KEY_A)];
    let error = send_via(&keys, &mut || Ok(()), live(), |deadline| {
        Err::<Option<VkLink>, _>(deadline.timeout_error())
    })
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(
        error.details.expect("details")["physical_delivery_started"],
        false
    );

    let outside = vec![tap(0x0100_4f60)];
    let error = send_via(&outside, &mut || Ok(()), live(), |_| Ok(None::<VkLink>))
        .expect_err("the Mutter layout refuses it");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}
