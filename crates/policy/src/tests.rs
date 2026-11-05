use std::path::PathBuf;

use crate::discover::{Origin, PolicySources};
use crate::{CommandFacts, PolicyGate, TargetFacts, Verdict, catalog};

pub(crate) fn facts(command: &'static str, app: &str) -> CommandFacts {
    CommandFacts {
        command,
        agent: "default".to_string(),
        target: TargetFacts {
            app: app.to_string(),
            ..TargetFacts::default()
        },
        headed: false,
        mutating: catalog::group_for(command) != Some("observe"),
        session: String::new(),
    }
}

pub(crate) fn write_policy(dir: &std::path::Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write policy");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    }
    path
}

fn gate_with(policy: &str) -> (tempfile::TempDir, PolicyGate) {
    let dir = tempfile::tempdir().expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod tempdir");
    }
    let file = write_policy(dir.path(), "test.dogwood", policy);
    let sources = PolicySources::for_file(file, Origin::Cli);
    let gate = PolicyGate::open_at(&sources, &dir.path().join("gate")).expect("open gate");
    (dir, gate)
}

#[test]
fn every_group_is_declared() {
    for (command, group) in catalog::COMMAND_GROUPS {
        assert!(
            catalog::GROUPS.contains(group),
            "command {command} names undeclared group {group}"
        );
    }
}

#[test]
fn schema_installs_and_default_denies() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);
"#,
    );
    assert_eq!(
        gate.check(&facts("snapshot", "Finder")).expect("verdict"),
        Verdict::Allowed
    );
    match gate.check(&facts("click", "Finder")).expect("verdict") {
        Verdict::Denied { policies, .. } => {
            assert!(policies.is_empty(), "default deny: {policies:?}")
        }
        Verdict::Allowed => panic!("click must be denied without a permit"),
    }
}

#[test]
fn forbid_overrides_permit_and_names_the_policy() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-everything")
permit (principal, action, resource);

@id("hands-off-passwords")
forbid (principal, action in Desktop::Action::"input", resource == Desktop::App::"1Password");
"#,
    );
    assert_eq!(
        gate.check(&facts("click", "Safari")).expect("verdict"),
        Verdict::Allowed
    );
    match gate.check(&facts("type", "1Password")).expect("verdict") {
        Verdict::Denied { policies, .. } => {
            assert!(
                policies.iter().any(|id| id == "hands-off-passwords"),
                "expected the forbid's @id, got {policies:?}"
            );
        }
        Verdict::Allowed => panic!("typing into 1Password must be denied"),
    }
}

#[test]
fn a_rule_on_the_stable_app_id_holds_under_any_display_name() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-everything")
permit (principal, action, resource);

@id("hands-off-settings")
forbid (principal, action, resource in Desktop::AppId::"com.apple.systempreferences");
"#,
    );
    for localized in [
        "System Settings",
        "Impostazioni di Sistema",
        "Systemeinstellungen",
    ] {
        let mut click = facts("click", localized);
        click.target.app_id = "com.apple.systempreferences".to_string();
        match gate.check(&click).expect("verdict") {
            Verdict::Denied { policies, .. } => {
                assert_eq!(policies, ["hands-off-settings"], "{localized}")
            }
            Verdict::Allowed => panic!("{localized} is System Settings by its bundle id"),
        }
    }
    let mut other = facts("click", "System Settings");
    other.target.app_id = "com.example.lookalike".to_string();
    assert_eq!(
        gate.check(&other).expect("verdict"),
        Verdict::Allowed,
        "a lookalike name with another bundle id is a different app"
    );
    assert_eq!(
        gate.check(&facts("click", "Safari")).expect("verdict"),
        Verdict::Allowed,
        "an app with no known id is in no AppId"
    );
}

#[test]
fn an_app_id_rule_in_lowercase_matches_the_listed_letter_case() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-all")
permit (principal, action, resource);

@id("no-textedit")
forbid (principal, action, resource in Desktop::AppId::"com.apple.textedit");
"#,
    );
    let mut launch = facts("launch", "TextEdit");
    launch.target.app_id = "com.apple.TextEdit".to_string();
    assert!(matches!(
        gate.check(&launch).expect("verdict"),
        Verdict::Denied { .. }
    ));
}

#[test]
fn a_rule_can_list_several_app_ids() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-everything")
permit (principal, action, resource);

@id("hands-off-secrets")
forbid (principal, action in Desktop::Action::"input", resource)
when { resource in [Desktop::AppId::"com.1password.1password",
                    Desktop::AppId::"com.apple.Passwords"] };
"#,
    );
    for (name, id) in [
        ("1Password", "com.1password.1password"),
        ("Passwords", "com.apple.Passwords"),
    ] {
        let mut click = facts("click", name);
        click.target.app_id = id.to_string();
        assert!(
            matches!(gate.check(&click).expect("verdict"), Verdict::Denied { .. }),
            "{id}"
        );
    }
    let mut notes = facts("click", "Notes");
    notes.target.app_id = "com.apple.Notes".to_string();
    assert_eq!(gate.check(&notes).expect("verdict"), Verdict::Allowed);
}

#[test]
fn the_app_id_reaches_context_and_history() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);

@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    formerly within 10m Desktop::Action::"snapshot"::response{ input.app_id: context.input.app_id }
};
"#,
    );
    let mut snapshot = facts("snapshot", "Finder");
    snapshot.target.app_id = "com.apple.finder".to_string();
    assert_eq!(gate.check(&snapshot).expect("verdict"), Verdict::Allowed);
    gate.record(&snapshot, true).expect("record");

    let mut renamed = facts("click", "Finder (renamed)");
    renamed.target.app_id = "com.apple.finder".to_string();
    assert_eq!(gate.check(&renamed).expect("verdict"), Verdict::Allowed);
}

#[test]
fn context_fields_reach_policies() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-everything")
permit (principal, action, resource);

@id("never-secure-fields")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.secure == true };

@id("no-window-sniping")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.window like "*Private*" };
"#,
    );
    let mut secure = facts("type", "Safari");
    secure.target.role = "textfield".to_string();
    secure.target.secure = true;
    assert!(matches!(
        gate.check(&secure).expect("verdict"),
        Verdict::Denied { .. }
    ));
    let mut plain = facts("type", "Safari");
    plain.target.role = "textfield".to_string();
    assert_eq!(gate.check(&plain).expect("verdict"), Verdict::Allowed);
    let mut private_window = facts("click", "Safari");
    private_window.target.window = "Private Browsing".to_string();
    assert!(matches!(
        gate.check(&private_window).expect("verdict"),
        Verdict::Denied { .. }
    ));
}

#[test]
fn observe_before_act_needs_a_recent_snapshot_response() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);

@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    formerly within 10m Desktop::Action::"snapshot"::response{ input.app: context.input.app }
};
"#,
    );
    let click = facts("click", "Finder");
    assert!(
        matches!(gate.check(&click).expect("verdict"), Verdict::Denied { .. }),
        "no snapshot yet: click must be denied"
    );

    let snapshot = facts("snapshot", "Finder");
    assert_eq!(gate.check(&snapshot).expect("verdict"), Verdict::Allowed);
    gate.record(&snapshot, true).expect("record response");

    assert_eq!(
        gate.check(&click).expect("verdict"),
        Verdict::Allowed,
        "snapshot response recorded: click must be allowed"
    );

    let other_app = facts("click", "Safari");
    assert!(
        matches!(
            gate.check(&other_app).expect("verdict"),
            Verdict::Denied { .. }
        ),
        "the snapshot was of Finder, not Safari"
    );
}

#[test]
fn failed_actions_record_as_error_events() {
    let (_dir, mut gate) = gate_with(
        r#"@id("allow-everything")
permit (principal, action, resource);
"#,
    );
    let click = facts("click", "Finder");
    assert_eq!(gate.check(&click).expect("verdict"), Verdict::Allowed);
    gate.record(&click, false).expect("record error event");
}

#[test]
fn broken_policy_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod tempdir");
    }
    let file = write_policy(dir.path(), "broken.dogwood", "permit (principal, action");
    let sources = PolicySources::for_file(file, Origin::Cli);
    let error = PolicyGate::open_at(&sources, &dir.path().join("gate"))
        .expect_err("broken policy must be rejected");
    assert!(
        matches!(error, crate::PolicyError::Install { .. }),
        "{error}"
    );
}

#[test]
fn reopening_keeps_history_and_skips_reinstall() {
    let dir = tempfile::tempdir().expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod tempdir");
    }
    let file = write_policy(
        dir.path(),
        "temporal.dogwood",
        r#"@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);

@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    formerly within 10m Desktop::Action::"snapshot"::response{ input.app: context.input.app }
};
"#,
    );
    let sources = PolicySources::for_file(file, Origin::Cli);
    let gate_root = dir.path().join("gate");
    {
        let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("open gate");
        let snapshot = facts("snapshot", "Finder");
        assert_eq!(gate.check(&snapshot).expect("verdict"), Verdict::Allowed);
        gate.record(&snapshot, true).expect("record");
    }
    let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("reopen gate");
    assert_eq!(
        gate.check(&facts("click", "Finder")).expect("verdict"),
        Verdict::Allowed,
        "the snapshot response must survive a restart"
    );
}
