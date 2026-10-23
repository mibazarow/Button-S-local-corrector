use objc2_app_kit::NSWorkspace;

pub fn frontmost_bundle_id() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle = app.bundleIdentifier()?;
    Some(bundle.to_string())
}

use crate::platform::RunningApp;

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
