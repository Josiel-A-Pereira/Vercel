//! Seeding the refs a policy-gate scenario acts on, the way a real
//! snapshot of a running app's window would have recorded them.

use agent_desktop_core::{
    RefCapabilities, RefEntry, RefEntryIdentity, RefGeometry, RefProcess, RefScope, RefSource,
    refs::refs_store::RefStore,
};

use super::policy_gate_desk::{Scenario, Screen, pid_of};

impl Scenario {
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
        self.show_seeded(app, window);
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
                    unique_at_capture: true,
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
                    renderer_accessibility_forced: false,
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

    /// A ref is only ever captured from a running app's listed window, which
    /// the action re-finds before it reads the element.
    fn show_seeded(&self, app: &str, window: &str) {
        let mut screen = self.adapter.screen.lock().unwrap();
        if !screen.apps.iter().any(|listed| listed.name == app) {
            screen.apps.push(Screen::app(
                app,
                &format!("scenario.{}", app.to_lowercase()),
            ));
        }
        if !screen
            .windows
            .iter()
            .any(|listed| listed.app == app && listed.title == window)
        {
            let id = format!("seeded-{}", screen.windows.len());
            screen
                .windows
                .push(Screen::window(&id, app, window, (0.0, 0.0, 1.0, 1.0)));
        }
    }
}
