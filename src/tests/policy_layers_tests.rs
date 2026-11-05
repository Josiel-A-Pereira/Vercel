//! Policy layers through the real binary: the owner's folder, the
//! environment variable, and each flag are separate layers, and a command
//! runs only if every one allows it.

use std::path::Path;

use super::{PolicyHome, restrict};

fn denial_names(result: &serde_json::Value, policy: &str) -> bool {
    result["error"]["code"] == "POLICY_DENIED"
        && result["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(policy))
}

/// An agent controls its own flags and environment, so a layer it adds
/// must never lift the owner's verdict, and a broken layer must not either.
#[test]
fn caller_layers_never_lift_the_owners_policies() {
    let home = PolicyHome::new("layers-owner");
    home.write_policy(
        "owner.dogwood",
        r#"@id("owner-permits-reads")
permit (principal, action in Desktop::Action::"observe", resource);

@id("owner-no-displays")
forbid (principal, action == Desktop::Action::"list-displays", resource);
"#,
    );
    let open = home.layer("agent-open", "permit (principal, action, resource);");
    let broken = home.root.join("agent-empty");
    std::fs::create_dir_all(&broken).unwrap();
    restrict(&broken);

    let owner_only = home.run(&["list-displays"]);
    assert!(
        denial_names(&owner_only, "owner-no-displays"),
        "{owner_only}"
    );

    let open_flag = home.run(&["--policy-dir", path(&open), "list-displays"]);
    assert!(
        denial_names(&open_flag, "owner-no-displays"),
        "a permissive --policy-dir adds a layer, it does not replace the owner's: {open_flag}"
    );

    let open_env = home.run_with_env(&["list-displays"], "AGENT_DESKTOP_POLICY_DIR", &open);
    assert!(
        denial_names(&open_env, "owner-no-displays"),
        "a permissive AGENT_DESKTOP_POLICY_DIR adds a layer too: {open_env}"
    );

    let broken_flag = home.run(&["--policy-dir", path(&broken), "list-displays"]);
    assert!(
        denial_names(&broken_flag, "owner-no-displays"),
        "a broken caller layer must not make reads fail open past the owner: {broken_flag}"
    );
}

/// A harness may tighten what its agent can do with a layer of its own,
/// through the flags or the environment.
#[test]
fn a_caller_layer_can_refuse_more_than_the_owner() {
    let home = PolicyHome::new("layers-harness");
    home.write_policy(
        "owner.dogwood",
        r#"@id("owner-permits-reads")
permit (principal, action in Desktop::Action::"observe", resource);
"#,
    );
    let strict = home.layer(
        "harness-strict",
        r#"@id("harness-permits-all")
permit (principal, action, resource);

@id("harness-no-displays")
forbid (principal, action == Desktop::Action::"list-displays", resource);
"#,
    );

    let owner_only = home.run(&["list-displays"]);
    assert_eq!(owner_only["ok"], true, "{owner_only}");

    let flagged = home.run(&["--policy-dir", path(&strict), "list-displays"]);
    assert!(denial_names(&flagged, "harness-no-displays"), "{flagged}");
    assert!(
        flagged["error"]["suggestion"]
            .as_str()
            .unwrap_or_default()
            .contains("--policy"),
        "the denial names the layer that refused: {flagged}"
    );

    let via_env = home.run_with_env(&["list-displays"], "AGENT_DESKTOP_POLICY_DIR", &strict);
    assert!(denial_names(&via_env, "harness-no-displays"), "{via_env}");

    let after = home.run(&["list-displays"]);
    assert_eq!(
        after["ok"], true,
        "a layer applies only to the calls that carry it: {after}"
    );
}

/// The harness and the agent share one command line: the agent's own
/// `--policy` file must not lift the harness's deny-by-default either.
#[test]
fn an_agents_flag_cannot_lift_a_harness_flag() {
    let home = PolicyHome::new("layers-flags");
    let harness = home.layer(
        "harness-apps-only",
        r#"@id("harness-apps-only")
permit (principal, action == Desktop::Action::"list-apps", resource);
"#,
    );
    let open = home.layer("agent-open", "permit (principal, action, resource);");
    let open_file = open.join("layer.dogwood");

    let harness_only = home.run(&["--policy-dir", path(&harness), "list-displays"]);
    assert_eq!(
        harness_only["error"]["code"], "POLICY_DENIED",
        "{harness_only}"
    );

    let lifted = home.run(&[
        "--policy-dir",
        path(&harness),
        "--policy",
        path(&open_file),
        "list-displays",
    ]);
    assert_eq!(
        lifted["error"]["code"], "POLICY_DENIED",
        "the agent's permit-everything file is a layer of its own: {lifted}"
    );
}

#[test]
fn an_empty_or_relative_policy_dir_variable_is_refused_before_anything_runs() {
    let home = PolicyHome::new("env-invalid");
    for value in ["", "relative/policies"] {
        let result = home.run_with_env(
            &["list-displays"],
            "AGENT_DESKTOP_POLICY_DIR",
            Path::new(value),
        );
        assert_eq!(
            result["error"]["code"], "INVALID_ARGS",
            "{value:?}: {result}"
        );
        assert_eq!(result["error"]["disposition"]["delivery"], "not_delivered");
    }
}

fn path(dir: &Path) -> &str {
    dir.to_str().expect("utf-8 path")
}
