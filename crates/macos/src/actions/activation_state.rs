use crate::tree::AXElement;
use agent_desktop_core::Deadline;

pub(crate) struct ActivationState {
    pub(crate) focus: Option<crate::actions::activation_effect::FocusState>,
    pub(crate) sheet: Option<crate::actions::activation_sheet::OwnedSheet>,
}

impl ActivationState {
    pub(crate) fn capture(element: &AXElement, action: &str, deadline: Deadline) -> Self {
        Self {
            focus: crate::actions::activation_effect::focus_state(element, deadline),
            sheet: crate::actions::activation_sheet::OwnedSheet::capture(element, action, deadline),
        }
    }

    pub(crate) fn sheet_closed(&self, deadline: Deadline) -> bool {
        self.sheet
            .as_ref()
            .is_some_and(|sheet| sheet.closed(deadline))
    }
}
