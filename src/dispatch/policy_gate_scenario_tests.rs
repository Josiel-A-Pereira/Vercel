//! Real-case scenarios for the Dogwood policy gate, run through the real
//! `dispatch` path with seeded refs and a counting adapter: the agent the
//! policies exist for is one that holds live refs and tries to act on them.
//!
//! Policies are passed on the context (`--policy` style), never through the
//! default folder, so concurrently running tests that carry no policy paths
//! cannot discover them. Each scenario holds a [`HomeGuard`], which both
//! isolates the ref store and the engine's durable store under a fresh
//! state root and serializes these tests with every other state-touching
//! test in the process.

use std::sync::atomic::Ordering;

use super::policy_gate_desk::{Scenario, error_code};

#[test]
fn password_manager_refs_are_untouchable_even_with_a_live_ref() {
    let scenario = Scenario::new(
        r#"@id("allow-everything")
permit (principal, action, resource);

@id("hands-off-the-password-manager")
forbid (principal, action in Desktop::Action::"input",
        resource == Desktop::App::"1Password");
"#,
    );
    let vault_ref = scenario.seed_ref("1Password", "1Password - Vault", "button", "Unlock", false);
    let editor_ref = scenario.seed_ref("TextEdit", "Untitled", "textfield", "Body", false);

    let denied = scenario.run(&["click", &vault_ref]);
    assert_eq!(error_code(&denied), "POLICY_DENIED");
    assert_eq!(
        scenario.resolve_calls(),
        0,
        "a denied click must never reach the platform adapter"
    );
    let message = denied.unwrap_err().to_string();
    assert!(
        message.contains("hands-off-the-password-manager"),
        "denial names the deciding policy: {message}"
    );

    let allowed = scenario.run(&["click", &editor_ref]);
    assert_eq!(
        error_code(&allowed),
        "STALE_REF",
        "the TextEdit click passes the gate and stops at the scenario desk"
    );
    assert!(
        scenario.resolve_calls() > 0,
        "the allowed click reaches the adapter"
    );
}

#[test]
fn secure_fields_reject_typing_in_any_app() {
    let scenario = Scenario::new(
        r#"@id("allow-everything")
permit (principal, action, resource);

@id("never-type-into-secure-fields")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.secure == true };
"#,
    );
    let password_ref = scenario.seed_ref("Safari", "Login", "textfield", "Password", true);
    let search_ref = scenario.seed_ref("Safari", "Login", "textfield", "Search", false);

    let denied = scenario.run(&["type", &password_ref, "hunter2"]);
    assert_eq!(
        error_code(&denied),
        "POLICY_DENIED",
        "both fields carry the canonical role textfield, as a real AXSecureTextField \
         snapshot does; only the secure state tells them apart"
    );
    assert_eq!(scenario.resolve_calls(), 0);

    let allowed = scenario.run(&["type", &search_ref, "weather tomorrow"]);
    assert_ne!(error_code(&allowed), "POLICY_DENIED");
    assert!(scenario.resolve_calls() > 0);
}

#[test]
fn clipboard_reads_unlock_only_after_a_successful_own_write() {
    let scenario = Scenario::new(
        r#"@id("allow-clipboard-writes")
permit (principal, action in Desktop::Action::"clipboard", resource);

@id("clipboard-reads-after-own-write")
permit (principal, action == Desktop::Action::"clipboard-get", resource)
when temporal {
    formerly within 10m Desktop::Action::"clipboard-set"::response{}
};
"#,
    );

    let read_before = scenario.run(&["clipboard-get"]);
    assert_eq!(
        error_code(&read_before),
        "POLICY_DENIED",
        "nothing written yet: the read is an exfiltration attempt"
    );

    scenario
        .adapter
        .pasteboard_broken
        .store(true, Ordering::SeqCst);
    let failed_write = scenario.run(&["clipboard-set", "agent scratch"]);
    assert_eq!(error_code(&failed_write), "ACTION_FAILED");
    let read_after_failure = scenario.run(&["clipboard-get"]);
    assert_eq!(
        error_code(&read_after_failure),
        "POLICY_DENIED",
        "a write that fails at the pasteboard is recorded as an error event, which is history, not \
         permission"
    );

    scenario
        .adapter
        .pasteboard_broken
        .store(false, Ordering::SeqCst);
    let write = scenario.run(&["clipboard-set", "agent scratch"]);
    assert_eq!(error_code(&write), "OK");

    let read = scenario.run(&["clipboard-get"]).expect("read is permitted");
    assert!(
        read.to_string().contains("agent scratch"),
        "the read returns what the agent wrote: {read}"
    );
}

#[test]
fn batch_items_are_gated_one_by_one() {
    let scenario = Scenario::new(
        r#"@id("allow-clipboard-writes-only")
permit (principal, action == Desktop::Action::"clipboard-set", resource);
"#,
    );
    let body = scenario
        .run(&[
            "batch",
            r#"[{"command":"clipboard-get"},{"command":"clipboard-set","args":{"text":"from batch"}}]"#,
        ])
        .expect("the batch wrapper itself is exempt");
    let results = body["results"].as_array().expect("batch results");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["ok"], false);
    assert_eq!(results[0]["error"]["code"], "POLICY_DENIED");
    assert_eq!(results[1]["ok"], true, "second item: {}", results[1]);
    assert_eq!(
        scenario.adapter.clipboard.lock().unwrap().as_deref(),
        Some("from batch"),
        "the permitted item really ran"
    );
}

#[test]
fn edited_policies_apply_to_the_next_command() {
    let scenario = Scenario::new(
        r#"@id("allow-observation-only")
permit (principal, action in Desktop::Action::"observe", resource);
"#,
    );
    let denied = scenario.run(&["clipboard-set", "first try"]);
    assert_eq!(error_code(&denied), "POLICY_DENIED");
    assert_eq!(scenario.adapter.clipboard.lock().unwrap().as_deref(), None);

    scenario.rewrite_policy(
        r#"@id("allow-observation-only")
permit (principal, action in Desktop::Action::"observe", resource);

@id("owner-now-allows-clipboard")
permit (principal, action in Desktop::Action::"clipboard", resource);
"#,
    );
    let allowed = scenario.run(&["clipboard-set", "second try"]);
    assert_eq!(error_code(&allowed), "OK");
    assert_eq!(
        scenario.adapter.clipboard.lock().unwrap().as_deref(),
        Some("second try")
    );
}

#[test]
fn deny_by_default_blocks_whatever_no_policy_permits() {
    let scenario = Scenario::new(
        r#"@id("allow-observation-only")
permit (principal, action in Desktop::Action::"observe", resource);
"#,
    );
    let mutate = scenario.run(&["clipboard-clear"]);
    assert_eq!(error_code(&mutate), "POLICY_DENIED");
    let message = mutate
        .as_ref()
        .err()
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        message.contains("earlier step"),
        "a refusal nothing permitted tells the agent a permit may need an earlier step: {message}"
    );

    let observe = scenario.run(&["list-apps"]);
    assert_ne!(
        error_code(&observe),
        "POLICY_DENIED",
        "observation stays permitted; it stops at the desk's unsupported call"
    );
}

/// A refusal by policy is not cured by waiting or by repeating the call,
/// so the envelope must not hand the agent a retry strategy, and it names
/// the layer that refused, because the agent reports it to that layer's
/// owner.
#[test]
fn a_policy_refusal_offers_no_retry_and_names_its_layer() {
    let scenario = Scenario::new(
        r#"@id("no-displays")
forbid (principal, action == Desktop::Action::"list-displays", resource);
"#,
    );
    let error = scenario
        .run(&["list-displays"])
        .expect_err("the forbid refuses list-displays");
    let payload = agent_desktop_core::response::output::ErrorPayload::from_app_error(&error);
    let envelope = serde_json::to_value(&payload).expect("payload serializes");
    assert_eq!(envelope["code"], "POLICY_DENIED");
    assert!(envelope.get("recovery").is_none(), "{envelope}");
    assert_eq!(envelope["details"]["retryable"], false, "{envelope}");
    assert_eq!(
        envelope["details"]["layer"], "--policy/--policy-dir",
        "{envelope}"
    );
    assert!(
        envelope["suggestion"]
            .as_str()
            .is_some_and(|text| text.contains("--policy") && !text.contains("owner")),
        "a harness layer is not blamed on the machine owner: {envelope}"
    );
}
