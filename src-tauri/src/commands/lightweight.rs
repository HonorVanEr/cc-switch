use crate::v1_compat::AppHandle;

#[tauri::command]
pub fn enter_lightweight_mode(app: AppHandle) -> Result<(), String> {
    crate::lightweight::enter_lightweight_mode(&app)
}

#[tauri::command]
pub fn exit_lightweight_mode(app: AppHandle) -> Result<(), String> {
    crate::lightweight::exit_lightweight_mode(&app)
}

#[tauri::command]
pub fn is_lightweight_mode() -> bool {
    crate::lightweight::is_lightweight_mode()
}