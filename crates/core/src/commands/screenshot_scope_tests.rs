use super::*;
use crate::AppInfo;

struct MultiInstanceAdapter {
    inner: ScreenshotAdapter,
}

impl ObservationOps for MultiInstanceAdapter {
    fn list_apps(&self, deadline: crate::Deadline) -> Result<Vec<AppInfo>, AdapterError> {
        let mut apps = self.inner.list_apps(deadline)?;
        let mut second = apps[0].clone();
        second.pid = crate::ProcessId::new(701);
        second.process_instance = Some("instance-701".into());
        apps.push(second);
        Ok(apps)
    }

    fn list_windows(
        &self,
        filter: &WindowFilter,
        deadline: crate::Deadline,
    ) -> Result<Vec<WindowInfo>, AdapterError> {
        self.inner.list_windows(filter, deadline)
    }
}

impl ActionOps for MultiInstanceAdapter {}
impl InputOps for MultiInstanceAdapter {}
impl SystemOps for MultiInstanceAdapter {
    fn screenshot(
        &self,
        target: ScreenshotTarget,
        deadline: crate::Deadline,
    ) -> Result<ImageBuffer, AdapterError> {
        self.inner.screenshot(target, deadline)
    }
}

#[test]
fn window_id_disambiguates_same_name_application_instances() {
    let adapter = MultiInstanceAdapter {
        inner: ScreenshotAdapter::new(),
    };
    let mut request = args();
    request.app = Some("Example".into());
    request.window_id = Some("w-42".into());

    screenshot::execute(request, &adapter).expect("window id pins one app instance");

    match adapter.inner.take_target() {
        Some(ScreenshotTarget::ExactWindow(window)) => {
            assert_eq!(window.id, "w-42");
            assert_eq!(window.pid, 700);
            assert_eq!(window.process_instance.as_deref(), Some("instance-700"));
        }
        _ => panic!("expected exact window target"),
    }
}

#[test]
fn window_id_rejects_an_owner_outside_the_named_application_instances() {
    for (pid, instance) in [(702, "instance-702"), (700, "replaced-instance-700")] {
        let mut inner = ScreenshotAdapter::new();
        inner.windows[1].pid = crate::ProcessId::new(pid);
        inner.windows[1].process_instance = Some(instance.into());
        let adapter = MultiInstanceAdapter { inner };
        let mut request = args();
        request.app = Some("Example".into());
        request.window_id = Some("w-42".into());

        let error = screenshot::execute(request, &adapter).expect_err("owner does not match");

        assert_eq!(error.code(), "WINDOW_NOT_FOUND");
        assert!(adapter.inner.take_target().is_none());
    }
}

#[test]
fn app_name_alone_still_refuses_multiple_instances() {
    let adapter = MultiInstanceAdapter {
        inner: ScreenshotAdapter::new(),
    };
    let mut request = args();
    request.app = Some("Example".into());

    let error = screenshot::execute(request, &adapter).expect_err("app is ambiguous");

    assert_eq!(error.code(), "AMBIGUOUS_TARGET");
    assert!(adapter.inner.take_target().is_none());
}
