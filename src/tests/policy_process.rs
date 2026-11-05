//! Real-case policy tests against the actual binary: each step is one CLI
//! invocation, exactly as an agent harness would issue it, with policies
//! discovered from the state root's policies folder, the environment
//! variable, or the global flags. Only permission-free, side-effect-free
//! commands are used, so the suite never touches the real desktop,
//! clipboard, or TCC permissions.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agent-desktop"))
}

struct PolicyHome {
    root: PathBuf,
}

impl PolicyHome {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "agent-desktop-policy-process-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let policies = root.join("policies");
        std::fs::create_dir_all(&policies).unwrap();
        restrict(&root);
        restrict(&policies);
        Self { root }
    }

    fn policy_dir(&self) -> PathBuf {
        self.root.join("policies")
    }

    fn write_policy(&self, name: &str, text: &str) -> PathBuf {
        let path = self.policy_dir().join(name);
        std::fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        path
    }

    /// A private directory beside the owner's folder holding one policy,
    /// as a harness or an agent would pass it.
    fn layer(&self, name: &str, text: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        restrict(&dir);
        let file = dir.join("layer.dogwood");
        std::fs::write(&file, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        dir
    }

    fn run(&self, args: &[&str]) -> serde_json::Value {
        self.run_command(binary().args(args))
    }

    fn run_with_env(&self, args: &[&str], name: &str, value: &Path) -> serde_json::Value {
        self.run_command(binary().args(args).env(name, value))
    }

    fn run_command(&self, command: &mut Command) -> serde_json::Value {
        let output = command
            .env("AGENT_DESKTOP_HOME", &self.root)
            .output()
            .expect("binary starts");
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "stdout is one JSON envelope ({error}): {:?} / stderr {:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }
}

impl Drop for PolicyHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

/// One agent session under one owner policy, step by step: exempt
/// administration, permitted observation, a forbidden app, deny-by-default
/// for mutations, and a temporal clause that unlocks a command only after
/// the observation it demands has succeeded.
#[test]
fn an_agent_session_runs_under_the_owner_policy() {
    let home = PolicyHome::new("session");
    home.write_policy(
        "guardrails.dogwood",
        r#"@id("inventory-may-be-listed")
permit (principal, action in [Desktop::Action::"list-apps", Desktop::Action::"list-displays"], resource);

@id("hands-off-the-password-manager")
forbid (principal, action, resource == Desktop::App::"1Password");

@id("windows-only-after-displays")
permit (principal, action == Desktop::Action::"list-windows", resource)
when temporal {
    formerly within 1h Desktop::Action::"list-displays"::response{}
};
"#,
    );

    let version = home.run(&["version"]);
    assert_eq!(version["ok"], true, "administration is exempt: {version}");

    let early_windows = home.run(&["list-windows"]);
    assert_eq!(early_windows["ok"], false);
    assert_eq!(
        early_windows["error"]["code"], "POLICY_DENIED",
        "the temporal clause is unsatisfied before any list-displays: {early_windows}"
    );

    let spying = home.run(&["list-apps", "--app", "1Password"]);
    assert_eq!(spying["error"]["code"], "POLICY_DENIED");
    assert!(
        spying["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("hands-off-the-password-manager"),
        "the denial names the deciding policy: {spying}"
    );

    let displays = home.run(&["list-displays"]);
    assert_eq!(
        displays["ok"], true,
        "permitted observation runs: {displays}"
    );

    let windows = home.run(&["list-windows"]);
    assert_ne!(
        windows["error"]["code"], "POLICY_DENIED",
        "the successful list-displays response satisfies the clause; the OS-level listing itself \
         may still need grants this suite does not assume: {windows}"
    );

    let mutate = home.run(&["clipboard-clear"]);
    assert_eq!(
        mutate["error"]["code"], "POLICY_DENIED",
        "nothing permits mutations, so deny-by-default holds: {mutate}"
    );
    assert_eq!(mutate["error"]["disposition"]["delivery"], "not_delivered");
}

#[test]
fn a_broken_policy_fails_closed_for_mutations_and_open_for_reads() {
    let home = PolicyHome::new("broken");
    home.write_policy("broken.dogwood", "permit (principal, action");

    let mutate = home.run(&["clipboard-clear"]);
    assert_eq!(mutate["ok"], false);
    assert_eq!(mutate["error"]["code"], "POLICY_DENIED");
    assert_eq!(
        mutate["error"]["details"]["kind"],
        "dogwood_policy_unavailable"
    );

    let observe = home.run(&["list-displays"]);
    assert_eq!(
        observe["ok"], true,
        "reads stay available so the agent can report the broken policy: {observe}"
    );

    let status = home.run(&["status"]);
    let layer = &status["data"]["policies"]["layers"][0];
    assert_eq!(
        layer["installed"], false,
        "status shows the owner it did not load: {status}"
    );
    assert!(
        layer["error"]
            .as_str()
            .is_some_and(|error| error.contains("policy rejected")),
        "and why: {status}"
    );
}

#[test]
fn status_reports_no_policies_when_none_are_configured() {
    let home = PolicyHome::new("status-none");
    let status = home.run(&["status"]);
    assert_eq!(status["data"]["policies"]["configured"], false, "{status}");
    assert_eq!(
        status["data"]["policies"]["layers"],
        serde_json::json!([]),
        "{status}"
    );
}

#[cfg(unix)]
#[test]
fn a_world_writable_policy_source_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let home = PolicyHome::new("loose");
    let file = home.write_policy("loose.dogwood", "permit (principal, action, resource);");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();

    let mutate = home.run(&["clipboard-clear"]);
    assert_eq!(mutate["error"]["code"], "POLICY_DENIED");
    assert!(
        mutate["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("insecure"),
        "the refusal says why: {mutate}"
    );
}

/// Two sessions run side by side and share one policy store.
/// Both must reach real verdicts: neither may find the policy store held by
/// the other and fall back to "policies could not be enforced".
#[test]
fn two_sessions_side_by_side_both_get_real_verdicts() {
    let home = PolicyHome::new("sessions");
    home.write_policy(
        "guardrails.dogwood",
        r#"@id("displays-only")
permit (principal, action == Desktop::Action::"list-displays", resource);
"#,
    );
    let session_id = |label: &str| {
        let started = home.run(&["session", "start"]);
        started["data"]["session_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{label} session starts: {started}"))
            .to_string()
    };
    let first = session_id("first");
    let second = session_id("second");

    let first_read = home.run(&["--session", &first, "list-displays"]);
    assert_eq!(
        first_read["ok"], true,
        "first session observes: {first_read}"
    );

    for session in [&second, &first] {
        let mutation = home.run(&["--session", session, "clipboard-clear"]);
        assert_eq!(mutation["error"]["code"], "POLICY_DENIED");
        assert_eq!(
            mutation["error"]["details"]["kind"], "dogwood_policy",
            "session {session} gets a real verdict while the other session is live: {mutation}"
        );
    }
}

#[path = "policy_layers_tests.rs"]
mod layers;

/// Every command is its own process, and a gate held in a static is never
/// dropped, so the binary must close its stores itself before it exits;
/// otherwise no store ever checkpoints and every open replays the whole
/// log. 40 commands record 80 events, past the checkpoint interval.
#[test]
fn one_shot_processes_checkpoint_the_log_they_grow() {
    let home = PolicyHome::new("checkpoint");
    home.write_policy("allow.dogwood", "permit (principal, action, resource);");
    for _ in 0..40 {
        let displays = home.run(&["list-displays"]);
        assert_ne!(
            displays["error"]["code"], "POLICY_DENIED",
            "the policy permits everything: {displays}"
        );
    }
    let checkpointed = std::fs::read_dir(home.root.join("policy-state"))
        .expect("the gate keeps its history under the state root")
        .flatten()
        .any(|store| store.path().join("checkpoint.offset").is_file());
    assert!(
        checkpointed,
        "a process that grew the log past the interval checkpoints it"
    );
}

/// An MCP server runs each tool call as its own CLI process, so the owner's
/// folder and the policy flags given to `mcp` judge every call it serves.
#[test]
fn an_mcp_server_judges_every_tool_call_under_its_policy_flags() {
    use std::io::Write;
    use std::process::Stdio;
    let home = PolicyHome::new("mcp");
    let layer = home.layer(
        "harness",
        r#"@id("allow-all")
permit (principal, action, resource);

@id("no-display-listing")
forbid (principal, action == Desktop::Action::"list-displays", resource);"#,
    );
    let mut child = binary()
        .arg("--policy-dir")
        .arg(&layer)
        .arg("mcp")
        .env("AGENT_DESKTOP_HOME", &home.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("binary starts");
    let mut stdin = child.stdin.take().expect("stdin");
    for (id, tool) in [(1, "desktop_list_displays"), (2, "desktop_version")] {
        let request = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                                          "params": { "name": tool, "arguments": {} } });
        writeln!(stdin, "{request}").expect("request written");
    }
    drop(stdin);
    let output = child.wait_with_output().expect("server exits");
    let mut responses = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON-RPC line"))
        .collect::<Vec<_>>();
    responses.sort_by_key(|response| response["id"].as_i64());

    let refused = &responses[0]["result"]["structuredContent"];
    assert_eq!(refused["error"]["code"], "POLICY_DENIED", "{responses:?}");
    assert_eq!(
        refused["error"]["details"]["policies"],
        serde_json::json!(["no-display-listing"]),
        "{refused}"
    );
    assert_eq!(responses[1]["result"]["isError"], false, "{responses:?}");
}

/// Text piped with `--stdin` is read before the gate runs, so a refusal must
/// still stop the command and must not repeat the text it consumed.
#[test]
fn a_refused_stdin_command_never_repeats_the_text_it_read() {
    use std::io::Write;
    use std::process::Stdio;
    const SECRET: &str = "piped-secret-text";
    let home = PolicyHome::new("stdin");
    let layer = home.layer(
        "harness",
        r#"@id("no-set-value")
forbid (principal, action == Desktop::Action::"set-value", resource);"#,
    );
    let mut child = binary()
        .arg("--policy-dir")
        .arg(&layer)
        .args(["set-value", "@s12345678:e1", "--stdin"])
        .env("AGENT_DESKTOP_HOME", &home.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary starts");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(SECRET.as_bytes()).expect("text written");
    drop(stdin);
    let output = child.wait_with_output().expect("binary exits");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON envelope");

    assert_eq!(envelope["error"]["code"], "POLICY_DENIED", "{envelope}");
    assert!(!stdout.contains(SECRET), "{stdout}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains(SECRET),
        "the text reached stderr"
    );
}
