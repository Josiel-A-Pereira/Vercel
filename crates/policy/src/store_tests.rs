use crate::discover::{Origin, PolicySources};
use crate::tests::{facts, write_policy};
use crate::{PolicyGate, Verdict};

const OBSERVE_BEFORE_ACT: &str = r#"@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);

@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    formerly within 10m Desktop::Action::"snapshot"::response{ input.app: context.input.app }
};
"#;

fn private_tempdir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod tempdir");
    }
    dir
}

#[test]
fn gates_open_side_by_side_on_one_store_decide_and_share_history() {
    let dir = private_tempdir();
    let sources = PolicySources::for_file(
        write_policy(dir.path(), "temporal.dogwood", OBSERVE_BEFORE_ACT),
        Origin::Cli,
    );
    let gate_root = dir.path().join("gate");
    let mut first_host = PolicyGate::open_at(&sources, &gate_root).expect("first host's gate");
    let mut second_host = PolicyGate::open_at(&sources, &gate_root)
        .expect("a second host opens its gate while the first is still alive");

    let snapshot = facts("snapshot", "Finder");
    assert_eq!(
        first_host.check(&snapshot).expect("verdict"),
        Verdict::Allowed
    );
    first_host.record(&snapshot, true).expect("record");

    assert_eq!(
        second_host
            .check(&facts("click", "Finder"))
            .expect("the second host decides while the first is alive"),
        Verdict::Allowed,
        "history recorded through one host's gate is visible to the other's"
    );
    assert_eq!(
        first_host
            .check(&facts("click", "Finder"))
            .expect("the store is handed back to the first host"),
        Verdict::Allowed
    );
}

#[test]
fn different_policy_sources_never_reinstall_over_each_others_history() {
    let dir = private_tempdir();
    let temporal = PolicySources::for_file(
        write_policy(dir.path(), "temporal.dogwood", OBSERVE_BEFORE_ACT),
        Origin::Home,
    );
    let other = PolicySources::for_file(
        write_policy(
            dir.path(),
            "other.dogwood",
            r#"@id("observe-only")
permit (principal, action in Desktop::Action::"observe", resource);
"#,
        ),
        Origin::Cli,
    );
    let state_root = dir.path().join("gate");

    let mut home_agent = PolicyGate::open_at(&temporal, &state_root).expect("home gate");
    let snapshot = facts("snapshot", "Finder");
    assert_eq!(
        home_agent.check(&snapshot).expect("verdict"),
        Verdict::Allowed
    );
    home_agent.record(&snapshot, true).expect("record");

    let mut flag_agent = PolicyGate::open_at(&other, &state_root).expect("flag gate");
    assert!(matches!(
        flag_agent
            .check(&facts("click", "Finder"))
            .expect("verdict"),
        Verdict::Denied { .. }
    ));

    assert_eq!(
        home_agent
            .check(&facts("click", "Finder"))
            .expect("verdict"),
        Verdict::Allowed,
        "another agent's policy sources must not replace this agent's installed set and history"
    );
}

fn checkpointed_offset(gate_root: &std::path::Path) -> u64 {
    std::fs::read_dir(gate_root)
        .expect("gate root")
        .filter_map(|entry| {
            std::fs::read_to_string(entry.ok()?.path().join("checkpoint.offset")).ok()
        })
        .filter_map(|text| text.trim().parse().ok())
        .max()
        .unwrap_or(0)
}

/// A one-shot CLI process opens the store, appends two events and exits,
/// so the engine's own every-64-events checkpoint never fires. Closing the
/// store must checkpoint once enough events gathered across processes, or
/// the log and the replay every open pays grow without bound; and pruning
/// behind the checkpoint must not lose what temporal clauses still need.
#[test]
fn a_store_reopened_per_command_is_checkpointed_without_losing_history() {
    let dir = private_tempdir();
    let sources = PolicySources::for_file(
        write_policy(dir.path(), "temporal.dogwood", OBSERVE_BEFORE_ACT),
        Origin::Cli,
    );
    let gate_root = dir.path().join("gate");
    {
        let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("gate");
        let snapshot = facts("snapshot", "Finder");
        assert_eq!(gate.check(&snapshot).expect("verdict"), Verdict::Allowed);
        gate.record(&snapshot, true).expect("record");
    }
    for _ in 0..40 {
        let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("gate");
        let read = facts("list-displays", "system");
        assert_eq!(gate.check(&read).expect("verdict"), Verdict::Allowed);
        gate.record(&read, true).expect("record");
    }
    assert!(
        checkpointed_offset(&gate_root) >= 64,
        "82 events across 41 processes must have produced a checkpoint, got offset {}",
        checkpointed_offset(&gate_root)
    );
    let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("gate");
    assert_eq!(
        gate.check(&facts("click", "Finder")).expect("verdict"),
        Verdict::Allowed,
        "the snapshot recorded before the checkpoint still satisfies the temporal clause"
    );
}

/// The owner edits the policy file all the time. A policy the edit left
/// alone keeps the observations its temporal clause has gathered; only a
/// policy whose own text changed starts again with an empty history.
#[test]
fn editing_a_policy_keeps_the_history_of_policies_it_left_alone() {
    let dir = private_tempdir();
    let file = write_policy(dir.path(), "temporal.dogwood", OBSERVE_BEFORE_ACT);
    let sources = PolicySources::for_file(file, Origin::Home);
    let gate_root = dir.path().join("gate");
    {
        let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("gate");
        let snapshot = facts("snapshot", "Finder");
        assert_eq!(gate.check(&snapshot).expect("verdict"), Verdict::Allowed);
        gate.record(&snapshot, true).expect("record");
    }
    write_policy(
        dir.path(),
        "temporal.dogwood",
        &format!(
            "// edited by the owner\n{OBSERVE_BEFORE_ACT}\n@id(\"no-clipboard\")\nforbid (principal, action in Desktop::Action::\"clipboard\", resource);\n"
        ),
    );
    let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("gate");
    assert_eq!(
        gate.check(&facts("click", "Finder")).expect("verdict"),
        Verdict::Allowed,
        "a comment and a new policy leave observe-before-act and its history alone"
    );
    assert!(matches!(
        gate.check(&facts("clipboard-clear", "system"))
            .expect("verdict"),
        Verdict::Denied { .. }
    ));
    drop(gate);

    write_policy(
        dir.path(),
        "temporal.dogwood",
        &OBSERVE_BEFORE_ACT.replace("within 10m", "within 20m"),
    );
    let mut gate = PolicyGate::open_at(&sources, &gate_root).expect("gate");
    assert!(
        matches!(
            gate.check(&facts("click", "Finder")).expect("verdict"),
            Verdict::Denied { .. }
        ),
        "a policy whose own text changed starts with an empty history"
    );
}

/// A layer is a folder; a file added beside the others joins the layer's
/// existing history instead of starting a new store.
#[test]
fn adding_a_file_to_a_layer_keeps_its_history() {
    let dir = private_tempdir();
    let folder = dir.path().join("policies");
    std::fs::create_dir_all(&folder).expect("folder");
    let first = write_policy(&folder, "a.dogwood", OBSERVE_BEFORE_ACT);
    let gate_root = dir.path().join("gate");
    let before = PolicySources {
        files: vec![first.clone()],
        origin: Origin::Home,
        root: folder.clone(),
    };
    {
        let mut gate = PolicyGate::open_at(&before, &gate_root).expect("gate");
        let snapshot = facts("snapshot", "Finder");
        assert_eq!(gate.check(&snapshot).expect("verdict"), Verdict::Allowed);
        gate.record(&snapshot, true).expect("record");
    }
    let second = write_policy(
        &folder,
        "b.dogwood",
        "@id(\"no-clipboard\")\nforbid (principal, action in Desktop::Action::\"clipboard\", resource);",
    );
    let after = PolicySources {
        files: vec![first, second],
        origin: Origin::Home,
        root: folder,
    };
    let mut gate = PolicyGate::open_at(&after, &gate_root).expect("gate");
    assert_eq!(
        gate.check(&facts("click", "Finder")).expect("verdict"),
        Verdict::Allowed,
        "the snapshot recorded before b.dogwood existed still counts"
    );
}
