use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chatgpt_tray::{api, auth};
use chrono::Local;
use tauri::Manager;
use tokio::sync::mpsc;

use crate::{
    cache,
    state::{SharedStatus, UsageSnapshot, publish_status},
    tray::{TrayView, render},
    tray_icon::{IconState, generate},
};

const TIMER_INTERVAL: Duration = Duration::from_secs(60);

fn retry_epoch(deadline: std::time::Instant, now: std::time::Instant, epoch: u64) -> u64 {
    let remaining = deadline.saturating_duration_since(now);
    // Round up so the displayed cooldown never ends before the monotonic gate.
    epoch
        .saturating_add(remaining.as_secs())
        .saturating_add(u64::from(remaining.subsec_nanos() != 0))
}

async fn load_usage(client: &reqwest::Client) -> anyhow::Result<UsageSnapshot> {
    let token = auth::read_access_token().map_err(|_| api::ApiError::Unauthorized)?;
    let response = api::fetch_usage(client, &token).await?;

    Ok(UsageSnapshot::from(response))
}

pub(crate) async fn background_worker(
    view: TrayView,
    mut refresh_rx: mpsc::Receiver<()>,
    app: tauri::AppHandle,
    shared: SharedStatus,
) {
    // Await startup IO before HTTP: a late cache event cannot overwrite success.
    let mut snapshot: Option<UsageSnapshot> = None;
    match app.path().app_data_dir() {
        Ok(directory) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if let Some(cached) = cache::load_async(directory, now).await {
                render(&view, &cached.usage);
                snapshot = Some(cached.usage.clone());
                let _ = view.status.set_text(format!(
                    "Кеш: получено в {} (старые данные)",
                    cached.updated_at
                ));
                publish_status(&app, &shared, |status| {
                    status.usage = Some(cached.usage);
                    status.updated_at = Some(cached.updated_at);
                    status.updated_at_epoch = Some(cached.updated_at_epoch);
                    status.from_cache = true;
                });
            }
        }
        Err(_) => eprintln!("Каталог кеша статистики недоступен"),
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("Ошибка создания HTTP-клиента: {error}");
            publish_status(&app, &shared, |status| {
                status.error = Some("Не удалось создать HTTP-клиент.".into());
                status.loading = false;
            });
            let _ = view.status.set_text("Ошибка HTTP-клиента");
            return;
        }
    };

    let mut retry = api::RetryPolicy::new(std::time::Instant::now());
    let mut timer_interval = tokio::time::interval(TIMER_INTERVAL);

    timer_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Deadline starts immediately and is moved only after a completed request.
            _ = tokio::time::sleep_until(retry.deadline().into()) => {
                refresh(&client, &view, &mut snapshot, &app, &shared, &mut retry).await;
                while refresh_rx.try_recv().is_ok() {}
            }

            // Ручное обновление через меню.
            Some(()) = refresh_rx.recv() => {
                if retry.manual_ready(std::time::Instant::now()) {
                    refresh(&client, &view, &mut snapshot, &app, &shared, &mut retry).await;
                    while refresh_rx.try_recv().is_ok() {}
                }
            }

            // Локальное обновление обратного отсчёта.
            _ = timer_interval.tick() => {
                if let Some(data) = &snapshot {
                    render(&view, data);
                }
            }
        }
    }
}

async fn refresh(
    client: &reqwest::Client,
    view: &TrayView,
    snapshot: &mut Option<UsageSnapshot>,
    app: &tauri::AppHandle,
    shared: &SharedStatus,
    retry: &mut api::RetryPolicy,
) {
    let _ = view.status.set_text("Обновление...");

    publish_status(app, shared, |status| {
        status.loading = true;
    });

    match load_usage(client).await {
        Ok(data) => {
            retry.success(std::time::Instant::now());
            let updated_at_epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            render(view, &data);
            *snapshot = Some(data.clone());

            let updated_at = Local::now().format("%H:%M:%S").to_string();

            let _ = view.status.set_text(format!("Обновлено в {updated_at}"));

            let cached =
                cache::CachedUsage::new(data.clone(), updated_at_epoch, updated_at.clone());
            publish_status(app, shared, |status| {
                status.from_cache = false;
                status.usage = Some(data);
                status.updated_at = Some(updated_at);
                status.updated_at_epoch = Some(updated_at_epoch);
                status.next_retry_at_epoch = None;
                status.error = None;
                status.loading = false;
            });
            match app.path().app_data_dir() {
                Ok(directory) => cache::save_async(directory, cached, updated_at_epoch).await,
                Err(_) => eprintln!("Каталог кеша статистики недоступен"),
            }
        }
        Err(error) => {
            let error = error
                .downcast_ref::<api::ApiError>()
                .copied()
                .unwrap_or(api::ApiError::InvalidResponse);
            let now = std::time::Instant::now();
            retry.failure(now, error);
            let epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let next_retry_at_epoch = retry_epoch(retry.deadline(), now, epoch);
            let message = error.to_string();
            eprintln!("Ошибка API: {message}");

            if snapshot.is_none() {
                let _ = view.tray.set_icon(Some(generate(IconState::Error)));
                let _ = view.status.set_text("Ошибка обновления");
            } else {
                let _ = view.status.set_text("Ошибка: показаны старые данные");
            }

            publish_status(app, shared, |status| {
                status.error = Some(message);
                status.next_retry_at_epoch = Some(next_retry_at_epoch);
                status.loading = false;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_deadline_converts_to_epoch() {
        let now = std::time::Instant::now();
        assert_eq!(retry_epoch(now + Duration::from_secs(900), now, 1000), 1900);
        assert_eq!(
            retry_epoch(now + Duration::from_millis(1500), now, 1000),
            1002
        );
        assert_eq!(retry_epoch(now, now + Duration::from_secs(1), 1000), 1000);
    }
}
