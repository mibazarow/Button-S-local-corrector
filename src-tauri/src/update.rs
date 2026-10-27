use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub const RELEASES_URL: &str = "https://gitverse.ru/mibazarow/Button-S-local-corrector/releases";
const CHECK_INTERVAL_SECS: u64 = 8 * 60 * 60;
const LOG_LIMIT: usize = 3;
const LOG_FILE: &str = "update-check.json";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub url: Option<String>,
    pub available: bool,
    pub checked_at: u64,
    pub cached: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct UpdateLog {
    checks: Vec<UpdateCheck>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheck {
    checked_at: u64,
    current: String,
    latest: Option<String>,
    url: Option<String>,
    available: bool,
    error: Option<String>,
}

static LAST: OnceLock<Mutex<Option<UpdateInfo>>> = OnceLock::new();

fn memory() -> &'static Mutex<Option<UpdateInfo>> {
    LAST.get_or_init(|| Mutex::new(None))
}

fn store(info: &UpdateInfo) {
    *memory().lock() = Some(info.clone());
}

pub fn forget() {
    *memory().lock() = None;
}

/// Последний известный релиз, если он новее установленной версии.
pub fn available() -> Option<UpdateInfo> {
    memory().lock().clone().filter(|info| info.available)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn log_path(dir: &Path) -> PathBuf {
    dir.join(LOG_FILE)
}

fn read_log(dir: &Path) -> UpdateLog {
    std::fs::read_to_string(log_path(dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_log(dir: &Path, log: &UpdateLog) {
    if let Ok(text) = serde_json::to_string_pretty(log) {
        let _ = std::fs::write(log_path(dir), text);
    }
}

fn fresh(last: &UpdateCheck) -> bool {
    now_secs().saturating_sub(last.checked_at) < CHECK_INTERVAL_SECS
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let core = value.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

fn latest_from_html(html: &str) -> Option<String> {
    const MARKER: &str = "releases/tag/v";
    let mut best: Option<((u64, u64, u64), String)> = None;
    let mut rest = html;
    while let Some(at) = rest.find(MARKER) {
        let after = &rest[at + MARKER.len()..];
        let raw: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Some(version) = parse_version(&raw) {
            if best.as_ref().is_none_or(|(known, _)| version > *known) {
                best = Some((version, raw));
            }
        }
        rest = after;
    }
    best.map(|(_, raw)| raw)
}

async fn fetch_latest() -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .user_agent("BUTTON-S-update-check")
        .build()
        .map_err(|error| error.to_string())?;
    let body = client
        .get(RELEASES_URL)
        .send()
        .await
        .map_err(|error| format!("update check request: {error}"))?
        .error_for_status()
        .map_err(|error| format!("update check status: {error}"))?
        .text()
        .await
        .map_err(|error| format!("update check body: {error}"))?;
    let latest = latest_from_html(&body)
        .ok_or_else(|| "no release tag on the releases page".to_string())?;
    let url = format!("{RELEASES_URL}/tag/v{latest}");
    Ok((latest, url))
}

pub async fn check(dir: &Path) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let mut log = read_log(dir);

    if let Some(last) = log.checks.last() {
        if fresh(last) {
            let info = UpdateInfo {
                current: last.current.clone(),
                latest: last.latest.clone(),
                url: last.url.clone(),
                available: last.available,
                checked_at: last.checked_at,
                cached: true,
            };
            store(&info);
            return info;
        }
    }

    let checked_at = now_secs();
    let (latest, url, available, error) = match fetch_latest().await {
        Ok((latest, url)) => {
            let available = is_newer(&latest, &current);
            (Some(latest), Some(url), available, None)
        }
        Err(error) => {
            log::warn!("update check failed: {error}");
            (None, None, false, Some(error))
        }
    };

    log.checks.push(UpdateCheck {
        checked_at,
        current: current.clone(),
        latest: latest.clone(),
        url: url.clone(),
        available,
        error,
    });
    if log.checks.len() > LOG_LIMIT {
        let excess = log.checks.len() - LOG_LIMIT;
        log.checks.drain(0..excess);
    }
    write_log(dir, &log);

    let info = UpdateInfo {
        current,
        latest,
        url,
        available,
        checked_at,
        cached: false,
    };
    store(&info);
    info
}

pub fn spawn_check(app: AppHandle, state: Arc<AppState>) {
    if !state.config.read().general.check_updates {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Some(dir) = state.config_path.parent().map(Path::to_path_buf) else {
            return;
        };
        check(&dir).await;
        crate::tray::refresh(&app, &state);
    });
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    if !state.config.read().general.check_updates {
        return Ok(UpdateInfo {
            current: env!("CARGO_PKG_VERSION").to_string(),
            latest: None,
            url: None,
            available: false,
            checked_at: 0,
            cached: false,
        });
    }
    let dir = state
        .config_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Не удалось определить папку данных".to_string())?;
    let info = check(&dir).await;
    crate::tray::refresh(&app, &state);
    Ok(info)
}

const ALLOWED_HOSTS: &[&str] = &[
    "gitverse.ru",
    "bazarow.ru",
    "www.bazarow.ru",
    "donationalerts.com",
    "www.donationalerts.com",
];

fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    ALLOWED_HOSTS.contains(&host)
}

#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !allowed(&url) {
        return Err("Недопустимый адрес".into());
    }
    crate::platform::open_url(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_trusted_https_hosts_can_be_opened() {
        assert!(allowed("https://bazarow.ru/"));
        assert!(allowed("https://www.donationalerts.com/r/bazarow_ru"));
        assert!(allowed(
            "https://gitverse.ru/mibazarow/Button-S-local-corrector/releases/tag/v1.0.0"
        ));
        assert!(!allowed("http://bazarow.ru/"));
        assert!(!allowed("https://evil.test/"));
        assert!(!allowed("javascript:alert(1)"));
        assert!(!allowed("https://bazarow.ru.evil.test/"));
    }

    #[test]
    fn releases_url_points_to_the_button_s_repository() {
        assert!(RELEASES_URL.contains("/mibazarow/Button-S-local-corrector/releases"));
    }

    #[test]
    fn parses_versions_and_compares_them() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert!(is_newer("1.1.1", "1.1.0"));
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(!is_newer("1.1.0", "1.1.0"));
        assert!(!is_newer("1.0.9", "1.1.0"));
    }

    #[test]
    fn reads_the_highest_release_tag_from_the_page() {
        let html = r#"
            <a href="/mibazarow/Button-S-local-corrector/releases/tag/v1.1.0">1.1.0</a>
            <a href="/mibazarow/Button-S-local-corrector/releases/tag/v1.0.9">1.0.9</a>
            <a href="/mibazarow/Button-S-local-corrector/releases/tag/v1.2.0">1.2.0</a>
        "#;
        assert_eq!(latest_from_html(html).as_deref(), Some("1.2.0"));
        assert_eq!(latest_from_html("no releases"), None);
    }

    #[test]
    fn log_keeps_only_the_last_three_checks() {
        let mut log = UpdateLog::default();
        for i in 0..5 {
            log.checks.push(UpdateCheck {
                checked_at: now_secs(),
                current: "1.1.0".into(),
                latest: Some(format!("1.1.{i}")),
                url: None,
                available: false,
                error: None,
            });
        }
        let excess = log.checks.len() - LOG_LIMIT;
        log.checks.drain(0..excess);
        assert_eq!(log.checks.len(), 3);
        assert_eq!(log.checks.last().unwrap().latest.as_deref(), Some("1.1.4"));
    }

    #[test]
    fn cached_check_is_fresh_for_eight_hours() {
        let now = now_secs();
        let mut last = UpdateCheck {
            checked_at: now - 60,
            current: "1.0.0".into(),
            latest: Some("1.0.1".into()),
            url: None,
            available: true,
            error: None,
        };
        assert!(fresh(&last));
        last.checked_at = now.saturating_sub(CHECK_INTERVAL_SECS + 1);
        assert!(!fresh(&last));
    }
}
