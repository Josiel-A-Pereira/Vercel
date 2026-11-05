//! The gated command surface: every CLI command the policy gate submits to
//! the engine, with the action group it belongs to. Commands absent from this
//! table (version, skills, status, permissions, session, trace,
//! cursor-overlay, batch) are administrative and never gated; batch is the
//! wrapper whose items are gated one by one when they execute.

/// `(command, group)` for every gated command. The schema is generated from
/// this table, so a command listed here can always be named in a policy and a
/// command added to the CLI fails the binary's exhaustiveness test until it
/// is classified here or exempted there.
pub const COMMAND_GROUPS: &[(&str, &str)] = &[
    ("snapshot", "observe"),
    ("find", "observe"),
    ("get", "observe"),
    ("is", "observe"),
    ("wait", "observe"),
    ("list-windows", "observe"),
    ("list-displays", "observe"),
    ("list-apps", "observe"),
    ("list-surfaces", "observe"),
    ("list-notifications", "observe"),
    ("screenshot", "capture"),
    ("clipboard-get", "capture"),
    ("click", "input"),
    ("double-click", "input"),
    ("triple-click", "input"),
    ("right-click", "input"),
    ("type", "input"),
    ("set-value", "input"),
    ("clear", "input"),
    ("focus", "input"),
    ("select", "input"),
    ("toggle", "input"),
    ("check", "input"),
    ("uncheck", "input"),
    ("expand", "input"),
    ("collapse", "input"),
    ("scroll", "input"),
    ("scroll-to", "input"),
    ("press", "input"),
    ("key-down", "input"),
    ("key-up", "input"),
    ("hover", "input"),
    ("drag", "input"),
    ("mouse-move", "input"),
    ("mouse-click", "input"),
    ("mouse-down", "input"),
    ("mouse-up", "input"),
    ("mouse-wheel", "input"),
    ("launch", "app"),
    ("close-app", "app"),
    ("focus-window", "app"),
    ("resize-window", "app"),
    ("move-window", "app"),
    ("minimize", "app"),
    ("maximize", "app"),
    ("restore", "app"),
    ("open-system-surface", "app"),
    ("dismiss-notification", "notifications"),
    ("dismiss-all-notifications", "notifications"),
    ("notification-action", "notifications"),
    ("clipboard-set", "clipboard"),
    ("clipboard-clear", "clipboard"),
];

/// The action groups, each also declared as a Cedar action so policies can
/// write `action in Desktop::Action::"input"`.
pub const GROUPS: &[&str] = &[
    "observe",
    "capture",
    "input",
    "app",
    "notifications",
    "clipboard",
];

/// The group a gated command belongs to, or `None` for an exempt command.
pub fn group_for(command: &str) -> Option<&'static str> {
    COMMAND_GROUPS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, group)| *group)
}

/// The built-in action schema, generated from [`COMMAND_GROUPS`] so the
/// vocabulary policies validate against can never drift from the CLI surface.
pub fn schema_text() -> &'static str {
    static SCHEMA: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(build_schema)
}

fn build_schema() -> String {
    let mut out = String::with_capacity(8 * 1024);
    out.push_str(
        "namespace Desktop {\n\
         \x20   type ActionInput = {\n\
         \x20       command: String,\n\
         \x20       app: String,\n\
         \x20       app_id: String,\n\
         \x20       window: String,\n\
         \x20       role: String,\n\
         \x20       element: String,\n\
         \x20       secure: Bool,\n\
         \x20       headed: Bool,\n\
         \x20       mutating: Bool,\n\
         \x20   };\n\
         \x20   type ActionOutput = { ok: Bool };\n\
         \x20   entity Agent;\n\
         \x20   entity AppId;\n\
         \x20   entity App in [AppId];\n",
    );
    for group in GROUPS {
        out.push_str(&format!(
            "    action \"{group}\" appliesTo {{\n\
             \x20       principal: [Agent],\n\
             \x20       resource: [App],\n\
             \x20       context: {{ input: ActionInput, output: ActionOutput }}\n\
             \x20   }};\n"
        ));
    }
    for (command, group) in COMMAND_GROUPS {
        out.push_str(&format!(
            "    action \"{command}\" in [Action::\"{group}\"] appliesTo {{\n\
             \x20       principal: [Agent],\n\
             \x20       resource: [App],\n\
             \x20       context: {{ input: ActionInput, output: ActionOutput }}\n\
             \x20   }};\n"
        ));
    }
    out.push_str("}\n");
    out
}
