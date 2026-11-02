//! What `status` reports about policies: every layer that applies to the
//! caller, its files, and whether it installed. Reads never refuse under a
//! broken layer, so without this an owner learns about a typo only when a
//! mutating command is refused.

use agent_desktop_core::context::CommandContext;
use agent_desktop_policy::PolicyGate;
use serde_json::{Value, json};

pub(crate) fn policy_status(context: &CommandContext) -> Value {
    let paths = context.policy_paths();
    let found = agent_desktop_policy::discover(&paths.files, &paths.dirs, paths.env_dir.as_deref());
    let layers: Vec<Value> = found
        .layers
        .iter()
        .map(|layer| {
            let mut entry = json!({
                "layer": layer.origin.describe(),
                "files": layer.files,
            });
            match PolicyGate::open(layer) {
                Ok(_) => entry["installed"] = json!(true),
                Err(error) => {
                    entry["installed"] = json!(false);
                    entry["error"] = json!(error.to_string());
                }
            }
            entry
        })
        .collect();
    let unreadable: Vec<String> = found.failures.iter().map(ToString::to_string).collect();
    json!({
        "configured": found.is_configured(),
        "layers": layers,
        "unreadable": unreadable,
    })
}
