use crate::config;
use crate::OverlayState;
use std::sync::Mutex;
use tauri::Manager;

#[tauri::command]
#[specta::specta]
pub fn update_font_size(
    size: u32,
    app: tauri::AppHandle,
    state: tauri::State<Mutex<OverlayState>>,
) -> Result<(), String> {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            return Err(format!("Gagal mengakses direktori data aplikasi: {}", err));
        }
    };

    let updated_config = config::update_config_font_size(&app_data_dir, size)?;

    let mut state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    state_guard.config = Some(updated_config);

    Ok(())
}
