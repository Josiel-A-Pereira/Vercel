//! Keyboard and pointer input that names no ref still lands in some app.
//! The gate must judge it as that app, not as `system`: otherwise "never
//! touch the password manager" is bypassed by a shortcut sent to whatever
//! is focused, or a click at the coordinates where its window sits.

use agent_desktop_core::AppInfo;

use super::policy_gate_desk::{Scenario, Screen, error_code};

const HANDS_OFF_THE_VAULT: &str = r#"@id("allow-input")
permit (principal, action in Desktop::Action::"input", resource);

@id("hands-off-the-vault")
forbid (principal, action, resource == Desktop::App::"1Password");
"#;

fn desk(focused: &str) -> Scenario {
    let scenario = Scenario::new(HANDS_OFF_THE_VAULT);
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.windows = vec![
        Screen::window("w-vault", "1Password", "Vault", (0.0, 0.0, 400.0, 400.0)),
        Screen::window("w-notes", "TextEdit", "Notes", (300.0, 300.0, 600.0, 600.0)),
    ];
    screen.focused = Some(focused.to_string());
    drop(screen);
    scenario
}

fn denial_names_the_vault_policy(result: &Result<serde_json::Value, agent_desktop_core::AppError>) {
    assert_eq!(error_code(result), "POLICY_DENIED");
    let message = result.as_ref().err().map(|error| error.to_string());
    assert!(
        message
            .as_deref()
            .is_some_and(|text| text.contains("hands-off-the-vault")),
        "the denial names the deciding policy: {message:?}"
    );
}

#[test]
fn shortcuts_are_judged_by_the_focused_app() {
    let scenario = desk("w-vault");
    denial_names_the_vault_policy(&scenario.run(&["press", "cmd+v"]));

    scenario.adapter.screen.lock().unwrap().focused = Some("w-notes".to_string());
    assert_ne!(
        error_code(&scenario.run(&["press", "cmd+v"])),
        "POLICY_DENIED"
    );
}

#[test]
fn a_frontmost_app_with_no_window_is_judged_as_itself() {
    let scenario = Scenario::new(
        r#"@id("allow-all")
permit (principal, action, resource);

@id("hands-off-the-vault")
forbid (principal, action, resource == Desktop::App::"1Password");
"#,
    );
    let reaches_the_frontmost_app: [&[&str]; 3] = [
        &["press", "cmd+v"],
        &["snapshot", "--surface", "menu"],
        &["snapshot", "--surface", "menubar"],
    ];
    let set_frontmost =
        |app: Option<AppInfo>| scenario.adapter.screen.lock().unwrap().frontmost = app;

    set_frontmost(Some(Screen::app("1Password", "com.1password.1password")));
    for args in reaches_the_frontmost_app {
        denial_names_the_vault_policy(&scenario.run(args));
    }

    set_frontmost(Some(Screen::app("TextEdit", "com.apple.TextEdit")));
    for args in reaches_the_frontmost_app {
        assert_ne!(error_code(&scenario.run(args)), "POLICY_DENIED", "{args:?}");
    }

    set_frontmost(None);
    assert_ne!(
        error_code(&scenario.run(&["press", "cmd+v"])),
        "POLICY_DENIED",
        "a platform that reports no frontmost app judges it as system"
    );

    scenario.adapter.screen.lock().unwrap().inventory_broken = true;
    let unreadable = scenario.run(&["press", "cmd+v"]);
    assert_eq!(error_code(&unreadable), "POLICY_DENIED", "{unreadable:?}");
}

#[test]
fn a_named_app_or_window_overrides_the_focused_one() {
    let scenario = desk("w-notes");
    denial_names_the_vault_policy(&scenario.run(&["press", "cmd+v", "--app", "1Password"]));
    denial_names_the_vault_policy(&scenario.run(&["press", "cmd+v", "--window-id", "w-vault"]));

    scenario.adapter.screen.lock().unwrap().focused = Some("w-vault".to_string());
    assert_ne!(
        error_code(&scenario.run(&["press", "cmd+v", "--app", "TextEdit"])),
        "POLICY_DENIED",
        "keys sent to a named app's process do not reach the focused one"
    );
}

#[test]
fn pointer_input_is_judged_by_the_window_under_the_point() {
    let scenario = desk("w-notes");
    denial_names_the_vault_policy(&scenario.run_headed(&["mouse-click", "--xy", "100,100"]));
    denial_names_the_vault_policy(&scenario.run_headed(&["hover", "--xy", "100,100"]));
    denial_names_the_vault_policy(&scenario.run_headed(&[
        "mouse-wheel",
        "--x",
        "100",
        "--y",
        "100",
    ]));

    assert_ne!(
        error_code(&scenario.run_headed(&["mouse-click", "--xy", "800,800"])),
        "POLICY_DENIED",
        "a click on the notes window is not a click on the vault"
    );
}

#[test]
fn a_drag_is_judged_at_both_ends() {
    let scenario = desk("w-notes");
    denial_names_the_vault_policy(&scenario.run_headed(&[
        "drag",
        "--from-xy",
        "800,800",
        "--to-xy",
        "100,100",
    ]));
    denial_names_the_vault_policy(&scenario.run_headed(&[
        "drag",
        "--from-xy",
        "100,100",
        "--to-xy",
        "800,800",
    ]));
}

#[test]
fn an_undeterminable_pointer_target_fails_closed() {
    let scenario = desk("w-notes");
    scenario.adapter.screen.lock().unwrap().overlap_unknown = true;
    let result = scenario.run_headed(&["mouse-click", "--xy", "350,350"]);
    assert_eq!(error_code(&result), "POLICY_DENIED");
    let details = match result {
        Err(agent_desktop_core::AppError::Adapter(error)) => error.details,
        _ => None,
    };
    assert_eq!(
        details.as_ref().and_then(|d| d["kind"].as_str()),
        Some("dogwood_policy_target_unknown"),
        "{details:?}"
    );
}

#[test]
fn the_receiving_window_title_reaches_policies() {
    let scenario = Scenario::new(
        r#"@id("allow-input")
permit (principal, action in Desktop::Action::"input", resource);

@id("no-typing-into-private-windows")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.window like "*Private*" };
"#,
    );
    let mut screen = scenario.adapter.screen.lock().unwrap();
    screen.windows = vec![Screen::window(
        "w-private",
        "Safari",
        "Private Browsing",
        (0.0, 0.0, 400.0, 400.0),
    )];
    screen.focused = Some("w-private".to_string());
    drop(screen);
    assert_eq!(
        error_code(&scenario.run(&["press", "cmd+v"])),
        "POLICY_DENIED"
    );
}

#[test]
fn without_policies_no_target_is_looked_up() {
    let scenario = desk("w-vault");
    let unpoliced = super::policy_gate_desk::run_command(
        &scenario,
        super::policy_gate_desk::parse(&["press", "cmd+v"]),
        &agent_desktop_core::context::CommandContext::default(),
    );
    assert_ne!(error_code(&unpoliced), "POLICY_DENIED");
    assert_eq!(
        scenario.target_queries(),
        0,
        "the gate stays free when no policy is configured"
    );
}
