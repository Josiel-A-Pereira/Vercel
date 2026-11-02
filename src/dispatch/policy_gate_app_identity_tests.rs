//! A policy names an application once; an agent can reach it under many
//! names. The gate must judge the application itself: the running app an
//! alias, another letter case or a bundle id names; the installed app a
//! launch would start; the owner of the window a window-scoped command acts
//! on; and the app's stable identifier, which a localized or renamed
//! display name does not change.

use super::policy_gate_desk::{Scenario, Screen, error_code, pid_of};

const VAULT_BY_NAME: &str = r#"@id("allow-all")
permit (principal, action, resource);

@id("hands-off-the-vault")
forbid (principal, action, resource == Desktop::App::"1Password");
"#;

const VAULT_BY_BUNDLE: &str = r#"@id("allow-all")
permit (principal, action, resource);

@id("hands-off-the-vault")
forbid (principal, action, resource in Desktop::AppId::"com.1password.1password");
"#;

fn desk(policy: &str) -> Scenario {
    let scenario = Scenario::new(policy);
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.apps = vec![
        Screen::app("1Password", "com.1password.1password"),
        Screen::app("TextEdit", "com.apple.TextEdit"),
    ];
    screen.windows = vec![
        Screen::window("w-vault", "1Password", "Vault", (0.0, 0.0, 400.0, 400.0)),
        Screen::window("w-notes", "TextEdit", "Notes", (500.0, 500.0, 300.0, 300.0)),
    ];
    screen.focused = Some("w-notes".to_string());
    drop(screen);
    scenario
}

fn assert_denied(result: &Result<serde_json::Value, agent_desktop_core::AppError>, what: &str) {
    assert_eq!(error_code(result), "POLICY_DENIED", "{what}: {result:?}");
}

fn assert_not_denied(result: &Result<serde_json::Value, agent_desktop_core::AppError>, what: &str) {
    assert_ne!(error_code(result), "POLICY_DENIED", "{what}: {result:?}");
}

#[test]
fn a_typed_app_name_is_judged_as_the_running_app_it_names() {
    let scenario = desk(VAULT_BY_NAME);
    for spelling in [
        "1Password",
        "1password",
        "1PASSWORD",
        "com.1password.1password",
    ] {
        assert_denied(
            &scenario.run(&["press", "cmd+v", "--app", spelling]),
            spelling,
        );
        assert_denied(&scenario.run(&["snapshot", "--app", spelling]), spelling);
    }
    assert_not_denied(
        &scenario.run(&["press", "cmd+v", "--app", "textedit"]),
        "another app under any spelling is still another app",
    );
}

#[test]
fn a_bundle_id_rule_holds_whatever_the_app_is_called() {
    let scenario = desk(VAULT_BY_BUNDLE);
    scenario.adapter.screen.lock().unwrap().apps[0] = agent_desktop_core::AppInfo {
        pid: pid_of("Tresor"),
        ..Screen::app("Tresor", "com.1password.1password")
    };
    let localized_ref = scenario.seed_ref("Tresor", "Vault", "button", "Unlock", false);
    assert_denied(
        &scenario.run(&["click", &localized_ref]),
        "a ref into the vault under a localized name",
    );
    assert_denied(
        &scenario.run(&["launch", "com.1password.1password"]),
        "the bundle id names the running vault",
    );

    let notes_ref = scenario.seed_ref("TextEdit", "Notes", "button", "Save", false);
    assert_not_denied(
        &scenario.run(&["click", &notes_ref]),
        "TextEdit is not the vault",
    );
}

#[test]
fn an_app_that_is_not_running_is_judged_as_the_installed_app() {
    let scenario = desk(VAULT_BY_BUNDLE);
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.apps.retain(|app| app.name != "1Password");
    screen.windows.retain(|window| window.app != "1Password");
    screen.installed = vec![agent_desktop_core::InstalledApp {
        name: "1Password".to_string(),
        bundle_id: Some("com.1password.1password".to_string()),
    }];
    drop(screen);
    assert_denied(&scenario.run(&["launch", "1password"]), "launch by name");
    assert_denied(
        &scenario.run(&["launch", "com.1password.1password"]),
        "launch by bundle id",
    );
    assert_not_denied(
        &scenario.run(&["launch", "Calculator"]),
        "an app nothing forbids",
    );
}

#[test]
fn an_installed_alias_of_a_running_app_is_judged_as_the_running_app() {
    let scenario = Scenario::new(
        r#"@id("allow-all")
permit (principal, action, resource);

@id("hands-off-the-editor")
forbid (principal, action, resource == Desktop::App::"gnome-text-editor");
"#,
    );
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.apps = vec![Screen::app("gnome-text-editor", "org.gnome.TextEditor")];
    screen.installed = vec![agent_desktop_core::InstalledApp {
        name: "Text Editor".to_string(),
        bundle_id: Some("org.gnome.TextEditor".to_string()),
    }];
    drop(screen);
    assert_denied(
        &scenario.run(&["press", "ctrl+s", "--app", "Text Editor"]),
        "the desktop Name of the running editor",
    );
}

#[test]
fn window_scoped_commands_are_judged_by_the_window_they_act_on() {
    let scenario = desk(VAULT_BY_NAME);
    assert_denied(
        &scenario.run(&["minimize", "--window-id", "w-vault"]),
        "minimize by window id",
    );
    assert_denied(
        &scenario.run(&["snapshot", "--window-id", "w-vault"]),
        "snapshot by window id",
    );
    assert_denied(
        &scenario.run(&["focus-window", "--title", "Vault"]),
        "focus by title",
    );
    assert_not_denied(
        &scenario.run(&["snapshot", "--window-id", "w-notes"]),
        "the notes window belongs to TextEdit",
    );

    scenario.adapter.screen.lock().unwrap().focused = Some("w-vault".to_string());
    assert_denied(
        &scenario.run(&["snapshot"]),
        "snapshot of the focused vault",
    );
    assert_denied(
        &scenario.run(&["list-surfaces"]),
        "surfaces of the focused vault",
    );
}

#[test]
fn an_unreadable_app_inventory_fails_closed() {
    let scenario = desk(VAULT_BY_NAME);
    let notes_ref = scenario.seed_ref("TextEdit", "Notes", "button", "Save", false);
    scenario.adapter.screen.lock().unwrap().inventory_broken = true;
    for args in [
        &["press", "cmd+v", "--app", "TextEdit"][..],
        &["click", &notes_ref][..],
    ] {
        let result = scenario.run(args);
        assert_denied(&result, "an app the gate cannot identify");
        let details = match result {
            Err(agent_desktop_core::AppError::Adapter(error)) => error.details,
            _ => None,
        };
        assert_eq!(
            details.as_ref().and_then(|d| d["kind"].as_str()),
            Some("dogwood_policy_target_unknown"),
            "{args:?}: {details:?}"
        );
    }
}
