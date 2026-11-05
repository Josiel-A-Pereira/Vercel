use super::*;
use agent_desktop_core::PermissionReport;

fn select(args: Value) -> BatchCommand {
    BatchCommand {
        command: "select".into(),
        session: None,
        args,
    }
}

#[test]
fn select_batch_validates_the_selector_before_dispatch() {
    for args in [
        serde_json::json!({"ref_id":"@s8f3k2p9:e1", "value":"choice"}),
        serde_json::json!({"ref_id":"@s8f3k2p9:e1", "match":"ana", "nth":2}),
        serde_json::json!({"ref_id":"@s8f3k2p9:e1", "range":"2..5"}),
    ] {
        let command = parse_command(select(args)).unwrap();
        assert_eq!(command.name(), "select");
        assert!(crate::command_policy::preflight(&command, &PermissionReport::default()).is_ok());
    }
    let invalid = parse_command(select(
        serde_json::json!({"ref_id":"@s8f3k2p9:e1", "range":"5..2"}),
    ))
    .unwrap();
    assert_eq!(
        crate::command_policy::preflight(&invalid, &PermissionReport::default())
            .unwrap_err()
            .code(),
        "INVALID_ARGS"
    );
}

#[test]
fn batch_preparation_rejects_conflicting_select_arguments() {
    for selector in [
        serde_json::json!({"match":"a", "range":"0..1"}),
        serde_json::json!({"range":"0..1", "nth":1}),
        serde_json::json!({"match":"a", "nth":0}),
        serde_json::json!({"value":"choice", "match":"a"}),
        serde_json::json!({"value":"choice", "range":"0..1"}),
        serde_json::json!({"value":"choice", "nth":1}),
        serde_json::json!({}),
    ] {
        let mut selector = selector;
        selector["ref_id"] = serde_json::json!("@s8f3k2p9:e1");
        let commands = serde_json::json!([{"command":"select", "args":selector}]);
        let error = match preparation::prepare(
            &commands.to_string(),
            &PermissionReport::default(),
            &CommandContext::default(),
        ) {
            Ok(_) => panic!("invalid selection must fail preparation"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "INVALID_ARGS");
    }
}
