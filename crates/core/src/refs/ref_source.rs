use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RefSource {
    pub source_app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_window_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_window_bounds_hash: Option<u64>,
    #[serde(default, skip_serializing_if = "crate::SnapshotSurface::is_window")]
    pub source_surface: crate::SnapshotSurface,
    /// Whether the capture read the window with Chromium renderer accessibility
    /// forced, so resolution reads it the same way and counts the same evidence.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub renderer_accessibility_forced: bool,
}
