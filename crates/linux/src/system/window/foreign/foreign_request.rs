use serde_json::{Value, json};

use super::foreign_flags::ToplevelFlags;

/// A write the compositor's toplevel protocol can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::system) enum ToplevelRequest {
    Activate,
    Close,
    SetMaximized(bool),
    SetMinimized(bool),
}

impl ToplevelRequest {
    /// Whether the reported state is the one this request asks for.
    /// Fullscreen never counts as maximized.
    pub(in crate::system) fn holds(self, flags: &ToplevelFlags) -> bool {
        match self {
            Self::Activate => flags.activated,
            Self::Close => false,
            Self::SetMaximized(on) => flags.maximized == on,
            Self::SetMinimized(on) => flags.minimized == on,
        }
    }

    pub(in crate::system) fn requested(self) -> Value {
        match self {
            Self::Activate => json!({ "activated": true }),
            Self::Close => json!({ "closed": true }),
            Self::SetMaximized(on) => json!({ "maximized": on }),
            Self::SetMinimized(on) => json!({ "minimized": on }),
        }
    }
}
