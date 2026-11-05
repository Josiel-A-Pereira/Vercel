use agent_desktop_core::{
    ActionResult, AdapterError, DeliverySemantics, ErrorCode, ResolvedTextSelection, Role,
    TextSelection,
};
use serde_json::json;

mod verification;

use self::verification::VerificationContext;
use super::context::Target;
use super::select_text_focus::{self, GlobalFocus};
use super::write::mutation::{WriteKind, WriteReply};
use super::write::write_call::{Call, invoke};
use crate::system::within;
use crate::tree::base_role;
use crate::tree::capabilities::text_selection::{TextState, read_text_state};
use crate::tree::vocabulary::role_numbers::PASSWORD_TEXT;
use crate::tree::{RawNode, ReadPurpose, read_node_for};

const TEXT_INTERFACE_NAME: &str = "org.a11y.atspi.Text";

fn refusal(code: ErrorCode, message: &'static str) -> AdapterError {
    AdapterError::new(code, message).with_disposition(DeliverySemantics::not_delivered())
}

fn ensure_safe_role(role: Option<u32>) -> Result<(), AdapterError> {
    let Some(role) = role else {
        return Err(refusal(
            ErrorCode::ActionNotSupported,
            "The target role could not be confirmed as non-secure",
        ));
    };
    if role == PASSWORD_TEXT {
        return Err(refusal(
            ErrorCode::ActionNotSupported,
            "Password fields cannot be selected through text readback",
        ));
    }
    if base_role(role) == Role::Unknown {
        return Err(refusal(
            ErrorCode::ActionNotSupported,
            "The target role is unknown, so text security cannot be confirmed",
        ));
    }
    Ok(())
}

fn ensure_text_target(node: &RawNode) -> Result<(), AdapterError> {
    ensure_safe_role(node.traits.role.present().copied())?;
    if node.traits.states.present().is_none() {
        return Err(refusal(
            ErrorCode::ActionNotSupported,
            "The target state could not be confirmed, so text security is unknown",
        ));
    }
    let Some(interfaces) = node.traits.interfaces.present() else {
        return Err(refusal(
            ErrorCode::ActionNotSupported,
            "The target's text interface could not be confirmed",
        ));
    };
    if !interfaces.iter().any(|name| name == TEXT_INTERFACE_NAME) {
        return Err(refusal(
            ErrorCode::ActionNotSupported,
            "The target does not expose the AT-SPI Text interface",
        ));
    }
    Ok(())
}

fn ensure_focus_available(focus: Option<&GlobalFocus>) -> Result<(), AdapterError> {
    if focus.is_some() {
        return Ok(());
    }
    Err(refusal(
        ErrorCode::ActionFailed,
        "Global focus could not be observed, so text selection was not sent",
    )
    .with_details(json!({
        "verification_scope": "text_selection",
        "kind": "global_focus_unknown",
    })))
}

fn send<B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType>(
    target: &Target<'_>,
    method: &'static str,
    body: &B,
) -> Result<WriteReply, AdapterError> {
    invoke(
        target,
        Call {
            interface: TEXT_INTERFACE_NAME,
            method,
            body,
            kind: WriteKind::SetSelection,
        },
    )
}

fn apply(
    target: &Target<'_>,
    before: &TextState,
    selection: &ResolvedTextSelection,
) -> Result<Option<&'static str>, AdapterError> {
    let start = i32::try_from(selection.range.start).map_err(|_| {
        refusal(
            ErrorCode::InvalidArgs,
            "The selection start is outside the AT-SPI range",
        )
    })?;
    let end = i32::try_from(selection.range.end).map_err(|_| {
        refusal(
            ErrorCode::InvalidArgs,
            "The selection end is outside the AT-SPI range",
        )
    })?;
    if selection.range.is_empty() {
        let mut changed = false;
        for index in (0..before.selections.len()).rev() {
            let index = i32::try_from(index).map_err(|_| {
                refusal(
                    ErrorCode::InvalidArgs,
                    "The provider selection count is invalid",
                )
            })?;
            match send(target, "RemoveSelection", &(index,)) {
                Ok(WriteReply::Done) => changed = true,
                Ok(WriteReply::Absent) if changed => {
                    return Err(AdapterError::new(
                        ErrorCode::ActionFailed,
                        "The provider stopped clearing prior selections",
                    )
                    .with_disposition(DeliverySemantics::uncertain()));
                }
                Ok(WriteReply::Absent) => return Ok(None),
                Err(error) if changed => {
                    return Err(error.with_disposition(DeliverySemantics::uncertain()));
                }
                Err(error) => return Err(error),
            }
        }
        return match send(target, "SetCaretOffset", &(start,)) {
            Ok(WriteReply::Done) => Ok(Some("Text.SetCaretOffset")),
            Ok(WriteReply::Absent) if changed => Err(AdapterError::new(
                ErrorCode::ActionFailed,
                "The provider refused the requested caret",
            )
            .with_disposition(DeliverySemantics::uncertain())),
            Ok(WriteReply::Absent) => Ok(None),
            Err(error) if changed => Err(error.with_disposition(DeliverySemantics::uncertain())),
            Err(error) => Err(error),
        };
    }
    let (method, reply) = if before.selections.is_empty() {
        ("AddSelection", send(target, "AddSelection", &(start, end))?)
    } else {
        (
            "SetSelection",
            send(target, "SetSelection", &(0i32, start, end))?,
        )
    };
    if reply == WriteReply::Absent {
        return Ok(None);
    }
    for index in (1..before.selections.len()).rev() {
        let index = i32::try_from(index).map_err(|_| {
            refusal(
                ErrorCode::InvalidArgs,
                "The provider selection count is invalid",
            )
        })?;
        match send(target, "RemoveSelection", &(index,)) {
            Ok(WriteReply::Done) => {}
            Ok(WriteReply::Absent) => {
                return Err(AdapterError::new(
                    ErrorCode::ActionFailed,
                    "The provider kept an extra text selection",
                )
                .with_disposition(DeliverySemantics::uncertain()));
            }
            Err(error) => return Err(error.with_disposition(DeliverySemantics::uncertain())),
        }
    }
    Ok(Some(if method == "AddSelection" {
        "Text.AddSelection"
    } else {
        "Text.SetSelection"
    }))
}

pub(crate) fn select_text(
    target: &Target<'_>,
    selector: &TextSelection,
) -> Result<ActionResult, AdapterError> {
    selector.validate()?;
    let connection = target.bus.get(target.deadline)?;
    let (node, _) = within(
        target.deadline,
        read_node_for(connection.inner(), target.element, ReadPurpose::Frame),
    )?;
    ensure_text_target(&node)?;
    let before_focus = select_text_focus::capture(target.bus, target.deadline)
        .map_err(|error| error.with_disposition(DeliverySemantics::not_delivered()))?;
    ensure_focus_available(before_focus.as_ref())?;
    let before =
        read_text_state(target.bus, target.element, target.deadline)?.ok_or_else(|| {
            refusal(
                ErrorCode::ActionNotSupported,
                "The provider text state is incomplete",
            )
        })?;
    let selection = selector.resolve(&before.text)?;
    let verification = VerificationContext {
        target,
        before: &before,
        selection: &selection,
        before_focus: before_focus.as_ref(),
    };
    if verification.preflight_satisfied() {
        verification.wait_without_delivery()?;
        return Ok(verification.satisfied_without_delivery());
    }
    let method = match apply(target, &before, &selection) {
        Ok(method) => method,
        Err(error) if error.disposition != DeliverySemantics::not_delivered() => {
            let state = read_text_state(target.bus, target.element, target.deadline)
                .ok()
                .flatten();
            return Err(verification.annotate_error(
                error.with_disposition(DeliverySemantics::uncertain()),
                state.as_ref(),
            ));
        }
        Err(error) => return Err(verification.annotate_error(error, None)),
    };
    let Some(method) = method else {
        let error = refusal(
            ErrorCode::ActionNotSupported,
            "The AT-SPI provider refused text selection",
        );
        return Err(verification.annotate_error(error, None));
    };
    verification.wait()?;
    Ok(verification.success(method))
}

#[cfg(test)]
#[path = "select_text_tests.rs"]
mod tests;
