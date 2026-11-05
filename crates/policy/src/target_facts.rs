/// Where a command lands, as far as policies can see it. String fields use
/// `""` for "unknown"; the default is a command that lands in no
/// application (`Desktop::App::"system"`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetFacts {
    /// The receiving application's name, as the platform lists it running.
    pub app: String,
    /// The receiving application's stable identifier: the macOS bundle
    /// identifier or the Linux desktop id. It does not change with the
    /// system language or a renamed display name, so policies that must
    /// hold write it as `resource in Desktop::AppId::"<id>"`.
    pub app_id: String,
    /// The receiving window's title.
    pub window: String,
    /// The target element's accessibility role, when a ref carries one.
    pub role: String,
    /// The target element's accessible name, when a ref carries one.
    pub element: String,
    /// Whether the target element is a secure (password-style) field. The
    /// canonical role of such a field is still `textfield`; secure-ness is
    /// carried as the `secure` state on the ref.
    pub secure: bool,
}

impl TargetFacts {
    /// The resource entity id: the app name, or `system` when the command
    /// lands in no application (the clipboard, displays, the desktop).
    pub fn app_or_system(&self) -> &str {
        if self.app.is_empty() {
            "system"
        } else {
            &self.app
        }
    }
}
