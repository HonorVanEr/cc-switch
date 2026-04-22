//! 托盘菜单管理模块（Tauri v1 兼容）
//!
//! 负责系统托盘图标和菜单的创建、更新和事件处理。
//! v1 使用 SystemTray + CustomMenuItem / Submenu，而非 v2 的 Menu API。

use tauri::{
    CustomMenuItem, Manager, SystemTrayMenu, SystemTrayMenuItem,
    SystemTraySubmenu,
};
use tauri::SystemTray;

use crate::app_config::AppType;
use crate::error::AppError;
use crate::store::AppState;

use crate::v1_compat::{AppHandle, Emitter, get_main_window, unminimize_window};

/// 托盘菜单文本（国际化）
#[derive(Clone, Copy)]
pub struct TrayTexts {
    pub show_main: &'static str,
    pub no_providers_label: &'static str,
    pub lightweight_mode: &'static str,
    pub quit: &'static str,
    pub _auto_label: &'static str,
}

impl TrayTexts {
    pub fn from_language(language: &str) -> Self {
        match language {
            "en" => Self {
                show_main: "Open main window",
                no_providers_label: "(no providers)",
                lightweight_mode: "Lightweight Mode",
                quit: "Quit",
                _auto_label: "Auto (Failover)",
            },
            "ja" => Self {
                show_main: "メインウィンドウを開く",
                no_providers_label: "(プロバイダーなし)",
                lightweight_mode: "軽量モード",
                quit: "終了",
                _auto_label: "自動 (フェイルオーバー)",
            },
            _ => Self {
                show_main: "打开主界面",
                no_providers_label: "(无供应商)",
                lightweight_mode: "轻量模式",
                quit: "退出",
                _auto_label: "自动 (故障转移)",
            },
        }
    }
}

/// 托盘应用分区配置
pub struct TrayAppSection {
    pub app_type: AppType,
    pub prefix: &'static str,
    pub empty_id: &'static str,
    pub header_label: &'static str,
    pub log_name: &'static str,
}

pub const AUTO_SUFFIX: &str = "auto";

pub const TRAY_SECTIONS: [TrayAppSection; 3] = [
    TrayAppSection {
        app_type: AppType::Claude,
        prefix: "claude_",
        empty_id: "claude_empty",
        header_label: "Claude",
        log_name: "Claude",
    },
    TrayAppSection {
        app_type: AppType::Codex,
        prefix: "codex_",
        empty_id: "codex_empty",
        header_label: "Codex",
        log_name: "Codex",
    },
    TrayAppSection {
        app_type: AppType::Gemini,
        prefix: "gemini_",
        empty_id: "gemini_empty",
        header_label: "Gemini",
        log_name: "Gemini",
    },
];

fn sort_providers(
    providers: &indexmap::IndexMap<String, crate::provider::Provider>,
) -> Vec<(&String, &crate::provider::Provider)> {
    let mut sorted: Vec<_> = providers.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| {
        match (a.sort_index, b.sort_index) {
            (Some(idx_a), Some(idx_b)) => return idx_a.cmp(&idx_b),
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        match (a.created_at, b.created_at) {
            (Some(time_a), Some(time_b)) => return time_a.cmp(&time_b),
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            _ => {}
        }

        a.name.cmp(&b.name)
    });
    sorted
}

/// 处理供应商托盘事件
pub fn handle_provider_tray_event(app: &AppHandle, event_id: &str) -> bool {
    for section in TRAY_SECTIONS.iter() {
        if let Some(suffix) = event_id.strip_prefix(section.prefix) {
            if suffix == AUTO_SUFFIX {
                log::info!("切换到{} Auto模式", section.log_name);
                let app_handle = app.clone();
                let app_type = section.app_type.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = handle_auto_click(&app_handle, &app_type) {
                        log::error!("切换{}Auto模式失败: {e}", section.log_name);
                    }
                });
                return true;
            }

            log::info!("切换到{}供应商: {suffix}", section.log_name);
            let app_handle = app.clone();
            let provider_id = suffix.to_string();
            let app_type = section.app_type.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(e) = handle_provider_click(&app_handle, &app_type, &provider_id) {
                    log::error!("切换{}供应商失败: {e}", section.log_name);
                }
            });
            return true;
        }
    }
    false
}

fn handle_auto_click(app: &AppHandle, app_type: &AppType) -> Result<(), AppError> {
    if let Some(app_state) = app.try_state::<AppState>() {
        let app_type_str = app_type.as_str();

        let mut queue = app_state.db.get_failover_queue(app_type_str)?;
        if queue.is_empty() {
            let current_id =
                crate::settings::get_effective_current_provider(&app_state.db, app_type)?;
            let Some(current_id) = current_id else {
                return Err(AppError::Message(
                    "故障转移队列为空，且未设置当前供应商，无法启用 Auto 模式".to_string(),
                ));
            };
            app_state
                .db
                .add_to_failover_queue(app_type_str, &current_id)?;
            queue = app_state.db.get_failover_queue(app_type_str)?;
        }

        let p1_provider_id = queue
            .first()
            .map(|item| item.provider_id.clone())
            .ok_or_else(|| AppError::Message("故障转移队列为空，无法启用 Auto 模式".to_string()))?;

        let proxy_service = &app_state.proxy_service;

        let is_running = futures::executor::block_on(proxy_service.is_running());
        if !is_running {
            log::info!("[Tray] Auto 模式：启动代理服务");
            if let Err(e) = futures::executor::block_on(proxy_service.start()) {
                log::error!("[Tray] 启动代理服务失败: {e}");
                return Err(AppError::Message(format!("启动代理服务失败: {e}")));
            }
        }

        log::info!("[Tray] Auto 模式：对 {app_type_str} 执行接管");
        if let Err(e) =
            futures::executor::block_on(proxy_service.set_takeover_for_app(app_type_str, true))
        {
            log::error!("[Tray] 执行接管失败: {e}");
            return Err(AppError::Message(format!("执行接管失败: {e}")));
        }

        app_state
            .db
            .set_proxy_flags_sync(app_type_str, true, true)?;

        if let Err(e) = futures::executor::block_on(
            proxy_service.switch_proxy_target(app_type_str, &p1_provider_id),
        ) {
            log::error!("[Tray] Auto 模式切换到队列 P1 失败: {e}");
            return Err(AppError::Message(format!(
                "Auto 模式切换到队列 P1 失败: {e}"
            )));
        }

        refresh_tray_menu(app);

        let event_data = serde_json::json!({
            "appType": app_type_str,
            "proxyEnabled": true,
            "autoFailoverEnabled": true,
            "providerId": p1_provider_id
        });
        if let Err(e) = app.emit("proxy-flags-changed", event_data.clone()) {
            log::error!("发射 proxy-flags-changed 事件失败: {e}");
        }
        if let Err(e) = app.emit("provider-switched", event_data) {
            log::error!("发射 provider-switched 事件失败: {e}");
        }
    }
    Ok(())
}

fn handle_provider_click(
    app: &AppHandle,
    app_type: &AppType,
    provider_id: &str,
) -> Result<(), AppError> {
    if let Some(app_state) = app.try_state::<AppState>() {
        let app_type_str = app_type.as_str();

        let (proxy_enabled, _) = app_state.db.get_proxy_flags_sync(app_type_str);
        app_state
            .db
            .set_proxy_flags_sync(app_type_str, proxy_enabled, false)?;

        crate::commands::switch_provider(
            app_state.clone(),
            app_type_str.to_string(),
            provider_id.to_string(),
        )
        .map_err(AppError::Message)?;

        refresh_tray_menu(app);

        let event_data = serde_json::json!({
            "appType": app_type_str,
            "proxyEnabled": proxy_enabled,
            "autoFailoverEnabled": false,
            "providerId": provider_id
        });
        if let Err(e) = app.emit("proxy-flags-changed", event_data.clone()) {
            log::error!("发射 proxy-flags-changed 事件失败: {e}");
        }
        if let Err(e) = app.emit("provider-switched", event_data) {
            log::error!("发射 provider-switched 事件失败: {e}");
        }
    }
    Ok(())
}

/// Create initial SystemTrayMenu for app startup (before AppState is available)
pub fn create_system_tray_menu_template() -> SystemTrayMenu {
    let mut menu = SystemTrayMenu::new();
    let show_main = CustomMenuItem::new("show_main", "打开主界面");
    menu = menu.add_item(show_main);
    menu = menu.add_native_item(SystemTrayMenuItem::Separator);
    let quit_item = CustomMenuItem::new("quit", "退出");
    menu = menu.add_item(quit_item);
    menu
}

/// Create a SystemTrayMenu (Tauri v1 API)
pub fn create_system_tray_menu(
    app: &AppHandle,
    app_state: &AppState,
) -> Result<SystemTrayMenu, AppError> {
    let app_settings = crate::settings::get_settings();
    let tray_texts = TrayTexts::from_language(app_settings.language.as_deref().unwrap_or("zh"));
    let visible_apps = app_settings.visible_apps.unwrap_or_default();

    let mut menu = SystemTrayMenu::new();

    // Show main window
    let show_main = CustomMenuItem::new("show_main", tray_texts.show_main);
    menu = menu.add_item(show_main);
    menu = menu.add_native_item(SystemTrayMenuItem::Separator);

    let is_proxy_running = futures::executor::block_on(app_state.proxy_service.is_running());

    for section in TRAY_SECTIONS.iter() {
        if !visible_apps.is_visible(&section.app_type) {
            continue;
        }

        let app_type_str = section.app_type.as_str();
        let providers = app_state.db.get_all_providers(app_type_str)?;

        let current_id =
            crate::settings::get_effective_current_provider(&app_state.db, &section.app_type)?
                .unwrap_or_default();

        if providers.is_empty() {
            let label = format!("{} {}", section.header_label, tray_texts.no_providers_label);
            let empty_item = CustomMenuItem::new(section.empty_id, label).disabled();
            menu = menu.add_item(empty_item);
        } else {
            let current_name = providers.get(&current_id).map(|p| p.name.as_str());
            let submenu_label = match current_name {
                Some(name) => format!("{} · {}", section.header_label, name),
                None => section.header_label.to_string(),
            };

            let is_app_taken_over = is_proxy_running
                && (futures::executor::block_on(app_state.db.get_live_backup(app_type_str))
                    .ok()
                    .flatten()
                    .is_some()
                    || app_state
                        .proxy_service
                        .detect_takeover_in_live_config_for_app(&section.app_type));

            let mut submenu = SystemTrayMenu::new();

            // Auto item
            let auto_id = format!("{}{}", section.prefix, AUTO_SUFFIX);
            let auto_item = CustomMenuItem::new(&auto_id, tray_texts._auto_label);
            submenu = submenu.add_item(auto_item);

            for (id, provider) in sort_providers(&providers) {
                let is_current = current_id == *id;
                let is_official_blocked =
                    is_app_taken_over && provider.category.as_deref() == Some("official");
                let label = if is_official_blocked {
                    format!("{} ⛔", &provider.name)
                } else if is_current {
                    format!("✓ {}", &provider.name)
                } else {
                    provider.name.clone()
                };
                let mut item = CustomMenuItem::new(format!("{}{}", section.prefix, id), &label);
                if is_official_blocked {
                    item = item.disabled();
                }
                submenu = submenu.add_item(item);
            }

            let submenu = SystemTraySubmenu::new(&submenu_label, submenu);
            menu = menu.add_submenu(submenu);
        }

        menu = menu.add_native_item(SystemTrayMenuItem::Separator);
    }

    // Lightweight mode toggle
    let lw_label = if crate::lightweight::is_lightweight_mode() {
        format!("✓ {}", tray_texts.lightweight_mode)
    } else {
        tray_texts.lightweight_mode.to_string()
    };
    let lightweight_item = CustomMenuItem::new("lightweight_mode", lw_label);
    menu = menu.add_item(lightweight_item);
    menu = menu.add_native_item(SystemTrayMenuItem::Separator);

    // Quit
    let quit_item = CustomMenuItem::new("quit", tray_texts.quit);
    menu = menu.add_item(quit_item);

    Ok(menu)
}

pub fn refresh_tray_menu(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(new_menu) = create_system_tray_menu(app, state.inner()) {
            let tray = app.tray_handle();
            if let Err(e) = tray.set_menu(new_menu) {
                log::error!("刷新托盘菜单失败: {e}");
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn apply_tray_policy(app: &AppHandle, dock_visible: bool) {
    // v1 doesn't have set_activation_policy, but we can use set_dock_visibility
    // This is a no-op on non-macOS platforms
    let desired_policy = if dock_visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };

    if let Err(err) = app.set_dock_visibility(dock_visible) {
        log::warn!("设置 Dock 显示状态失败: {err}");
    }

    if let Err(err) = app.set_activation_policy(desired_policy) {
        log::warn!("设置激活策略失败: {err}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply_tray_policy(_app: &AppHandle, _dock_visible: bool) {
    // no-op on non-macOS
}

/// Handle tray menu events
pub fn handle_tray_menu_event(app: &AppHandle, event_id: &str) {
    log::info!("处理托盘菜单事件: {event_id}");

    match event_id {
        "show_main" => {
            if let Some(window) = get_main_window(app) {
                #[cfg(target_os = "windows")]
                {
                    let _ = window.set_skip_taskbar(false);
                }
                let _ = unminimize_window(&window);
                let _ = window.show();
                let _ = window.set_focus();
                #[cfg(target_os = "linux")]
                {
                    crate::linux_fix::nudge_main_window(window);
                }
                #[cfg(target_os = "macos")]
                {
                    apply_tray_policy(app, true);
                }
            } else if crate::lightweight::is_lightweight_mode() {
                if let Err(e) = crate::lightweight::exit_lightweight_mode(app) {
                    log::error!("退出轻量模式重建窗口失败: {e}");
                }
            }
        }
        "lightweight_mode" => {
            if crate::lightweight::is_lightweight_mode() {
                if let Err(e) = crate::lightweight::exit_lightweight_mode(app) {
                    log::error!("退出轻量模式失败: {e}");
                }
            } else if let Err(e) = crate::lightweight::enter_lightweight_mode(app) {
                log::error!("进入轻量模式失败: {e}");
            }
        }
        "quit" => {
            log::info!("退出应用");
            app.exit(0);
        }
        _ => {
            if handle_provider_tray_event(app, event_id) {
                return;
            }
            log::warn!("未处理的菜单事件: {event_id}");
        }
    }
}