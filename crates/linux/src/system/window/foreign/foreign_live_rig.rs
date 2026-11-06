use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use agent_desktop_core::{AppInfo, Deadline, InteractionLease, WindowFilter, WindowInfo};

use crate::system::interaction::interaction_lease::acquire;
use crate::system::process::liveness::{Liveness, liveness};
use crate::system::{SharedBus, list_windows};
use crate::tree::live::live_launch::{Launched, nonce};

const SETTLE: Duration = Duration::from_secs(30);
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/e2e-linux/apps/foreign_window.py"
);
pub(super) const DIALOG_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/e2e-linux/apps/foreign_dialog4.py"
);

const KEY_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/e2e-linux/apps/key_log_window.py"
);

/// Refuses to run unless the session is an isolated nested Hyprland the
/// caller named: the tests focus, maximize and close real windows.
pub(crate) fn require_isolated_hyprland() {
    assert_eq!(
        std::env::var("AGENT_DESKTOP_ISOLATED_HYPRLAND").as_deref(),
        Ok("1"),
        "foreign live tests act on real windows: set AGENT_DESKTOP_ISOLATED_HYPRLAND=1 only inside an isolated nested Hyprland"
    );
    assert_eq!(
        std::env::var("XDG_CURRENT_DESKTOP").as_deref(),
        Ok("Hyprland"),
        "foreign live tests need XDG_CURRENT_DESKTOP=Hyprland"
    );
    assert!(
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|value| !value.is_empty()),
        "foreign live tests need the isolated compositor's HYPRLAND_INSTANCE_SIGNATURE"
    );
    let owner = std::fs::metadata("/proc/self").map_or(0, |meta| meta.uid());
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    assert!(
        !runtime.is_empty() && runtime != format!("/run/user/{owner}"),
        "XDG_RUNTIME_DIR={runtime} is the login session's runtime directory, not an isolated one"
    );
}

pub(crate) fn deadline(ms: u64) -> Deadline {
    Deadline::after(ms).expect("a deadline builds")
}

pub(crate) fn lease() -> InteractionLease {
    acquire(deadline(15_000)).expect("the interaction lease is free")
}

/// A client as the compositor's own IPC reports it, read apart from the
/// product's route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Client {
    pub(crate) class: String,
    pub(crate) title: String,
    pub(crate) pid: u64,
    pub(crate) shown: (bool, bool),
    pub(crate) fullscreen: (i64, i64),
    pub(crate) recency: i64,
}

fn client_of(entry: &serde_json::Value) -> Client {
    let text = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
    Client {
        class: text("class"),
        title: text("title"),
        pid: entry["pid"].as_u64().unwrap_or_default(),
        shown: (
            entry["mapped"].as_bool().unwrap_or_default(),
            entry["hidden"].as_bool().unwrap_or_default(),
        ),
        fullscreen: (
            entry["fullscreen"].as_i64().unwrap_or(-1),
            entry["fullscreenClient"].as_i64().unwrap_or(-1),
        ),
        recency: entry["focusHistoryID"].as_i64().unwrap_or(-1),
    }
}

/// Every client of the isolated compositor, ordered by process and title so
/// two reads of an unchanged desktop are equal.
pub(crate) fn clients() -> Vec<Client> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .expect("hyprctl runs");
    let listed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("hyprctl clients -j is JSON");
    let mut all: Vec<Client> = listed
        .as_array()
        .map(|entries| entries.iter().map(client_of).collect())
        .unwrap_or_default();
    all.sort_by(|a, b| (a.pid, &a.title, a.recency).cmp(&(b.pid, &b.title, b.recency)));
    all
}

pub(crate) fn clients_of(pid: u32) -> Vec<Client> {
    clients()
        .into_iter()
        .filter(|client| client.pid == u64::from(pid))
        .collect()
}

pub(crate) fn active_title() -> String {
    clients()
        .into_iter()
        .find(|client| client.recency == 0)
        .map(|client| client.title)
        .unwrap_or_default()
}

/// A window the test launched, with its process identity.
pub(crate) struct Shown {
    pub(crate) pid: u32,
    pub(crate) window: WindowInfo,
}

impl Shown {
    pub(crate) fn app(&self) -> AppInfo {
        AppInfo {
            name: self.window.app.clone(),
            pid: self.window.pid,
            bundle_id: None,
            process_instance: self.window.process_instance.clone(),
            presentation: None,
        }
    }

    pub(crate) fn gone(&self) -> bool {
        let token = self.window.process_instance.as_deref().unwrap_or_default();
        liveness(self.pid, token) == Liveness::Gone
    }

    pub(crate) fn wait_gone(&self) -> bool {
        until(|| self.gone())
    }

    pub(crate) fn answer_prompt(&self) {
        let status = Command::new("kill")
            .args(["-USR1", &self.pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(status.success(), "the prompt answer reached the app");
    }
}

pub(crate) fn until(mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Everything one test starts. Dropping it ends every process, also when the
/// test failed half way.
pub(crate) struct Scene {
    launched: Launched,
    keyboard: Option<Child>,
    scratch: Vec<PathBuf>,
    pub(crate) bus: SharedBus,
    unique: String,
}

impl Scene {
    pub(crate) fn start(label: &str) -> Self {
        require_isolated_hyprland();
        Self {
            launched: Launched::bare(),
            keyboard: None,
            scratch: Vec::new(),
            bus: SharedBus::default(),
            unique: nonce(label),
        }
    }

    /// A title no other window of the run has.
    pub(crate) fn title(&self, part: &str) -> String {
        format!("{}-{part}", self.unique)
    }

    /// Gives the isolated seat a keyboard, which a toolkit needs before it
    /// reports any of its windows focused. The virtual keyboard lives as long
    /// as the helper that holds it.
    pub(crate) fn with_keyboard(&mut self) {
        let helper = Command::new("wtype")
            .args(["-M", "shift", "-s", "600000", "-m", "shift"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("wtype is installed: it gives the isolated seat a keyboard");
        self.keyboard = Some(helper);
        std::thread::sleep(Duration::from_millis(500));
    }

    pub(crate) fn window(&mut self, title: &str, extra: &[&str]) -> Shown {
        let mut args = vec![FIXTURE, title];
        args.extend_from_slice(extra);
        let pid = self.launched.spawn("python3", &args);
        self.shown(pid, title)
    }

    /// Gives the isolated seat a keyboard that sends nothing and holds no key
    /// or modifier, so a window can be focused with a keyboard present while
    /// the typing under test is the only input.
    pub(crate) fn with_idle_keyboard(&mut self) {
        let helper = Command::new("wtype")
            .args(["-s", "600000", "a"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("wtype is installed: it gives the isolated seat a keyboard");
        self.keyboard = Some(helper);
        std::thread::sleep(Duration::from_millis(500));
    }

    /// A path inside a scratch directory the scene removes when it ends.
    pub(crate) fn scratch_file(&mut self, name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(nonce("keys"));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let file = dir.join(name);
        self.scratch.push(dir);
        file
    }

    /// The key-logging text entry fixture: `args` are its extra flags.
    pub(crate) fn key_window(&mut self, title: &str, log: &str, args: &[&str]) -> Shown {
        let mut all = vec![KEY_FIXTURE, title, log];
        all.extend_from_slice(args);
        let pid = self.launched.spawn("python3", &all);
        self.shown(pid, title)
    }

    /// A window of another program: the same fixture under another file name,
    /// so none of its process names relates to the first program's app id.
    pub(crate) fn other_program_window(&mut self, title: &str, app_id: &str) -> Shown {
        let dir = std::env::temp_dir().join(nonce("other-program"));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let script = dir.join("other_program.py");
        std::fs::copy(FIXTURE, &script).expect("the fixture copies");
        self.scratch.push(dir);
        let script = script.to_string_lossy().into_owned();
        let pid = self
            .launched
            .spawn("python3", &[&script, title, "--app-id", app_id]);
        self.shown(pid, title)
    }

    /// A fixture run as a script named `program`, so its process names
    /// are that word and the interpreter's, whatever the fixture is called.
    pub(super) fn program_window(
        &mut self,
        fixture: &str,
        program: &str,
        title: &str,
        extra: &[&str],
    ) -> Shown {
        let dir = std::env::temp_dir().join(nonce(program));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let script = dir.join(program);
        std::fs::copy(fixture, &script).expect("the fixture copies");
        self.scratch.push(dir);
        let script = script.to_string_lossy().into_owned();
        let mut args = vec![script.as_str(), title];
        args.extend_from_slice(extra);
        let pid = self.launched.spawn("python3", &args);
        self.shown(pid, title)
    }

    pub(super) fn plain_fixture() -> &'static str {
        FIXTURE
    }

    pub(crate) fn shown(&self, pid: u32, title: &str) -> Shown {
        let started = Instant::now();
        let everything = WindowFilter {
            focused_only: false,
            app: None,
        };
        while started.elapsed() < SETTLE {
            let found = list_windows(&self.bus, &everything, deadline(10_000))
                .unwrap_or_default()
                .into_iter()
                .find(|window| window.pid.get() == pid && window.title == title);
            if let Some(window) = found
                && clients().iter().any(|client| client.title == title)
            {
                return Shown { pid, window };
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("pid {pid} never showed a window titled {title}");
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        if let Some(mut keyboard) = self.keyboard.take() {
            let _ = keyboard.kill();
            let _ = keyboard.wait();
        }
        self.launched.stop();
        for dir in self.scratch.drain(..) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
