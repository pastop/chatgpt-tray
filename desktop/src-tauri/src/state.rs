use std::sync::{Arc, Mutex};

use chatgpt_tray::models::{UsageResponse, UsageWindow};
use tauri::Emitter;
use tokio::sync::mpsc;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageSnapshot {
    pub(crate) primary: Option<WindowSnapshot>,
    pub(crate) secondary: Option<WindowSnapshot>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WindowSnapshot {
    pub(crate) remaining: u8,
    pub(crate) reset_at: i64,
}

impl From<&UsageWindow> for WindowSnapshot {
    fn from(window: &UsageWindow) -> Self {
        Self {
            remaining: (100.0_f64 - window.used_percent).clamp(0.0, 100.0).round() as u8,
            reset_at: window.reset_at,
        }
    }
}

impl From<UsageResponse> for UsageSnapshot {
    fn from(usage: UsageResponse) -> Self {
        Self {
            primary: usage
                .rate_limit
                .primary_window
                .as_ref()
                .map(WindowSnapshot::from),
            secondary: usage
                .rate_limit
                .secondary_window
                .as_ref()
                .map(WindowSnapshot::from),
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppStatus {
    pub(crate) usage: Option<UsageSnapshot>,
    pub(crate) plan: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) updated_at_epoch: Option<u64>,
    pub(crate) next_retry_at_epoch: Option<u64>,
    pub(crate) error: Option<String>,
    pub(crate) loading: bool,
    pub(crate) from_cache: bool,
}

impl Default for AppStatus {
    fn default() -> Self {
        Self {
            usage: None,
            plan: None,
            updated_at: None,
            updated_at_epoch: None,
            next_retry_at_epoch: None,
            error: None,
            loading: true,
            from_cache: false,
        }
    }
}

pub(crate) type SharedStatus = Arc<Mutex<AppStatus>>;

pub(crate) struct AppState {
    pub(crate) status: SharedStatus,
    pub(crate) refresh_tx: mpsc::Sender<()>,
}

pub(crate) fn publish_status(
    app: &tauri::AppHandle,
    shared: &SharedStatus,
    update: impl FnOnce(&mut AppStatus),
) {
    let current = {
        let Ok(mut status) = shared.lock() else {
            eprintln!("Ошибка блокировки состояния");
            return;
        };

        update(&mut status);
        status.clone()
    };

    if let Err(error) = app.emit("usage-updated", current) {
        eprintln!("Ошибка отправки события: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatgpt_tray::api;

    #[test]
    fn absent_usage_windows_are_preserved() {
        let window = serde_json::json!({
            "used_percent": 25.0, "limit_window_seconds": 3600,
            "reset_after_seconds": 0, "reset_at": 1
        });
        for (primary, secondary) in [(false, false), (false, true), (true, false)] {
            let mut rate_limit = serde_json::json!({"allowed": true, "limit_reached": false});
            if primary {
                rate_limit["primary_window"] = window.clone();
            }
            if secondary {
                rate_limit["secondary_window"] = window.clone();
            }
            let usage: UsageResponse = serde_json::from_value(serde_json::json!({
                "plan_type": "test", "rate_limit": rate_limit
            }))
            .unwrap();
            let snapshot = UsageSnapshot::from(usage);
            assert_eq!(snapshot.primary.is_some(), primary);
            assert_eq!(snapshot.secondary.is_some(), secondary);
            let json = serde_json::to_value(snapshot).unwrap();
            assert_eq!(json["primary"].is_null(), !primary);
            assert_eq!(json["secondary"].is_null(), !secondary);
        }
    }

    #[test]
    fn status_epoch_schema_with_cached_data_and_error() {
        let mut status = AppStatus::default();
        let initial = serde_json::to_value(&status).unwrap();
        assert_eq!(initial["fromCache"], false);
        status.from_cache = true;
        assert!(initial["updatedAtEpoch"].is_null());
        assert!(initial.get("nextRetryAtEpoch").unwrap().is_null());
        status.usage = Some(UsageSnapshot {
            primary: None,
            secondary: None,
        });
        status.updated_at = Some("12:00:00".into());
        status.updated_at_epoch = Some(1_800_000_000);
        status.error = Some(api::ApiError::Network.to_string());
        status.next_retry_at_epoch = Some(1_800_000_030);
        status.loading = false;
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["fromCache"], true);
        assert_eq!(value["updatedAtEpoch"], 1_800_000_000_u64);
        assert_eq!(value["nextRetryAtEpoch"], 1_800_000_030_u64);
        assert_eq!(value["updatedAt"], "12:00:00");
        assert!(value["usage"].is_object());
        assert!(value.get("updated_at_epoch").is_none());
    }
}
