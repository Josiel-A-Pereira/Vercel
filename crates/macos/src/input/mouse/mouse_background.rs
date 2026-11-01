use agent_desktop_core::{
    AdapterError, Deadline, DeliverySemantics, ErrorCode, MouseButton, MouseEvent, MouseEventKind,
    OperationReport, Point,
};

const PRESS: &str = "AXPress";
const PRESS_SEARCH_DEPTH: usize = 4;

/// Accessibility can only press, so anything richer than a plain click stays
/// with `--headed`.
fn is_plain_click(event: &MouseEvent) -> bool {
    matches!(event.button, MouseButton::Left)
        && event.modifiers.is_empty()
        && matches!(event.kind, MouseEventKind::Click { count: 1 })
}

fn nothing_to_press() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionNotSupported,
        "Nothing at that point in this app can be pressed",
    )
    .with_disposition(DeliverySemantics::not_delivered())
    .with_suggestion("Use a ref from snapshot, or --headed mouse-click for physical input")
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use crate::actions::ax_perform::{advertises_action, perform_observed_action};
    use crate::actions::chain::chain_delivery::DeliveryOutcome;
    use crate::tree::{AXElement, element_for_pid, properties::ax_ipc};

    pub(crate) fn background_click(
        pid: i32,
        event: MouseEvent,
        deadline: Deadline,
    ) -> Result<OperationReport, AdapterError> {
        crate::input::mouse::validate_point(&event.point)?;
        if !is_plain_click(&event) {
            return Err(AdapterError::new(
                ErrorCode::ActionNotSupported,
                "--app takes a single left click with no modifiers",
            )
            .with_disposition(DeliverySemantics::not_delivered())
            .with_suggestion("Use --headed mouse-click for physical input"));
        }
        if press_element_at(pid, &event.point, deadline)? {
            return Ok(OperationReport::background_click(&event));
        }
        Err(nothing_to_press())
    }

    fn press_element_at(pid: i32, point: &Point, deadline: Deadline) -> Result<bool, AdapterError> {
        let app = element_for_pid(pid);
        let (status, raw) =
            ax_ipc::element_at_position(&app, (point.x as f32, point.y as f32), deadline);
        if status != accessibility_sys::kAXErrorSuccess || raw.is_null() {
            return Ok(false);
        }
        let mut element = AXElement(raw);
        for _ in 0..PRESS_SEARCH_DEPTH {
            if advertises_action(&element, PRESS, deadline) == Some(true) {
                let outcome = perform_observed_action(&element, PRESS, deadline)?;
                return Ok(!matches!(outcome, DeliveryOutcome::NotDelivered));
            }
            match crate::actions::scroll_read::element(&element, "AXParent", deadline) {
                Ok(Some(parent)) => element = parent,
                _ => return Ok(false),
            }
        }
        Ok(false)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    pub(crate) fn background_click(
        _pid: i32,
        _event: MouseEvent,
        _deadline: Deadline,
    ) -> Result<OperationReport, AdapterError> {
        Err(AdapterError::not_supported("background_click"))
    }
}

pub(crate) use imp::background_click;

#[cfg(test)]
mod tests {
    use super::*;

    fn click(button: MouseButton, count: u32) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Click { count },
            point: Point { x: 5.0, y: 5.0 },
            button,
            modifiers: Vec::new(),
        }
    }

    #[test]
    fn only_a_plain_left_click_can_press_an_element() {
        assert!(is_plain_click(&click(MouseButton::Left, 1)));
        assert!(!is_plain_click(&click(MouseButton::Left, 2)));
        assert!(!is_plain_click(&click(MouseButton::Right, 1)));
        let mut held = click(MouseButton::Left, 1);
        held.modifiers.push(agent_desktop_core::Modifier::Shift);
        assert!(!is_plain_click(&held));
    }
}
