/// The input mechanism that carried a keyboard delivery, so the step the
/// caller reads names what actually sent the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryChannel {
    Xtest,
    RemoteDesktop,
    VirtualKeyboard,
}

impl DeliveryChannel {
    /// The step label for `operation`: `type_text`, or a key press for any
    /// other value.
    pub(crate) fn step_label(self, operation: &str) -> &'static str {
        match (self, operation) {
            (Self::Xtest, "type_text") => "XTEST.type_text",
            (Self::Xtest, _) => "XTEST.press_key",
            (Self::RemoteDesktop, "type_text") => "RemoteDesktop.type_text",
            (Self::RemoteDesktop, _) => "RemoteDesktop.press_key",
            (Self::VirtualKeyboard, "type_text") => "VirtualKeyboard.type_text",
            (Self::VirtualKeyboard, _) => "VirtualKeyboard.press_key",
        }
    }
}

#[cfg(test)]
#[path = "delivery_channel_tests.rs"]
mod tests;
