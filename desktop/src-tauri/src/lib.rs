mod cache;
mod commands;
#[cfg(target_os = "linux")]
mod instance;
mod state;
mod tray;
mod tray_icon;
mod worker;

use std::sync::{Arc, Mutex};

use tauri::Manager;
use tokio::sync::mpsc;

use state::{AppState, AppStatus, SharedStatus};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    let instance_lock = match instance::acquire() {
        Ok(Some(file)) => file,
        Ok(None) => return,
        Err(_) => {
            eprintln!("Не удалось получить блокировку экземпляра приложения.");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::request_refresh
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();

                if let Err(error) = window.hide() {
                    eprintln!("Ошибка скрытия окна: {error}");
                }
            }
        })
        .setup(|app| {
            let (refresh_tx, refresh_rx) = mpsc::channel::<()>(1);
            let shared: SharedStatus = Arc::new(Mutex::new(AppStatus::default()));

            app.manage(AppState {
                status: shared.clone(),
                refresh_tx: refresh_tx.clone(),
            });

            let view = tray::create_tray(app, refresh_tx)?;

            tauri::async_runtime::spawn(worker::background_worker(
                view,
                refresh_rx,
                app.handle().clone(),
                shared,
            ));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Ошибка запуска Tauri");

    #[cfg(target_os = "linux")]
    drop(instance_lock);
}
