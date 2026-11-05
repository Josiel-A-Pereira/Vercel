//! The modifier oracle of the live key tests: what the toolkit of the focused
//! window believes is held. A fresh wtype keyboard sends its own modifier
//! state with every key, so its letter cannot show a modifier another
//! keyboard left behind.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::vk_live_rig::{KeyApp, until_for};

impl KeyApp {
    /// The modifiers the fixture's toolkit holds after its last change, or `-`.
    pub(super) fn held_modifiers(&self) -> String {
        self.lines()
            .iter()
            .rev()
            .find_map(|line| line.strip_prefix("MODS\t").map(str::to_owned))
            .unwrap_or_else(|| "-".to_owned())
    }

    /// Whether the toolkit settles on holding no modifier.
    pub(super) fn modifiers_released(&self) -> bool {
        until_for(Duration::from_secs(3), || self.held_modifiers() == "-")
    }
}

/// A keyboard of another process that presses a modifier and keeps it down
/// until the guard drops: the negative control the oracle must see.
pub(super) struct Holder(Child);

impl Holder {
    pub(super) fn modifier(name: &str) -> Self {
        Self::spawn(&["-M", name, "-s", "600000"])
    }

    /// A physical key held for `millis`, then released by the same keyboard.
    pub(super) fn key_for(name: &str, millis: u32) -> Self {
        Self::spawn(&["-P", name, "-s", &millis.to_string(), "-p", name])
    }

    fn spawn(args: &[&str]) -> Self {
        let child = Command::new("wtype")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("wtype is installed");
        Self(child)
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
