//! Tauri v1 compatibility shim.
//!
//! This module provides type aliases and trait re-exports to make
//! the v2→v1 migration easier. Instead of changing every file,
//! we centralise the API differences here.

pub use tauri::api;
pub use tauri::Manager;

/// In Tauri v1, AppHandle is generic over the Wry runtime.
/// We create a type alias so the rest of the codebase can use `AppHandle`
/// without repeating the generic parameter everywhere.
pub type AppHandle = tauri::AppHandle<tauri::Wry>;

/// Tauri v1 uses `Window<tauri::Wry>` instead of v2's `WebviewWindow`.
/// Since our codebase only ever deals with the main window, we alias it.
pub type Window = tauri::Window<tauri::Wry>;

/// In v1, `State<'_, T>` still works the same way.
pub use tauri::State;

/// Tauri v1's emit is on `AppHandle` directly (via Manager trait),
/// not through a separate `Emitter` trait. The method signature is
/// `app_handle.emit_all(event, payload)` or `app_handle.emit_to(window_label, event, payload)`.
/// In v2, `app.emit(event, payload)` broadcasts to all windows.
/// In v1, `app.emit_all(event, payload)` does the same.
///
/// We provide a trait to mimic the v2 `Emitter` trait for smoother migration.
pub trait Emitter: Sized {
    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), tauri::Error>;
}

impl Emitter for AppHandle {
    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), tauri::Error> {
        self.emit_all(event, payload)
    }
}

/// Helper to get the main window from an AppHandle.
/// In v2: `app.get_webview_window("main")` returns `Option<WebviewWindow>`
/// In v1: `app.get_window("main")` returns `Option<Window<Wry>>`
pub fn get_main_window(app: &AppHandle) -> Option<Window> {
    app.get_window("main")
}

/// In v2, `WebviewWindow::destroy()` closes and destroys the window.
/// In v1, `window.close()` is the equivalent.
pub fn destroy_window(window: &Window) -> Result<(), tauri::Error> {
    window.close()
}

/// In v2, `window.unminimize()` restores a minimized window.
/// In v1, we use `window.set_minimized(false)` or `window.show()`.
pub fn unminimize_window(window: &Window) -> Result<(), tauri::Error> {
    window.unminimize()
}