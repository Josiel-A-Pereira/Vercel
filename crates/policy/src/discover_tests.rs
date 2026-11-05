use crate::discover::{self, Origin};
use crate::tests::write_policy;

#[cfg(unix)]
#[test]
fn world_writable_policy_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("chmod tempdir");
    let file = write_policy(
        dir.path(),
        "loose.dogwood",
        "permit (principal, action, resource);",
    );
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).expect("chmod");
    let found = discover::discover(&[file], &[], None);
    assert!(
        matches!(
            found.failures.as_slice(),
            [crate::PolicyError::Insecure { .. }]
        ),
        "world-writable policy must be refused: {:?}",
        found.failures
    );
}

fn private_policy_dir(text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod tempdir");
    }
    write_policy(dir.path(), "layer.dogwood", text);
    dir
}

#[test]
fn the_environment_and_the_flags_each_add_a_layer() {
    let env = private_policy_dir("// env");
    let cli = private_policy_dir("// cli");
    let found = discover::discover(&[], &[cli.path().to_path_buf()], Some(env.path()));
    let origins: Vec<_> = found
        .layers
        .iter()
        .map(|layer| layer.origin)
        .filter(|origin| *origin != Origin::Home)
        .collect();
    assert_eq!(origins, [Origin::Env, Origin::Cli]);
    assert!(found.failures.is_empty(), "{:?}", found.failures);
}

#[test]
fn a_broken_layer_is_reported_apart_and_never_hides_a_healthy_one() {
    let empty = tempfile::tempdir().expect("tempdir");
    let cli = private_policy_dir("// cli");
    let found = discover::discover(&[], &[cli.path().to_path_buf()], Some(empty.path()));
    assert!(
        found.layers.iter().any(|layer| layer.origin == Origin::Cli),
        "the healthy flag layer still loads: {:?}",
        found.layers
    );
    assert!(
        matches!(found.failures.as_slice(), [crate::PolicyError::Load { .. }]),
        "the empty environment layer is a failure, not silence: {:?}",
        found.failures
    );
}

#[test]
fn cli_dir_discovery_sorts_and_filters() {
    let dir = tempfile::tempdir().expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
            .expect("chmod tempdir");
    }
    write_policy(dir.path(), "b.dogwood", "// b");
    write_policy(dir.path(), "a.dogwood", "// a");
    write_policy(dir.path(), "ignored.txt", "// not a policy");
    let found = discover::discover(&[], &[dir.path().to_path_buf()], None);
    let sources = found
        .layers
        .iter()
        .find(|layer| layer.origin == Origin::Cli)
        .expect("the flags form a layer");
    let names: Vec<_> = sources
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a.dogwood", "b.dogwood"]);
    assert_eq!(sources.origin, Origin::Cli);
}

/// An agent that points `HOME` or `AGENT_DESKTOP_HOME` at a state root of
/// its own still runs under the account's policies folder.
#[test]
fn the_account_folder_applies_wherever_the_state_root_points() {
    let account = private_policy_dir(
        r#"@id("owner-rule")
forbid (principal, action, resource == Desktop::App::"Vault");"#,
    );
    let found = discover::discover_from(Some(account.path()), &[], &[], None);
    let owner = found
        .layers
        .iter()
        .find(|layer| layer.origin == Origin::Account)
        .unwrap_or_else(|| panic!("the account layer is missing: {:?}", found.layers));
    assert_eq!(owner.root, account.path());
    assert!(found.failures.is_empty(), "{:?}", found.failures);
}

#[cfg(unix)]
#[test]
fn one_folder_reached_two_ways_is_one_folder() {
    let dir = private_policy_dir("// layer");
    let alias = dir.path().with_extension("alias");
    std::os::unix::fs::symlink(dir.path(), &alias).expect("symlink");
    assert!(discover::same_folder(dir.path(), &alias));
    assert!(!discover::same_folder(dir.path(), &std::env::temp_dir()));
    let _ = std::fs::remove_file(&alias);
}

/// The owner's history lives under the account's own home, so relocating
/// the state root cannot hand the owner's rules a fresh, empty history.
#[test]
fn the_account_layer_keeps_its_history_under_the_account_home() {
    let account = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        discover::state_dir_for(Origin::Account, Some(account.path())).expect("state dir"),
        account.path().join(".agent-desktop").join("policy-state")
    );
    let state_root_store = discover::engine_state_dir().expect("state root store");
    for origin in [Origin::Home, Origin::Env, Origin::Cli] {
        assert_eq!(
            discover::state_dir_for(origin, Some(account.path())).expect("state dir"),
            state_root_store,
            "{origin:?} keeps its history under the state root"
        );
    }
}
