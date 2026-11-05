use crate::{
    AppError, TextSelection,
    actions::action::Action,
    adapter::PlatformAdapter,
    commands::{
        helpers::{RefArgs, execute_ref_action_with_context},
        point_resolve::require_pointer_capability,
    },
    context::CommandContext,
};
use serde_json::Value;

pub struct SelectArgs {
    pub ref_id: String,
    pub value: Option<String>,
    pub selection: Option<TextSelection>,
    pub timeout_ms: Option<u64>,
}

pub fn execute(
    args: SelectArgs,
    adapter: &dyn PlatformAdapter,
    context: &CommandContext,
) -> Result<Value, AppError> {
    let action = match (args.value, args.selection) {
        (Some(value), None) => Action::Select(value),
        (None, Some(selection)) => {
            selection.validate()?;
            Action::SelectText(selection)
        }
        _ => {
            return Err(AppError::invalid_input(
                "Specify an option value or text selection",
            ));
        }
    };
    let needs_pointer = matches!(action, Action::Select(_));
    let request = context.request_base(action);
    if needs_pointer && request.policy.is_headed() {
        require_pointer_capability(adapter)?;
    }
    execute_ref_action_with_context(
        RefArgs {
            ref_id: args.ref_id,
            timeout_ms: args.timeout_ms,
        },
        adapter,
        request,
        context,
    )
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "select_text_tests.rs"]
mod text_tests;
