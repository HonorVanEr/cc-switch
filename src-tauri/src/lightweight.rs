use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Manager;
use crate::v1_compat::{AppHandle, get_main_window, unminimize_window, destroy_window};

static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);

pub fn enter_lightweight_mode(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(window) = get_main_window(app) {
            let _ = window.set_skip_taskbar(true);
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::tray::apply_tray_policy(app, false);
    }

    if let Some(window) = get_main_window(app) {
        destroy_window(&window)
            .map_err(|e| format!("销毁主窗口失败: {e}"))?;
    }

    LIGHTWEIGHT_MODE.store(true, Ordering::Release);
    crate::tray::refresh_tray_menu(app);
    log::info!("进入轻量模式");
    Ok(())
}

pub fn exit_lightweight_mode(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = get_main_window(app) {
        let _ = unminimize_window(&window);
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "linux")]
        {
            crate::linux_fix::nudge_main_window(window);
        }
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_skip_taskbar(false);
        }
        #[cfg(target_os = "macos")]
        {
            crate::tray::apply_tray_policy(app, true);
        }
        LIGHTWEIGHT_MODE.store(false, Ordering::Release);
        crate::tray::refresh_tray_menu(app);
        log::info!("退出轻量模式");
        return Ok(());
    }

    // In v1, we need to recreate the window differently
    let window = tauri::WindowBuilder::new(app, "main", tauri::WindowUrl::App("index.html".into()))
        .visible(true)
        .build()
        .map_err(|e| format!("创建主窗口失败: {e}"))?;

    let _ = window.set_focus();
    #[cfg(target_os = "linux")]
    {
        crate::linux_fix::nudge_main_window(window);
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(window) = get_main_window(app) {
            let _ = window.set_skip_taskbar(false);
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::tray::apply_tray_policy(app, true);
    }

    LIGHTWEIGHT_MODE.store(false, Ordering::Release);
    crate::tray::refresh_tray_menu(app);
    log::info!("退出轻量模式");
    Ok(())
}

pub fn is_lightweight_mode() -> bool {
    LIGHTWEIGHT_MODE.load(Ordering::Acquire)
}