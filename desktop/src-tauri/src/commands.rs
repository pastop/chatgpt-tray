use tauri::State;
use tokio::sync::mpsc;

use crate::state::{AppState, AppStatus};

#[tauri::command]
pub(crate) fn get_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    state
        .status
        .lock()
        .map(|value| value.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn request_refresh(state: State<'_, AppState>) -> Result<(), String> {
    match state.refresh_tx.try_send(()) {
        Ok(()) | Err(mpsc::error::TrySendError::Full(())) => Ok(()),
        Err(mpsc::error::TrySendError::Closed(())) => Err("Фоновый обработчик остановлен".into()),
    }
}
