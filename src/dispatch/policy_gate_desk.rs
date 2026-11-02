//! The desk the policy-gate scenarios play out on: ref resolution is
//! counted and then stopped (the security question is whether the adapter
//! was reached at all), while the clipboard genuinely works so temporal
//! policies see real successes and failures.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use agent_desktop_core::{
    ActionOps, AdapterError, AppInfo, ClipboardContent, ClipboardFormat, Deadline, ErrorCode,
    InputOps, InstalledApp, InteractionLease, NativeHandle, ObservationOps, OperationReport,
    OperationTarget, PermissionReport, Point, PolicyPaths, ProcessId, Rect, RefCapabilities,
    RefEntry, RefEntryIdentity, RefGeometry, RefProcess, RefScope, RefSource, SystemOps,
    WindowFilter, WindowInfo, WindowState, context::CommandContext, refs::refs_store::RefStore,
};
use clap::Parser;

use super::test_support::HomeGuard;
use crate::cli::{Cli, Commands};

#[derive(Default)]
pub(super) struct DeskAdapter {
    pub(super) resolve_calls: AtomicU32,
    pub(super) clipboard: Mutex<Option<String>>,
    pub(super) pasteboard_broken: AtomicBool,
    pub(super) screen: Mutex<Screen>,
    pub(super) target_queries: AtomicU32,
}

/// What is on screen, front to back, which window has keyboard focus, and
/// which applications run or are installed.
#[derive(Default)]
pub(super) struct Screen {
    pub(super) windows: Vec<WindowInfo>,
    pub(super) focused: Option<String>,
    pub(super) overlap_unknown: bool,
    pub(super) apps: Vec<AppInfo>,
    pub(super) installed: Vec<InstalledApp>,
    pub(super) inventory_broken: bool,
}

/// Each scenario app runs as its own process, so a window or ref is owned
/// by exactly the app it was created under.
pub(super) fn pid_of(app: &str) -> ProcessId {
    ProcessId::new(1000 + app.bytes().map(u32::from).sum::<u32>())
}

impl Screen {
    pub(super) fn app(name: &str, bundle_id: &str) -> AppInfo {
        AppInfo {
            name: name.to_string(),
            pid: pid_of(name),
            bundle_id: Some(bundle_id.to_string()),
            process_instance: Some("scenario-proc-1".to_string()),
            presentation: None,
        }
    }

    pub(super) fn window(
        id: &str,
        app: &str,
        title: &str,
        bounds: (f64, f64, f64, f64),
    ) -> WindowInfo {
        WindowInfo {
            id: id.to_string(),
            title: title.to_string(),
            app: app.to_string(),
            pid: pid_of(app),
            process_instance: Some("scenario-proc-1".to_string()),
            bounds: Some(Rect {
                x: bounds.0,
                y: bounds.1,
                width: bounds.2,
                height: bounds.3,
            }),
            state: WindowState {
                is_focused: false,
                accessible: true,
                minimized: None,
                visible: Some(true),
            },
        }
    }
}

impl ObservationOps for DeskAdapter {
    fn list_apps(&self, _deadline: Deadline) -> Result<Vec<AppInfo>, AdapterError> {
        self.target_queries.fetch_add(1, Ordering::SeqCst);
        let screen = self.screen.lock().unwrap();
        if screen.inventory_broken {
            return Err(AdapterError::new(
                ErrorCode::AppUnresponsive,
                "the application inventory is churning",
            ));
        }
        Ok(screen.apps.clone())
    }

    fn find_installed_app(
        &self,
        identifier: &str,
        _deadline: Deadline,
    ) -> Result<Option<InstalledApp>, AdapterError> {
        Ok(self
            .screen
            .lock()
            .unwrap()
            .installed
            .iter()
            .find(|app| {
                app.name.eq_ignore_ascii_case(identifier)
                    || app
                        .bundle_id
                        .as_deref()
                        .is_some_and(|id| id.eq_ignore_ascii_case(identifier))
            })
            .cloned())
    }

    fn list_windows(
        &self,
        _filter: &WindowFilter,
        _deadline: Deadline,
    ) -> Result<Vec<WindowInfo>, AdapterError> {
        self.target_queries.fetch_add(1, Ordering::SeqCst);
        Ok(self.screen.lock().unwrap().windows.clone())
    }

    fn resolve_element_strict(
        &self,
        _entry: &RefEntry,
        _deadline: Deadline,
    ) -> Result<NativeHandle, AdapterError> {
        self.resolve_calls.fetch_add(1, Ordering::SeqCst);
        Err(AdapterError::new(
            ErrorCode::StaleRef,
            "scenario desk stops at resolution",
        ))
    }
}

impl ActionOps for DeskAdapter {}

impl InputOps for DeskAdapter {
    fn get_clipboard_content(
        &self,
        _format: ClipboardFormat,
        _deadline: Deadline,
    ) -> Result<Option<ClipboardContent>, AdapterError> {
        Ok(self
            .clipboard
            .lock()
            .unwrap()
            .clone()
            .map(ClipboardContent::Text))
    }

    fn set_clipboard_content(
        &self,
        content: &ClipboardContent,
        _lease: &InteractionLease,
    ) -> Result<OperationReport, AdapterError> {
        if self.pasteboard_broken.load(Ordering::SeqCst) {
            return Err(AdapterError::new(
                ErrorCode::ActionFailed,
                "pasteboard unavailable",
            ));
        }
        if let ClipboardContent::Text(text) = content {
            *self.clipboard.lock().unwrap() = Some(text.clone());
        }
        Ok(OperationReport::delivered_unverified(
            "clipboard_write",
            OperationTarget::Clipboard,
        ))
    }
}

impl SystemOps for DeskAdapter {
    fn acquire_interaction_lease(
        &self,
        deadline: Deadline,
    ) -> Result<InteractionLease, AdapterError> {
        InteractionLease::guarded(deadline, ())
    }

    fn focused_window(&self, _deadline: Deadline) -> Result<Option<WindowInfo>, AdapterError> {
        self.target_queries.fetch_add(1, Ordering::SeqCst);
        let screen = self.screen.lock().unwrap();
        Ok(screen.focused.as_ref().and_then(|id| {
            screen
                .windows
                .iter()
                .find(|window| &window.id == id)
                .cloned()
        }))
    }

    fn window_at_point(
        &self,
        point: Point,
        _deadline: Deadline,
    ) -> Result<Option<WindowInfo>, AdapterError> {
        self.target_queries.fetch_add(1, Ordering::SeqCst);
        let screen = self.screen.lock().unwrap();
        if screen.overlap_unknown {
            return Err(AdapterError::new(
                ErrorCode::ActionFailed,
                "windows of several apps overlap the point",
            ));
        }
        Ok(screen
            .windows
            .iter()
            .find(|window| {
                window.bounds.as_ref().is_some_and(|b| {
                    point.x >= b.x
                        && point.y >= b.y
                        && point.x < b.x + b.width
                        && point.y < b.y + b.height
                })
            })
            .cloned())
    }
}

pub(super) struct Scenario {
    _home: HomeGuard,
    dir: std::path::PathBuf,
    policy_file: std::path::PathBuf,
    pub(super) adapter: DeskAdapter,
}

impl Scenario {
    pub(super) fn new(policy: &str) -> Self {
        let home = HomeGuard::new();
        let dir = home.path().join("owner-policies");
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let policy_file = dir.join("guardrails.dogwood");
        write_policy(&policy_file, policy);
        Self {
            _home: home,
            dir: dir.clone(),
            policy_file,
            adapter: DeskAdapter::default(),
        }
    }

    pub(super) fn rewrite_policy(&self, policy: &str) {
        write_policy(&self.policy_file, policy);
    }

    pub(super) fn context(&self) -> CommandContext {
        CommandContext::default().with_policy_paths(PolicyPaths {
            files: Vec::new(),
            dirs: vec![self.dir.clone()],
            env_dir: None,
        })
    }

    pub(super) fn run(
        &self,
        args: &[&str],
    ) -> Result<serde_json::Value, agent_desktop_core::AppError> {
        run_command(self, parse(args), &self.context())
    }

    pub(super) fn run_headed(
        &self,
        args: &[&str],
    ) -> Result<serde_json::Value, agent_desktop_core::AppError> {
        run_command(self, parse(args), &self.context().with_headed(true))
    }

    pub(super) fn target_queries(&self) -> u32 {
        self.adapter.target_queries.load(Ordering::SeqCst)
    }

    pub(super) fn resolve_calls(&self) -> u32 {
        self.adapter.resolve_calls.load(Ordering::SeqCst)
    }

    /// Seed one snapshotted element the way a real snapshot would have, and
    /// hand back the qualified ref an agent would act on.
    pub(super) fn seed_ref(
        &self,
        app: &str,
        window: &str,
        role: &str,
        name: &str,
        secure: bool,
    ) -> String {
        let mut refmap = agent_desktop_core::RefMap::new();
        let states = if secure {
            vec!["secure".to_string()]
        } else {
            Vec::new()
        };
        let local = refmap
            .try_allocate(RefEntry {
                process: RefProcess {
                    pid: pid_of(app),
                    process_instance: Some("scenario-proc-1".to_string()),
                },
                identity: RefEntryIdentity {
                    retained_object: None,
                    role: role.to_string(),
                    name: Some(name.to_string()),
                    value: None,
                    description: None,
                    native_id: None,
                },
                geometry: RefGeometry {
                    bounds: None,
                    bounds_hash: None,
                    bounds_space: None,
                },
                capabilities: RefCapabilities {
                    states,
                    available_actions: vec!["Click".to_string()],
                },
                source: RefSource {
                    source_app: Some(app.to_string()),
                    source_window_id: None,
                    source_window_title: Some(window.to_string()),
                    source_window_bounds_hash: None,
                    source_surface: Default::default(),
                },
                scope: RefScope {
                    root_ref: None,
                    path_is_absolute: false,
                    path: Default::default(),
                },
            })
            .expect("ref allocates");
        let snapshot_id = RefStore::new()
            .expect("ref store opens")
            .save_new_snapshot(&refmap)
            .expect("snapshot saves");
        format!("@{snapshot_id}:{}", local.trim_start_matches('@'))
    }
}

pub(super) fn run_command(
    scenario: &Scenario,
    cmd: Commands,
    context: &CommandContext,
) -> Result<serde_json::Value, agent_desktop_core::AppError> {
    super::dispatch(
        cmd,
        &scenario.adapter,
        &PermissionReport::default(),
        context,
    )
}

pub(super) fn parse(args: &[&str]) -> Commands {
    let mut argv = vec!["agent-desktop"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv)
        .expect("scenario command parses")
        .command
        .expect("scenario command present")
}

fn write_policy(path: &std::path::Path, policy: &str) {
    std::fs::write(path, policy).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

pub(super) fn error_code(
    result: &Result<serde_json::Value, agent_desktop_core::AppError>,
) -> String {
    match result {
        Ok(_) => "OK".to_string(),
        Err(error) => error.code().to_string(),
    }
}
