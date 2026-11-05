use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use agent_desktop_core::{
    Action, ActionRequest, ActionResult, AdapterError, KeyCombo, Modifier, NativeHandle,
};

use crate::system::SharedBus;
use crate::system::focus::focus_window::focus_window;
use crate::system::window::foreign_live_rig::{Scene, Shown, deadline, lease};
use crate::system::window::window_id;
use crate::tree::{AtspiElement, list_children, read_node};

/// One line of the fixture's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Event {
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) mods: String,
}

/// A key-logging text entry the test launched and focused.
pub(super) struct KeyApp {
    pub(super) shown: Shown,
    pub(super) log: PathBuf,
    entry: NativeHandle,
}

fn find_entry(bus: &SharedBus, frame: AtspiElement) -> Option<AtspiElement> {
    let budget = deadline(10_000);
    let connection = bus.get(budget).ok()?;
    let mut root = frame.clone();
    root.set_ancestry(crate::tree::element::ancestry::of_root(&root));
    let mut queue = vec![root];
    let mut index = 0;
    while index < queue.len() && index < 500 {
        let element = queue[index].clone();
        index += 1;
        let node = crate::system::within(budget, read_node(connection.inner(), &element)).ok()?;
        if node
            .properties
            .name
            .present()
            .is_some_and(|name| name == "keys-entry")
        {
            return Some(element);
        }
        let children =
            crate::system::within(budget, list_children(connection.inner(), &element)).ok()?;
        let role = node.traits.role.present().copied();
        queue.extend(children.into_iter().map(|(bus, path)| {
            let mut child = AtspiElement::new(bus, path, frame.pid());
            child.set_ancestry(crate::tree::element::ancestry::child(&element, role));
            child
        }));
    }
    None
}

pub(super) fn until_for(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

pub(super) fn combo(key: &str, modifiers: Vec<Modifier>) -> KeyCombo {
    KeyCombo {
        key: key.to_owned(),
        modifiers,
    }
}

impl KeyApp {
    /// Launches the fixture, focuses it through the compositor and waits for
    /// the toolkit to report its own keyboard focus. The seat must already
    /// have a keyboard.
    pub(super) fn open(scene: &mut Scene, name: &str, args: &[&str]) -> Self {
        let log = scene.scratch_file(&format!("{name}.log"));
        let title = scene.title(name);
        let shown = scene.key_window(&title, &log.to_string_lossy(), args);
        let app = Self::attach(scene, shown, log);
        assert!(app.focus(scene), "{name} never took keyboard focus");
        app
    }

    pub(super) fn attach(scene: &Scene, shown: Shown, log: PathBuf) -> Self {
        let (bus, path) = window_id::decode(&shown.window.id).expect("a window id");
        let frame = AtspiElement::new(bus, path, shown.pid);
        let entry = find_entry(&scene.bus, frame).expect("the entry is in the tree");
        Self {
            shown,
            log,
            entry: entry.into_native_handle(),
        }
    }

    /// Gives the window the compositor focus and waits for the toolkit to
    /// report that it holds it.
    pub(super) fn focus(&self, scene: &Scene) -> bool {
        focus_window(&scene.bus, &self.shown.window, &lease()).expect("the window takes focus");
        until_for(Duration::from_secs(8), || {
            self.lines().iter().rev().find_map(|line| {
                line.starts_with("FOCUS-")
                    .then(|| line.starts_with("FOCUS-IN"))
            }) == Some(true)
        })
    }

    pub(super) fn lines(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn events(&self) -> Vec<Event> {
        self.lines()
            .iter()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                matches!(fields.first(), Some(&"PRESS" | &"RELEASE")).then(|| Event {
                    kind: fields[0].to_owned(),
                    name: fields[1].to_owned(),
                    mods: fields[3].to_owned(),
                })
            })
            .collect()
    }

    /// The entry text after the last change, as the fixture saw it.
    pub(super) fn text(&self) -> String {
        self.lines()
            .iter()
            .rev()
            .find_map(|line| line.strip_prefix("TEXT\t"))
            .map(|json| serde_json::from_str(json).expect("the fixture writes JSON"))
            .unwrap_or_default()
    }

    pub(super) fn selections(&self) -> Vec<String> {
        self.lines()
            .iter()
            .filter_map(|line| line.strip_prefix("SELECT\t").map(str::to_owned))
            .collect()
    }

    pub(super) fn wait_text(&self, expected: &str) -> bool {
        until_for(Duration::from_secs(5), || self.text() == expected)
    }

    /// Whether every press in the log has its release, in order.
    pub(super) fn balanced(&self) -> bool {
        let mut down: Vec<String> = Vec::new();
        for event in self.events() {
            if event.kind == "PRESS" {
                down.push(event.name);
            } else if let Some(at) = down.iter().position(|name| *name == event.name) {
                down.remove(at);
            }
        }
        down.is_empty()
    }

    pub(super) fn act(&self, scene: &Scene, action: Action) -> Result<ActionResult, AdapterError> {
        crate::actions::execute_action(
            &scene.bus,
            &self.entry,
            ActionRequest::headed(action),
            &lease(),
        )
    }

    /// The physical leg alone, without the semantic fallback `execute_action`
    /// takes when the physical leg refuses a request it cannot express.
    pub(super) fn physical_only(
        &self,
        scene: &Scene,
        action: Action,
    ) -> Result<ActionResult, AdapterError> {
        let element = crate::tree::atspi_element(&self.entry).expect("an element");
        let request = ActionRequest::headed(action.clone());
        crate::actions::physical_target::deliver(
            &scene.bus,
            &action,
            Some(element),
            &request,
            &lease(),
        )
    }

    pub(super) fn type_text(
        &self,
        scene: &Scene,
        text: &str,
    ) -> Result<ActionResult, AdapterError> {
        self.act(scene, Action::TypeText(text.to_owned()))
    }

    pub(super) fn press(
        &self,
        scene: &Scene,
        key: &str,
        modifiers: Vec<Modifier>,
    ) -> Result<ActionResult, AdapterError> {
        self.act(scene, Action::PressKey(combo(key, modifiers)))
    }
}

pub(super) fn step_label(result: &ActionResult) -> String {
    result
        .steps
        .first()
        .map(|step| step.label().to_owned())
        .unwrap_or_default()
}

/// The names of the compositor's virtual keyboards, read apart from the
/// product.
pub(super) fn virtual_keyboards() -> Vec<String> {
    let output = Command::new("hyprctl")
        .args(["devices", "-j"])
        .output()
        .expect("hyprctl runs");
    let devices: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("hyprctl devices -j is JSON");
    devices["keyboards"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|keyboard| keyboard["name"].as_str())
                .filter(|name| name.starts_with("hl-virtual-keyboard"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the virtual keyboards are exactly the ones the scene started.
pub(super) fn only_scene_keyboards(count: usize) -> bool {
    until_for(Duration::from_secs(3), || {
        virtual_keyboards().len() == count
    })
}

/// A fresh keyboard of another process types a plain `a`; it arrives
/// lowercase with no modifier when nothing was left behind.
pub(super) fn next_keyboard_types_lowercase(app: &KeyApp) -> bool {
    let before = app.events().len();
    let status = Command::new("wtype")
        .arg("a")
        .status()
        .expect("wtype is installed");
    assert!(status.success());
    until_for(Duration::from_secs(5), || app.events().len() >= before + 2)
        && app.events()[before..]
            .iter()
            .all(|event| event.name == "a" && event.mods == "-")
}

/// A scene with an idle keyboard in the seat, and how many virtual keyboards
/// that makes the baseline.
pub(super) fn scene_with_keyboard(label: &str) -> (Scene, usize) {
    let mut scene = Scene::start(label);
    scene.with_idle_keyboard();
    let helpers = virtual_keyboards().len();
    (scene, helpers)
}

/// What every scenario leaves behind once delivery ended: no press without
/// its release, no virtual keyboard beyond the scene's own, and a fresh
/// keyboard that types a lowercase letter, and a toolkit that holds no modifier.
pub(super) fn assert_no_residue(app: &KeyApp, helpers: usize) {
    assert!(app.balanced(), "{:?}", app.lines());
    assert!(
        only_scene_keyboards(helpers),
        "leftover keyboards: {:?}",
        virtual_keyboards()
    );
    assert!(
        next_keyboard_types_lowercase(app),
        "residue: {:?}",
        app.lines()
    );
    assert!(
        app.modifiers_released(),
        "the toolkit still holds {}: {:?}",
        app.held_modifiers(),
        app.lines()
    );
}

/// The built command line, run in its own home so its ref store is private.
pub(super) struct Cli {
    home: PathBuf,
}

impl Cli {
    pub(super) fn in_scene(scene: &mut Scene) -> Self {
        let home = scene.scratch_file("cli-home");
        std::fs::create_dir_all(&home).expect("a private home");
        Self { home }
    }

    fn binary() -> PathBuf {
        let exe = std::env::current_exe().expect("the test binary path");
        let debug = exe
            .parent()
            .and_then(std::path::Path::parent)
            .expect("target/debug");
        let binary = debug.join("agent-desktop");
        assert!(
            binary.exists(),
            "build the command line first: cargo build -p agent-desktop ({binary:?})"
        );
        binary
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(Self::binary());
        command.args(args).env("HOME", &self.home);
        command
    }

    /// The qualified ref of the fixture's entry, from a real snapshot.
    pub(super) fn entry_ref(&self, window_id: &str) -> String {
        let output = self
            .command(&["snapshot", "--window-id", window_id, "-i", "--compact"])
            .output()
            .expect("the command line runs");
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("the snapshot is JSON");
        ref_of(&json).expect("the snapshot has the entry")
    }

    pub(super) fn spawn_type(&self, reference: &str, text: &str) -> Child {
        self.command(&[
            "type",
            reference,
            text,
            "--headed",
            "--timeout-ms",
            "120000",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the command line starts")
    }
}

fn ref_of(node: &serde_json::Value) -> Option<String> {
    match node {
        serde_json::Value::Object(map) => {
            if map.get("name").and_then(|name| name.as_str()) == Some("keys-entry")
                && let Some(reference) = map.get("ref_id").and_then(|id| id.as_str())
            {
                return Some(reference.to_owned());
            }
            map.values().find_map(ref_of)
        }
        serde_json::Value::Array(all) => all.iter().find_map(ref_of),
        _ => None,
    }
}

impl KeyApp {
    pub(super) fn window_id(&self) -> &str {
        &self.shown.window.id
    }

    /// Launches and attaches the fixture without focusing it or waiting for
    /// its keyboard focus: the seat may have no keyboard yet.
    pub(super) fn open_unfocused(scene: &mut Scene, name: &str) -> Self {
        let log = scene.scratch_file(&format!("{name}.log"));
        let title = scene.title(name);
        let shown = scene.key_window(&title, &log.to_string_lossy(), &[]);
        Self::attach(scene, shown, log)
    }

    pub(super) fn raise(&self, scene: &Scene) {
        focus_window(&scene.bus, &self.shown.window, &lease()).expect("the window takes focus");
    }
}
