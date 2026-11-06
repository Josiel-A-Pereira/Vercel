use super::*;

#[test]
fn remembered_process_is_agent_scoped_and_requires_identity() {
    let _home = crate::refs::refs_test_support::HomeGuard::new();
    let first = CommandContext::default()
        .with_agent_id(Some("agent-a".into()))
        .unwrap();
    let second = CommandContext::default()
        .with_agent_id(Some("agent-b".into()))
        .unwrap();
    let process = ProcessIdentity::new(42, "instance-42");

    first.remember_process(Some(process.clone())).unwrap();

    assert_eq!(first.remembered_process().unwrap(), Some(process));
    assert_eq!(second.remembered_process().unwrap(), None);
    CommandContext::default()
        .remember_process(Some(ProcessIdentity::new(99, "unscoped")))
        .unwrap();
    assert_eq!(
        CommandContext::default().remembered_process().unwrap(),
        None
    );
    first.remember_process(None).unwrap();
    assert_eq!(first.remembered_process().unwrap(), None);
}
