use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{
    Manager, Wry,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
};
use tokio::sync::mpsc;

use crate::{
    state::{UsageSnapshot, WindowSnapshot},
    tray_icon::{IconState, generate},
};

pub(crate) struct TrayView {
    pub(crate) tray: TrayIcon,
    five_hour: MenuItem<Wry>,
    five_hour_reset: MenuItem<Wry>,
    weekly: MenuItem<Wry>,
    weekly_reset: MenuItem<Wry>,
    pub(crate) status: MenuItem<Wry>,
}

fn seconds_until(timestamp: i64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    timestamp.saturating_sub(now).max(0) as u64
}

fn format_duration(seconds: u64) -> String {
    if seconds == 0 {
        return "Сброс ожидается".into();
    }

    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    if days > 0 {
        format!("{days} д. {hours} ч.")
    } else if hours > 0 {
        format!("{hours} ч. {minutes} мин.")
    } else if minutes > 0 {
        format!("{minutes} мин.")
    } else {
        "Менее минуты".into()
    }
}

fn update_window(
    title: &str,
    snapshot: Option<&WindowSnapshot>,
    value_item: &MenuItem<Wry>,
    reset_item: &MenuItem<Wry>,
) {
    match snapshot {
        Some(window) => {
            let _ = value_item.set_text(format!("{title}: {}% осталось", window.remaining));

            let _ = reset_item.set_text(format!(
                "Сброс: {}",
                format_duration(seconds_until(window.reset_at))
            ));
        }
        None => {
            let _ = value_item.set_text(format!("{title}: нет данных"));
            let _ = reset_item.set_text("Сброс: неизвестно");
        }
    }
}

pub(crate) fn render(view: &TrayView, snapshot: &UsageSnapshot) {
    update_window(
        "5 часов",
        snapshot.primary.as_ref(),
        &view.five_hour,
        &view.five_hour_reset,
    );

    update_window(
        "Неделя",
        snapshot.secondary.as_ref(),
        &view.weekly,
        &view.weekly_reset,
    );

    let icon_state = snapshot
        .primary
        .as_ref()
        .map(|window| IconState::Remaining(window.remaining))
        .unwrap_or(IconState::Error);

    if let Err(error) = view.tray.set_icon(Some(generate(icon_state))) {
        eprintln!("Ошибка обновления иконки: {error}");
    }
}

pub(crate) fn create_tray(
    app: &tauri::App,
    refresh_tx: mpsc::Sender<()>,
) -> tauri::Result<TrayView> {
    let five_hour = MenuItem::with_id(
        app,
        "five_hour",
        "5 часов: загрузка...",
        false,
        None::<&str>,
    )?;

    let five_hour_reset = MenuItem::with_id(
        app,
        "five_hour_reset",
        "Сброс: неизвестно",
        false,
        None::<&str>,
    )?;

    let weekly = MenuItem::with_id(app, "weekly", "Неделя: загрузка...", false, None::<&str>)?;

    let weekly_reset = MenuItem::with_id(
        app,
        "weekly_reset",
        "Сброс: неизвестно",
        false,
        None::<&str>,
    )?;

    let status = MenuItem::with_id(app, "status", "Ожидание данных", false, None::<&str>)?;

    let open_window =
        MenuItem::with_id(app, "open_window", "Открыть статистику", true, None::<&str>)?;

    let refresh_item = MenuItem::with_id(app, "refresh", "Обновить сейчас", true, None::<&str>)?;

    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

    let weekly_separator = PredefinedMenuItem::separator(app)?;
    let status_separator = PredefinedMenuItem::separator(app)?;
    let actions_separator = PredefinedMenuItem::separator(app)?;
    let quit_separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &five_hour,
            &five_hour_reset,
            &weekly_separator,
            &weekly,
            &weekly_reset,
            &status_separator,
            &status,
            &actions_separator,
            &open_window,
            &refresh_item,
            &quit_separator,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::new()
        .icon(generate(IconState::Error))
        .menu(&menu)
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "open_window" => {
                    if let Some(window) = app.get_webview_window("main") {
                        if let Err(error) = window.show() {
                            eprintln!("Ошибка открытия окна: {error}");
                        }

                        let _ = window.set_focus();
                    } else {
                        eprintln!("Окно с label 'main' не найдено");
                    }
                }

                "refresh" => {
                    // Максимум одно ожидающее обновление.
                    let _ = refresh_tx.try_send(());
                }

                "quit" => {
                    app.exit(0);
                }

                _ => {}
            }
        })
        .build(app)?;

    Ok(TrayView {
        tray,
        five_hour,
        five_hour_reset,
        weekly,
        weekly_reset,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_boundaries_and_past_reset() {
        for (seconds, text) in [
            (0, "Сброс ожидается"),
            (59, "Менее минуты"),
            (60, "1 мин."),
            (3599, "59 мин."),
            (3600, "1 ч. 0 мин."),
            (3660, "1 ч. 1 мин."),
            (86400, "1 д. 0 ч."),
            (90000, "1 д. 1 ч."),
        ] {
            assert_eq!(format_duration(seconds), text);
        }
        assert_eq!(seconds_until(1), 0);
        assert_eq!(seconds_until(i64::MIN), 0);
        assert_eq!(format_duration(seconds_until(1)), "Сброс ожидается");
    }
}
