//! What a command can reach beyond the app it names: the titles of the
//! app's windows, the shell surfaces of the OS, the notifications of apps
//! it does not name, and the history a one-shot process leaves behind.

use super::policy_gate_desk::{Scenario, Screen, error_code};

const NO_PRIVATE_WINDOWS: &str = r#"@id("allow-all")
permit (principal, action, resource);

@id("no-private-windows")
forbid (principal, action, resource)
when { context.input.window like "*Private*" };
"#;

const HANDS_OFF_THE_VAULT: &str = r#"@id("allow-all")
permit (principal, action, resource);

@id("hands-off-the-vault")
forbid (principal, action, resource in Desktop::AppId::"com.1password.1password");
"#;

fn browser_desk() -> Scenario {
    let scenario = Scenario::new(NO_PRIVATE_WINDOWS);
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.apps = vec![
        Screen::app("Safari", "com.apple.Safari"),
        Screen::app("TextEdit", "com.apple.TextEdit"),
    ];
    screen.windows = vec![
        Screen::window("w-news", "Safari", "News", (0.0, 0.0, 400.0, 400.0)),
        Screen::window(
            "w-private",
            "Safari",
            "Private Browsing",
            (400.0, 0.0, 400.0, 400.0),
        ),
        Screen::window("w-notes", "TextEdit", "Notes", (0.0, 400.0, 400.0, 400.0)),
    ];
    screen.focused = Some("w-notes".to_string());
    drop(screen);
    scenario
}

fn kind_of(result: &Result<serde_json::Value, agent_desktop_core::AppError>) -> Option<String> {
    match result {
        Err(agent_desktop_core::AppError::Adapter(error)) => error
            .details
            .as_ref()
            .and_then(|details| details["kind"].as_str())
            .map(str::to_string),
        _ => None,
    }
}

/// Notification mutations also need `--headed`, a refusal of their own
/// that carries no Dogwood kind.
fn policies_allowed(result: &Result<serde_json::Value, agent_desktop_core::AppError>) -> bool {
    !kind_of(result).is_some_and(|kind| kind.starts_with("dogwood_policy"))
}

#[test]
fn naming_the_app_judges_every_window_it_may_act_on() {
    let scenario = browser_desk();
    for args in [
        &["snapshot", "--app", "Safari", "-i"][..],
        &["screenshot", "--app", "safari"],
        &["list-surfaces", "--app", "com.apple.Safari"],
        &["press", "cmd+v", "--app", "Safari"],
        &["focus-window", "--app", "Safari"],
    ] {
        let result = scenario.run(args);
        assert_eq!(error_code(&result), "POLICY_DENIED", "{args:?}: {result:?}");
        assert_eq!(
            kind_of(&result).as_deref(),
            Some("dogwood_policy"),
            "{args:?}"
        );
    }
    assert_ne!(
        error_code(&scenario.run(&["snapshot", "--app", "TextEdit", "-i"])),
        "POLICY_DENIED",
        "an app with no private window is not refused"
    );
    assert_ne!(
        error_code(&scenario.run(&["snapshot", "--window-id", "w-news", "-i"])),
        "POLICY_DENIED",
        "naming the one window keeps the judgment to that window"
    );
}

const HANDS_OFF_THE_DOCK: &str = r#"@id("allow-all")
permit (principal, action, resource);

@id("hands-off-the-dock")
forbid (principal, action, resource in Desktop::AppId::"com.apple.dock");
"#;

#[test]
fn a_shell_surface_is_judged_as_the_shell_process_that_hosts_it() {
    let scenario = Scenario::new(HANDS_OFF_THE_DOCK);
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.apps = vec![
        Screen::app("Dock", "com.apple.dock"),
        Screen::app("TextEdit", "com.apple.TextEdit"),
    ];
    screen.windows = vec![Screen::window(
        "w-notes",
        "TextEdit",
        "Notes",
        (0.0, 0.0, 400.0, 400.0),
    )];
    screen.focused = Some("w-notes".to_string());
    screen.shell = Some(Screen::window(
        "w-dock",
        "Dock",
        "Dock",
        (0.0, 900.0, 1440.0, 80.0),
    ));
    drop(screen);
    for args in [
        &["snapshot", "--surface", "dock", "-i"][..],
        &["snapshot", "--surface", "dock", "--app", "TextEdit"],
        &["find", "--surface", "dock", "--role", "button"],
    ] {
        let result = scenario.run(args);
        assert_eq!(
            kind_of(&result).as_deref(),
            Some("dogwood_policy"),
            "{args:?}: {result:?}"
        );
    }
    assert!(
        policies_allowed(&scenario.run(&["snapshot", "-i"])),
        "the focused TextEdit window stays readable"
    );
}

#[test]
fn a_platform_without_shell_surfaces_judges_the_window_it_falls_back_to() {
    let scenario = Scenario::new(HANDS_OFF_THE_DOCK);
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.apps = vec![Screen::app("Dock", "com.apple.dock")];
    screen.windows = vec![Screen::window(
        "w-dock",
        "Dock",
        "Dock",
        (0.0, 900.0, 1440.0, 80.0),
    )];
    screen.focused = Some("w-dock".to_string());
    drop(screen);
    let result = scenario.run(&["snapshot", "--surface", "taskbar", "-i"]);
    assert_eq!(
        kind_of(&result).as_deref(),
        Some("dogwood_policy"),
        "{result:?}"
    );
}

#[test]
fn a_notification_named_only_by_title_cannot_be_judged() {
    let scenario = Scenario::new(HANDS_OFF_THE_VAULT);
    let action = scenario.run(&[
        "notification-action",
        "1",
        "Open",
        "--expected-title",
        "Vault",
    ]);
    assert_eq!(error_code(&action), "POLICY_DENIED");
    assert_eq!(
        kind_of(&action).as_deref(),
        Some("dogwood_policy_target_unknown")
    );
    let dismiss = scenario.run(&["dismiss-notification", "1", "--expected-title", "Vault"]);
    assert_eq!(
        kind_of(&dismiss).as_deref(),
        Some("dogwood_policy_target_unknown")
    );
}

#[test]
fn a_notification_is_judged_by_the_app_its_identity_names() {
    let scenario = Scenario::new(HANDS_OFF_THE_VAULT);
    scenario.adapter.screen.lock().unwrap().apps = vec![
        Screen::app("1Password", "com.1password.1password"),
        Screen::app("TextEdit", "com.apple.TextEdit"),
    ];
    let vault = scenario.run(&[
        "notification-action",
        "1",
        "Open",
        "--expected-app",
        "1Password",
    ]);
    assert_eq!(kind_of(&vault).as_deref(), Some("dogwood_policy"));
    let dismissed = scenario.run(&["dismiss-notification", "1", "--app", "1password"]);
    assert_eq!(kind_of(&dismissed).as_deref(), Some("dogwood_policy"));
    assert!(policies_allowed(&scenario.run(&[
        "notification-action",
        "1",
        "Open",
        "--expected-app",
        "TextEdit"
    ])));
}

#[test]
fn notifications_of_every_app_are_judged_as_every_running_app() {
    let scenario = Scenario::new(HANDS_OFF_THE_VAULT);
    scenario.adapter.screen.lock().unwrap().apps =
        vec![Screen::app("TextEdit", "com.apple.TextEdit")];
    assert!(
        policies_allowed(&scenario.run(&["dismiss-all-notifications"])),
        "no forbidden app runs"
    );
    scenario
        .adapter
        .screen
        .lock()
        .unwrap()
        .apps
        .push(Screen::app("1Password", "com.1password.1password"));
    for args in [&["dismiss-all-notifications"][..], &["list-notifications"]] {
        let result = scenario.run(args);
        assert_eq!(
            kind_of(&result).as_deref(),
            Some("dogwood_policy"),
            "{args:?}"
        );
    }
    assert!(
        policies_allowed(&scenario.run(&["dismiss-all-notifications", "--app", "TextEdit"])),
        "an app filter keeps the vault's notifications out of reach"
    );
}

#[test]
fn releasing_the_stores_checkpoints_a_grown_log() {
    let scenario = Scenario::new(HANDS_OFF_THE_VAULT);
    for _ in 0..40 {
        assert_ne!(
            error_code(&scenario.run(&["clipboard-get"])),
            "POLICY_DENIED"
        );
    }
    let checkpoints = || {
        std::fs::read_dir(scenario.state_root().join("policy-state"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|store| store.path().join("checkpoint.offset").is_file())
            .count()
    };
    assert_eq!(checkpoints(), 0, "a store still held has not checkpointed");
    crate::command_policy::dogwood::release_stores();
    assert_eq!(
        checkpoints(),
        1,
        "closing the held store checkpoints the 80 events it gathered"
    );
}
