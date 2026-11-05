//! Resolving a [`TargetPlan`] into the applications, windows and elements a
//! policy judges. Every target is named the way the platform lists the app
//! running and carries its stable identifier, so a rule holds whatever the
//! agent typed: an alias, another letter case, the bundle id, or a display
//! name that changes with the system language. Keyboard and pointer input
//! that names no ref is judged as the application it lands in, never as
//! `system`.

use agent_desktop_core::{
    AppInfo, Deadline, ErrorCode, PlatformAdapter, Point, ProcessId, SnapshotSurface, WindowFilter,
    WindowInfo, context::CommandContext, refs::refs_store::RefStore,
};
use agent_desktop_policy::TargetFacts;

use super::dogwood_plan::TargetPlan;

const TARGET_LOOKUP_MS: u64 = 2_000;

/// Resolves the plans of one command, listing the running applications at
/// most once however many targets it has.
pub(super) struct Resolver<'a> {
    context: &'a CommandContext,
    adapter: &'a dyn PlatformAdapter,
    running: Option<Vec<AppInfo>>,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(context: &'a CommandContext, adapter: &'a dyn PlatformAdapter) -> Self {
        Self {
            context,
            adapter,
            running: None,
        }
    }

    /// Resolve one plan into every application it may reach. An error means
    /// the receiving application could not be determined, which the gate
    /// refuses rather than guess at.
    pub(super) fn resolve(&mut self, plan: &TargetPlan<'_>) -> Result<Vec<TargetFacts>, String> {
        match plan {
            TargetPlan::System | TargetPlan::App(None) => Ok(vec![TargetFacts::default()]),
            TargetPlan::App(Some(query)) => self.named(query),
            TargetPlan::Ref(ref_id) => self.ref_target(ref_id).map(|target| vec![target]),
            TargetPlan::Focused => self.focused().map(|target| vec![target]),
            TargetPlan::WindowId(window_id) => {
                let window = self
                    .listed_windows()?
                    .into_iter()
                    .find(|window| window.id == *window_id)
                    .ok_or_else(|| format!("no listed window has id {window_id}"))?;
                self.window(Some(window)).map(|target| vec![target])
            }
            TargetPlan::Titled { app, title } => self.titled(*app, title),
            TargetPlan::Point(x, y) => {
                let window = self
                    .adapter
                    .window_at_point(Point { x: *x, y: *y }, lookup_deadline()?)
                    .map_err(|error| {
                        format!(
                            "the window under ({x}, {y}) could not be determined: {}",
                            error.message
                        )
                    })?;
                self.window(window).map(|target| vec![target])
            }
            TargetPlan::AppWindows(query) => self.app_windows(query),
            TargetPlan::Running => {
                let running: Vec<TargetFacts> = self.running()?.iter().map(app_target).collect();
                Ok(if running.is_empty() {
                    vec![TargetFacts::default()]
                } else {
                    running
                })
            }
            TargetPlan::Unnamed(reason) => Err((*reason).to_string()),
            TargetPlan::Shell { surface, app } => self.shell(*surface, *app),
        }
    }

    /// The focused window's owner. A frontmost app with no focused window still
    /// receives keys and presents its menus, so it is the one judged; only
    /// with no frontmost app, or a platform that reports none, is the command
    /// judged as `system`.
    fn focused(&mut self) -> Result<TargetFacts, String> {
        let window = self
            .adapter
            .focused_window(lookup_deadline()?)
            .map_err(|error| format!("the focused window could not be read: {}", error.message))?;
        if window.is_some() {
            return self.window(window);
        }
        match self.adapter.focused_app(lookup_deadline()?) {
            Ok(app) => Ok(app.as_ref().map(app_target).unwrap_or_default()),
            Err(error) if error.is_default_not_supported("focused_app") => {
                Ok(TargetFacts::default())
            }
            Err(error) => Err(format!(
                "the frontmost application could not be read: {}",
                error.message
            )),
        }
    }

    /// The shell process hosting `surface`, which is what the command reads.
    /// A platform that resolves no shell surface reads the app's windows, or
    /// the focused window, as the command itself does.
    fn shell(
        &mut self,
        surface: SnapshotSurface,
        app: Option<&str>,
    ) -> Result<Vec<TargetFacts>, String> {
        match self
            .adapter
            .resolve_shell_surface(surface, lookup_deadline()?)
        {
            Ok(window) => self.window(Some(window)).map(|target| vec![target]),
            Err(error) if error.is_default_not_supported("resolve_shell_surface") => {
                self.resolve(&match app {
                    Some(app) => TargetPlan::AppWindows(app),
                    None => TargetPlan::Focused,
                })
            }
            Err(error) => Err(format!(
                "the {} surface could not be read: {}",
                surface.as_str(),
                error.message
            )),
        }
    }

    /// Every listed window of the app `query` names, each with its title,
    /// since the command may act on any of them. Only that app's windows are
    /// listed, which spares reading every other app's. An app with no listed
    /// window, one not running, or a platform that lists none, is judged as
    /// the app alone.
    fn app_windows(&mut self, query: &str) -> Result<Vec<TargetFacts>, String> {
        let apps = self.named(query)?;
        let mut windows = Vec::new();
        for app in &apps {
            let filter = WindowFilter {
                focused_only: false,
                app: Some(app.app.clone()),
            };
            windows.extend(
                self.adapter
                    .list_windows(&filter, lookup_deadline()?)
                    .or_else(|error| match error.code {
                        ErrorCode::AppNotFound => Ok(Vec::new()),
                        _ => unsupported_as_empty(error),
                    })
                    .map_err(|error| format!("windows could not be listed: {}", error.message))?,
            );
        }
        let mut targets: Vec<TargetFacts> = Vec::new();
        for window in windows {
            let target = self.window(Some(window))?;
            let owned = apps
                .iter()
                .any(|app| app.app == target.app && app.app_id == target.app_id);
            if owned && !targets.contains(&target) {
                targets.push(target);
            }
        }
        Ok(if targets.is_empty() { apps } else { targets })
    }

    /// Every running application `query` names (name in any letter case,
    /// bundle id); failing that, the installed application it would launch;
    /// failing that, the processes the platform's own lookup matches
    /// (verified aliases, processes outside the app inventory); failing
    /// that, the name as typed, since nothing it names can be reached. The
    /// platform lookup is the slowest, so it runs last.
    fn named(&mut self, query: &str) -> Result<Vec<TargetFacts>, String> {
        let mut matched: Vec<TargetFacts> = self
            .running()?
            .iter()
            .filter(|app| app.matches_identifier(query))
            .map(app_target)
            .collect();
        if matched.is_empty() {
            matched = self.installed(query)?;
        }
        if matched.is_empty() {
            matched = self
                .adapter
                .list_apps_scoped(query, None, lookup_deadline()?)
                .or_else(unsupported_as_empty)
                .map_err(|error| format!("'{query}' could not be looked up: {}", error.message))?
                .iter()
                .map(app_target)
                .collect();
        }
        if matched.is_empty() {
            matched.push(TargetFacts {
                app: query.to_string(),
                ..TargetFacts::default()
            });
        }
        let mut unique: Vec<TargetFacts> = Vec::with_capacity(matched.len());
        for target in matched {
            if !unique.contains(&target) {
                unique.push(target);
            }
        }
        Ok(unique)
    }

    /// The installed application `query` would launch. When a process of it
    /// already runs under another name (a Linux desktop `Name` is an alias
    /// of the listed process name), the running app is the one judged.
    fn installed(&mut self, query: &str) -> Result<Vec<TargetFacts>, String> {
        let installed = self
            .adapter
            .find_installed_app(query, lookup_deadline()?)
            .or_else(|error| match error.code {
                ErrorCode::PlatformNotSupported => Ok(None),
                _ => Err(error),
            })
            .map_err(|error| {
                format!(
                    "installed '{query}' could not be looked up: {}",
                    error.message
                )
            })?;
        let Some(installed) = installed else {
            return Ok(Vec::new());
        };
        let running: Vec<TargetFacts> = match installed.bundle_id.as_deref() {
            Some(id) => self
                .running()?
                .iter()
                .filter(|app| app.bundle_id.as_deref() == Some(id))
                .map(app_target)
                .collect(),
            None => Vec::new(),
        };
        if !running.is_empty() {
            return Ok(running);
        }
        Ok(vec![TargetFacts {
            app: installed.name,
            app_id: installed.bundle_id.unwrap_or_default(),
            ..TargetFacts::default()
        }])
    }

    /// The ref's recorded target, named by the process that owns it. A ref
    /// that no longer loads leaves the target empty; dispatch fails it with
    /// `STALE_REF` on the same store, so nothing executable slips past
    /// app-scoped policies.
    fn ref_target(&mut self, ref_id: &str) -> Result<TargetFacts, String> {
        let entry = match RefStore::for_session(self.context.session_id())
            .and_then(|store| store.load_ref(ref_id))
        {
            Ok(entry) => entry,
            Err(error) => {
                tracing::debug!(%error, ref_id, "policy gate could not resolve ref target");
                return Ok(TargetFacts::default());
            }
        };
        let (app, app_id) = self.owner(
            entry.process.pid,
            entry.process.process_instance.as_deref(),
            entry.source.source_app.as_deref().unwrap_or_default(),
        )?;
        Ok(TargetFacts {
            app,
            app_id,
            window: entry.source.source_window_title.clone().unwrap_or_default(),
            role: entry.identity.role.clone(),
            element: entry.identity.name.clone().unwrap_or_default(),
            secure: carries_secure_state(&entry.capabilities.states),
        })
    }

    fn window(&mut self, window: Option<WindowInfo>) -> Result<TargetFacts, String> {
        let Some(window) = window else {
            return Ok(TargetFacts::default());
        };
        let (app, app_id) =
            self.owner(window.pid, window.process_instance.as_deref(), &window.app)?;
        Ok(TargetFacts {
            app,
            app_id,
            window: window.title,
            ..TargetFacts::default()
        })
    }

    /// Every window whose title holds `title` may be the one focused, so
    /// each owner is judged. With no such window the command finds nothing
    /// to act on, and the named app (or none) is judged.
    fn titled(&mut self, app: Option<&str>, title: &str) -> Result<Vec<TargetFacts>, String> {
        let windows: Vec<WindowInfo> = self
            .listed_windows()?
            .into_iter()
            .filter(|window| window.title.contains(title))
            .collect();
        if windows.is_empty() {
            return self.resolve(&TargetPlan::App(app));
        }
        windows
            .into_iter()
            .map(|window| self.window(Some(window)))
            .collect()
    }

    /// The running application that owns `pid`, by its listed name and
    /// stable identifier. A process no longer listed keeps the name it was
    /// observed under.
    fn owner(
        &mut self,
        pid: ProcessId,
        instance: Option<&str>,
        observed_name: &str,
    ) -> Result<(String, String), String> {
        let owner = self.running()?.iter().find(|app| {
            app.pid == pid
                && match (instance, app.process_instance.as_deref()) {
                    (Some(expected), Some(listed)) => expected == listed,
                    _ => true,
                }
        });
        Ok(match owner {
            Some(app) => (app.name.clone(), app.bundle_id.clone().unwrap_or_default()),
            None => (observed_name.to_string(), String::new()),
        })
    }

    fn running(&mut self) -> Result<&[AppInfo], String> {
        if self.running.is_none() {
            let apps = self
                .adapter
                .list_app_identities(lookup_deadline()?)
                .map_err(|error| {
                    format!(
                        "running applications could not be listed: {}",
                        error.message
                    )
                })?;
            self.running = Some(apps);
        }
        Ok(self.running.as_deref().unwrap_or_default())
    }

    fn listed_windows(&self) -> Result<Vec<WindowInfo>, String> {
        self.adapter
            .list_windows(&WindowFilter::default(), lookup_deadline()?)
            .map_err(|error| format!("windows could not be listed: {}", error.message))
    }
}

fn app_target(app: &AppInfo) -> TargetFacts {
    TargetFacts {
        app: app.name.clone(),
        app_id: app.bundle_id.clone().unwrap_or_default(),
        ..TargetFacts::default()
    }
}

fn unsupported_as_empty<T>(
    error: agent_desktop_core::AdapterError,
) -> Result<Vec<T>, agent_desktop_core::AdapterError> {
    match error.code {
        ErrorCode::PlatformNotSupported => Ok(Vec::new()),
        _ => Err(error),
    }
}

fn lookup_deadline() -> Result<Deadline, String> {
    Deadline::after(TARGET_LOOKUP_MS).map_err(|error| error.message)
}

/// A password-style field keeps the canonical role `textfield`; its
/// secure-ness travels only as a state token on the ref.
fn carries_secure_state(states: &[String]) -> bool {
    states
        .iter()
        .any(|state| state == agent_desktop_core::tree::state::SECURE)
}
