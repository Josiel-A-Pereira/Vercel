use super::*;
use agent_desktop_core::Rect;
use rustc_hash::FxHashSet;

const LAUNCH_DATE_TWELVE_DAYS_EARLY: f64 = 1_000_000.0;

struct NamedOwner {
    pid: i32,
    name: &'static str,
    launch_time: f64,
}

impl OwnerSnapshotView for NamedOwner {
    fn eligible_pids(&self) -> FxHashSet<i32> {
        FxHashSet::from_iter([self.pid])
    }

    fn generation(&self, pid: i32) -> Option<OwnerGeneration> {
        (pid == self.pid).then_some(OwnerGeneration::LaunchTime(self.launch_time))
    }

    fn frontmost_pid(&self) -> Option<i32> {
        None
    }

    fn same_generation(&self, _other: &Self) -> bool {
        true
    }

    fn owner_name(&self, pid: i32) -> Option<&str> {
        (pid == self.pid).then_some(self.name)
    }
}

fn current_pid() -> i32 {
    i32::try_from(std::process::id()).expect("test pid fits macOS pid_t")
}

fn live_token() -> String {
    process_identity::token_for_pid(current_pid())
        .unwrap()
        .expect("current process token")
}

fn misdated_owner(name: &'static str) -> NamedOwner {
    NamedOwner {
        pid: current_pid(),
        name,
        launch_time: LAUNCH_DATE_TWELVE_DAYS_EARLY,
    }
}

fn window(app_name: &str, process_instance: String) -> WindowRecord {
    WindowRecord {
        app_name: app_name.into(),
        pid: current_pid(),
        title: Some("Untitled".into()),
        window_number: 1,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
        visible: true,
        process_instance: Some(process_instance),
    }
}

fn phase(error: AdapterError) -> serde_json::Value {
    error.details.unwrap()["phase"].clone()
}

#[test]
fn live_owner_with_a_misdated_launch_is_accepted_when_window_server_names_the_same_app() {
    let records = vec![window("TextEdit", live_token())];

    validate_record_generations(&records, &misdated_owner("TextEdit")).unwrap();
}

#[test]
fn misdated_launch_is_rejected_when_window_server_names_a_different_app() {
    let records = vec![window("Reused Pid App", live_token())];

    let error = validate_record_generations(&records, &misdated_owner("Preview")).unwrap_err();

    assert_eq!(phase(error), "process_generation_changed");
}

#[test]
fn misdated_launch_is_rejected_when_one_window_names_a_different_app() {
    let records = vec![
        window("TextEdit", live_token()),
        WindowRecord {
            window_number: 2,
            ..window("Other", live_token())
        },
    ];

    let error = validate_record_generations(&records, &misdated_owner("TextEdit")).unwrap_err();

    assert_eq!(phase(error), "process_generation_changed");
}

#[test]
fn misdated_launch_is_rejected_when_the_captured_instance_is_no_longer_live() {
    let live = live_token();
    let (prefix, start) = live.rsplit_once(':').unwrap();
    let stale = format!("{prefix}:{}", start.parse::<u64>().unwrap() + 1);
    let records = vec![window("TextEdit", stale)];

    let error = validate_record_generations(&records, &misdated_owner("TextEdit")).unwrap_err();

    assert_eq!(phase(error), "process_generation_changed");
}

#[test]
fn owner_without_a_name_keeps_the_strict_launch_time_check() {
    struct UnnamedOwner(NamedOwner);
    impl OwnerSnapshotView for UnnamedOwner {
        fn eligible_pids(&self) -> FxHashSet<i32> {
            self.0.eligible_pids()
        }
        fn generation(&self, pid: i32) -> Option<OwnerGeneration> {
            self.0.generation(pid)
        }
        fn frontmost_pid(&self) -> Option<i32> {
            None
        }
        fn same_generation(&self, _other: &Self) -> bool {
            true
        }
    }
    let records = vec![window("TextEdit", live_token())];

    let error = validate_record_generations(&records, &UnnamedOwner(misdated_owner("TextEdit")))
        .unwrap_err();

    assert_eq!(phase(error), "process_generation_changed");
}
