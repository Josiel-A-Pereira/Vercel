use serde::{Deserialize, Serialize};

/// An application as installed, named the way the running app would be
/// listed once launched. `bundle_id` is the platform's stable identifier
/// (the macOS bundle identifier, the Linux desktop id) where one exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
}
