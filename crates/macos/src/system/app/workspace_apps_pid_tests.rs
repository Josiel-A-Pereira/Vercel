use super::*;

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(1)
}

fn snapshot_with(application_pid: i32, frontmost_pid: i32) -> String {
    format!(
        r#"{{
            "applications":[
                {{"name":"Mail","pid":{application_pid},"launch_time":100.25,"activation_policy":"regular"}}
            ],
            "frontmost_pid":{frontmost_pid},
            "frontmost_launch_time":null
        }}"#
    )
}

#[test]
fn owner_snapshot_skips_non_positive_application_pid() {
    for pid in [0, -1] {
        let bytes = snapshot_with(pid, 0);
        let snapshot = window_owner_snapshot_from_json(bytes.as_bytes(), deadline()).unwrap();
        assert!(snapshot.eligible_pids().is_empty());
    }
}

#[test]
fn app_listing_skips_non_positive_application_pid() {
    for pid in [0, -1] {
        let bytes = snapshot_with(pid, 0);
        let apps = apps_from_json(bytes.as_bytes(), deadline()).unwrap();
        assert!(apps.is_empty());
    }
}

#[test]
fn app_listing_omits_an_application_whose_process_no_longer_exists() {
    let bytes = br#"{
        "applications":[
            {"name":"Mail","pid":10,"launch_time":100.25,"activation_policy":"regular"},
            {"name":"Preview","pid":11,"launch_time":101.5,"activation_policy":"regular"}
        ],
        "frontmost_pid":10,
        "frontmost_launch_time":100.25
    }"#;
    let apps = apps_from_json_with(
        bytes,
        deadline(),
        |_| true,
        true,
        |pid| Ok((pid != 11).then(|| format!("instance-{pid}"))),
    )
    .unwrap();

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "Mail");
}

#[test]
fn scoped_lookup_of_an_exited_application_finds_nothing_instead_of_failing() {
    let bytes = br#"{
        "applications":[
            {"name":"Preview","pid":11,"launch_time":101.5,"activation_policy":"regular"}
        ],
        "frontmost_pid":0,
        "frontmost_launch_time":null
    }"#;
    let apps = apps_from_json_with(bytes, deadline(), |_| true, false, |_| Ok(None)).unwrap();

    assert!(apps.is_empty());
}
