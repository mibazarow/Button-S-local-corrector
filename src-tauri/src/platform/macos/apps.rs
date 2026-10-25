use objc2_app_kit::NSWorkspace;

use crate::platform::{RunningApp, RunningAppInfo};

pub fn frontmost_bundle_id() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle = app.bundleIdentifier()?;
    Some(bundle.to_string())
}

pub fn frontmost_app_info() -> Option<RunningAppInfo> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle = app.bundleIdentifier()?;
    Some(RunningAppInfo {
        pid: app.processIdentifier(),
        bundle_id: bundle.to_string(),
    })
}

pub fn running_apps() -> Vec<RunningApp> {
    let workspace = NSWorkspace::sharedWorkspace();
    workspace
        .runningApplications()
        .iter()
        .filter_map(|app| {
            let bundle_id = app.bundleIdentifier()?.to_string();
            let name = app
                .localizedName()
                .map(|name| name.to_string())
                .unwrap_or_else(|| bundle_id.clone());
            Some(RunningApp { bundle_id, name })
        })
        .collect()
}
