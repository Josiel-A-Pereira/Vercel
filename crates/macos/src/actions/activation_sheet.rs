use crate::tree::{AXElement, surface_read};
use agent_desktop_core::Deadline;

const SHEET_ANCESTOR_WALK: usize = 12;

/// A sheet that encloses an `AXOpen` target, kept with the window that owns it.
/// The sheet's own attributes become unreadable once it closes, so its absence
/// is read from the owner's children instead.
pub(crate) struct OwnedSheet {
    sheet: AXElement,
    owner: AXElement,
}

impl OwnedSheet {
    pub(crate) fn capture(element: &AXElement, action: &str, deadline: Deadline) -> Option<Self> {
        if action != "AXOpen" {
            return None;
        }
        let end = crate::tree::locator_deadline::from_operation(deadline).ok()?;
        Self::capture_with(
            element,
            |current| surface_read::string(current, "AXRole", end).ok().flatten(),
            |current| {
                surface_read::element(current, "AXParent", end)
                    .ok()
                    .flatten()
            },
            |owner| surface_read::elements(owner, "AXChildren", end).ok(),
        )
    }

    fn capture_with(
        element: &AXElement,
        role: impl Fn(&AXElement) -> Option<String>,
        parent: impl Fn(&AXElement) -> Option<AXElement>,
        children: impl FnOnce(&AXElement) -> Option<Vec<AXElement>>,
    ) -> Option<Self> {
        let mut current = element.clone();
        for _ in 0..SHEET_ANCESTOR_WALK {
            if role(&current)? == "AXSheet" {
                let owner = parent(&current)?;
                if role(&owner)? != "AXWindow" {
                    return None;
                }
                let sheet = Self {
                    sheet: current,
                    owner,
                };
                return sheet
                    .present_with(children)
                    .filter(|present| *present)
                    .map(|_| sheet);
            }
            current = parent(&current)?;
        }
        None
    }

    pub(crate) fn closed(&self, deadline: Deadline) -> bool {
        let Ok(end) = crate::tree::locator_deadline::from_operation(deadline) else {
            return false;
        };
        self.closed_with(|owner| surface_read::elements(owner, "AXChildren", end).ok())
    }

    fn closed_with(&self, read: impl FnOnce(&AXElement) -> Option<Vec<AXElement>>) -> bool {
        self.present_with(read) == Some(false)
    }

    fn present_with(
        &self,
        read: impl FnOnce(&AXElement) -> Option<Vec<AXElement>>,
    ) -> Option<bool> {
        read(&self.owner).map(|children| {
            children
                .iter()
                .any(|child| crate::tree::same_element(child, &self.sheet))
        })
    }
}

#[cfg(test)]
#[path = "activation_sheet_tests.rs"]
mod tests;
