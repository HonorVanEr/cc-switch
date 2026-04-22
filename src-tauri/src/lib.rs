mod app_config;
mod app_store;
mod auto_launch;
mod claude_mcp;
mod claude_plugin;
mod codex_config;
mod commands;
mod config;
mod database;
mod deeplink;
mod error;
mod gemini_config;
mod gemini_mcp;
mod init_status;
mod lightweight;
#[cfg(target_os = "linux")]
mod linux_fix;
mod mcp;
mod openclaw_config;
mod opencode_config;
mod panic_hook;
mod prompt;
mod prompt_files;
mod provider;
mod provider_defaults;
mod proxy;
mod services;
mod session_manager;
mod settings;
mod store;

mod tray;
mod usage_script;
mod v1_compat;

pub use app_config::{AppType, InstalledSkill, McpApps, McpServer, MultiAppConfig, SkillApps};
pub use codex_config::{get_codex_auth_path, get_codex_config_path, write_codex_live_atomic};
pub use commands::open_provider_terminal;
pub use commands::*;
pub use config::{get_claude_mcp_path, get_claude_settings_path, read_json_file};
pub use database::Database;
pub use deeplink::{import_provider_from_deeplink, parse_deeplink_url, DeepLinkImportRequest};
pub use error::AppError;
pub use mcp::{
    import_from_claude, import_from_codex, import_from_gemini, remove_server_from_claude,
    remove_server_from_codex, remove_server_from_gemini, sync_enabled_to_claude,
    sync_enabled_to_codex, sync_enabled_to_gemini, sync_single_server_to_claude,
    sync_single_server_to_codex, sync_single_server_to_gemini,
};
pub use provider::{Provider, ProviderMeta};
pub use services::{
    skill::{migrate_skills_to_ssot, ImportSkillSelection},
    ConfigService, EndpointLatency, McpService, PromptService, ProviderService, ProxyService,
    SkillService, SpeedtestService,
};
pub use settings::{update_settings, AppSettings};
pub use store::AppState;
use std::sync::Arc;
#[cfg(target_os = "macos")]
use tauri::image::Image;
use v1_compat::{AppHandle, Emitter, Manager as V1Manager, Window, get_main_window, unminimize_window, destroy_window};

// In Tauri v1, dialog is part of tauri::api::dialog
use tauri::api::dialog::{MessageDialogBuilder, MessageDialogButtons, MessageDialogKind};

// v1 does not have deep-link as a plugin; we handle URL events in RunEvent
// use tauri_plugin_deep_link::DeepLinkExt;

fn redact_url_for_log(url_str: &str) -> String {
    match url::Url::parse(url_str) {
        Ok(url) => {
            let mut output = format!("{}://", url.scheme());
            if let Some(host) = url.host_str() {
                output.push_str(host);
            }
            output.push_str(url.path());

            let mut keys: Vec<String> = url.query_pairs().map(|(k, _)| k.to_string()).collect();
            keys.sort();
            keys.dedup();

            if !keys.is_empty() {
                output.push_str("?[keys:");
                output.push_str(&keys.join(","));
                output.push(']');
            }

            output
        }
        Err(_) => {
            let base = url_str.split('#').next().unwrap_or(url_str);
            match base.split_once('?') {
                Some((prefix, _)) => format!("{prefix}?[redacted]"),
                None => base.to_string(),
            }
        }
    }
}

/// 统一处理 ccswitch:// 深链接 URL
///
/// - 解析 URL
/// - 向前端发射 `deeplink-import` / `deeplink-error` 事件
/// - 可选：在成功时聚焦主窗口
fn handle_deeplink_url(
    app: &AppHandle,
    url_str: &str,
    focus_main_window: bool,
    source: &str,
) -> bool {
    if !url_str.starts_with("ccswitch://") {
        return false;
    }

    let redacted_url = redact_url_for_log(url_str);
    log::info!("✓ Deep link URL detected from {source}: {redacted_url}");
    log::debug!("Deep link URL (raw) from {source}: {url_str}");

    match crate::deeplink::parse_deeplink_url(url_str) {
        Ok(request) => {
            log::info!(
                "✓ Successfully parsed deep link: resource={}, app={:?}, name={:?}",
                request.resource,
                request.app,
                request.name
            );

            if let Err(e) = app.emit("deeplink-import", &request) {
                log::error!("✗ Failed to emit deeplink-import event: {e}");
            } else {
                log::info!("✓ Emitted deeplink-import event to frontend");
            }

            if focus_main_window {
                if let Some(window) = get_main_window(app) {
                    let _ = unminimize_window(&window);
                    let _ = window.show();
                    let _ = window.set_focus();
                    #[cfg(target_os = "linux")]
                    {
                        linux_fix::nudge_main_window(window);
                    }
                    log::info!("✓ Window shown and focused");
                }
            }
        }
        Err(e) => {
            log::error!("✗ Failed to parse deep link URL: {e}");

            if let Err(emit_err) = app.emit(
                "deeplink-error",
                serde_json::json!({
                    "url": url_str,
                    "error": e.to_string()
                }),
            ) {
                log::error!("✗ Failed to emit deeplink-error event: {emit_err}");
            }
        }
    }

    true
}

/// 更新托盘菜单的Tauri命令
#[tauri::command]
async fn update_tray_menu(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    match tray::create_system_tray_menu(&app, state.inner()) {
        Ok(new_menu) => {
            let tray = app.tray_handle();
            tray.set_menu(new_menu)
                .map_err(|e| format!("更新托盘菜单失败: {e}"))?;
            return Ok(true);
        }
        Err(err) => {
            log::error!("创建托盘菜单失败: {err}");
            Ok(false)
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_tray_icon() -> Option<tauri::image::Image<'static>> {
    const ICON_BYTES: &[u8] = include_bytes!("../icons/tray/macos/statusbar_template_3x.png");

    tauri::image::Image::from_bytes(ICON_BYTES).ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    panic_hook::setup_panic_hook();

    let mut builder = tauri::Builder::default();

    // Single instance plugin
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {
            log::info!("Single instance callback triggered");
        }));
    }

    let system_tray_menu = tray::create_system_tray_menu_template();
    let system_tray = tauri::SystemTray::new()
        .with_menu(system_tray_menu)
        .with_tooltip("CC Switch");

    let app = builder
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| {
            if let tauri::SystemTrayEvent::MenuItemClick { id, .. } = event {
                tray::handle_tray_menu_event(app, &id);
            }
        })
        .on_window_event(|event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event.event() {
                let settings = crate::settings::get_settings();
                if settings.minimize_to_tray_on_close {
                    api.prevent_close();
                    let window = event.window();
                    let _ = window.hide();
                    #[cfg(target_os = "windows")]
                    { let _ = window.set_skip_taskbar(true); }
                    #[cfg(target_os = "macos")]
                    { tray::apply_tray_policy(&window.app_handle(), false); }
                } else {
                    event.window().app_handle().exit(0);
                }
            }
        })
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app: &mut tauri::App<tauri::Wry>| {
            app_store::refresh_app_config_dir_override(&app.handle());
            panic_hook::init_app_config_dir(crate::config::get_app_config_dir());

            {
                let log_dir = panic_hook::get_log_dir();
                let _ = std::fs::create_dir_all(&log_dir);
                let _ = std::fs::remove_file(log_dir.join("cc-switch.log"));
                #[cfg(debug_assertions)]
                { let _ = env_logger::Builder::from_default_env().filter_level(log::LevelFilter::Trace).try_init(); }
                #[cfg(not(debug_assertions))]
                { let _ = env_logger::Builder::new().filter_level(log::LevelFilter::Info).try_init(); }
            }

            let app_config_dir = crate::config::get_app_config_dir();
            let db_path = app_config_dir.join("cc-switch.db");
            let json_path = app_config_dir.join("config.json");
            let has_json = json_path.exists();
            let has_db = db_path.exists();

            let migration_config = if !has_db && has_json {
                loop {
                    match crate::app_config::MultiAppConfig::load() {
                        Ok(config) => break Some(config),
                        Err(e) => {
                            log::error!("加载旧配置文件失败: {e}");
                            if !show_migration_error_dialog(&app.handle(), &e.to_string()) {
                                std::process::exit(1);
                            }
                        }
                    }
                }
            } else { None };

            let db = loop {
                match crate::database::Database::init() {
                    Ok(db) => break Arc::new(db),
                    Err(e) => {
                        log::error!("Failed to init database: {e}");
                        if !show_database_init_error_dialog(&app.handle(), &db_path, &e.to_string()) {
                            std::process::exit(1);
                        }
                    }
                }
            };

            if let Some(config) = migration_config {
                match db.migrate_from_json(&config) {
                    Ok(_) => { crate::init_status::set_migration_success(); let _ = std::fs::rename(&json_path, json_path.with_extension("json.migrated")); }
                    Err(e) => { log::error!("配置迁移失败: {e}"); }
                }
            }

            let app_state = AppState::new(db);
            app_state.proxy_service.set_app_handle(app.handle().clone());

            // Initialize default skills repos, import providers, MCP, prompts etc.
            let _ = app_state.db.init_default_skill_repos();
            for app_type in crate::app_config::AppType::all().filter(|t| !t.is_additive_mode()) {
                let _ = crate::services::provider::import_default_config(&app_state, app_type.clone());
            }
            let _ = app_state.db.init_default_official_providers();
            let _ = crate::services::provider::import_opencode_providers_from_live(&app_state);
            let _ = crate::services::provider::import_openclaw_providers_from_live(&app_state);
            let _ = crate::services::McpService::import_from_claude(&app_state);
            let _ = crate::services::McpService::import_from_codex(&app_state);
            let _ = crate::services::McpService::import_from_gemini(&app_state);

            if let Err(e) = app_store::migrate_app_config_dir_from_settings(&app.handle()) {
                log::warn!("迁移 app_config_dir 失败: {e}");
            }

            app.manage(app_state);

            let skill_service = SkillService::new();
            app.manage(commands::skill::SkillServiceState(Arc::new(skill_service)));

            {
                use crate::proxy::providers::copilot_auth::CopilotAuthManager;
                use commands::CopilotAuthState;
                use tokio::sync::RwLock;
                let copilot_auth_manager = CopilotAuthManager::new(app_config_dir.clone());
                app.manage(CopilotAuthState(Arc::new(RwLock::new(copilot_auth_manager))));
            }

            {
                use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
                use commands::CodexOAuthState;
                use tokio::sync::RwLock;
                let codex_oauth_manager = CodexOAuthManager::new(app_config_dir.clone());
                app.manage(CodexOAuthState(Arc::new(RwLock::new(codex_oauth_manager))));
            }

            #[cfg(target_os = "linux")]
            {
                if let Some(window) = get_main_window(&app.handle()) {
                    let _ = window.with_webview(|webview| {
                        use webkit2gtk::{WebViewExt, SettingsExt};
                        let wk_webview = webview.inner();
                        if let Some(settings) = wk_webview.settings() {
                            settings.set_hardware_acceleration_policy(webkit2gtk::HardwareAccelerationPolicy::Never);
                            log::info!("已禁用 WebKitGTK 硬件加速");
                        }
                    });
                }
            }

            let settings = crate::settings::get_settings();
            if let Some(window) = get_main_window(&app.handle()) {
                #[cfg(target_os = "linux")]
                let _ = window.set_decorations(!settings.use_app_window_controls);
                if settings.silent_startup {
                    let _ = window.hide();
                    #[cfg(target_os = "windows")]
                    let _ = window.set_skip_taskbar(true);
                    #[cfg(target_os = "macos")]
                    tray::apply_tray_policy(&app.handle(), false);
                } else {
                    let _ = window.show();
                    #[cfg(target_os = "linux")]
                    { crate::linux_fix::nudge_main_window(window); }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_providers,
            commands::get_current_provider,
            commands::add_provider,
            commands::update_provider,
            commands::delete_provider,
            commands::remove_provider_from_live_config,
            commands::switch_provider,
            commands::import_default_config,
            commands::get_claude_config_status,
            commands::get_config_status,
            commands::get_claude_code_config_path,
            commands::get_config_dir,
            commands::open_config_folder,
            commands::pick_directory,
            commands::open_external,
            commands::get_init_error,
            commands::get_migration_result,
            commands::get_skills_migration_result,
            commands::get_app_config_path,
            commands::open_app_config_folder,
            commands::get_claude_common_config_snippet,
            commands::set_claude_common_config_snippet,
            commands::get_common_config_snippet,
            commands::set_common_config_snippet,
            commands::extract_common_config_snippet,
            commands::read_live_provider_settings,
            commands::get_settings,
            commands::save_settings,
            commands::get_rectifier_config,
            commands::set_rectifier_config,
            commands::get_optimizer_config,
            commands::set_optimizer_config,
            commands::get_copilot_optimizer_config,
            commands::set_copilot_optimizer_config,
            commands::get_log_config,
            commands::set_log_config,
            commands::restart_app,
            commands::check_for_updates,
            commands::is_portable_mode,
            commands::copy_text_to_clipboard,
            commands::get_claude_plugin_status,
            commands::read_claude_plugin_config,
            commands::apply_claude_plugin_config,
            commands::is_claude_plugin_applied,
            commands::apply_claude_onboarding_skip,
            commands::clear_claude_onboarding_skip,
            commands::get_claude_mcp_status,
            commands::read_claude_mcp_config,
            commands::upsert_claude_mcp_server,
            commands::delete_claude_mcp_server,
            commands::validate_mcp_command,
            commands::queryProviderUsage,
            commands::testUsageScript,
            commands::get_subscription_quota,
            commands::get_codex_oauth_quota,
            commands::get_coding_plan_quota,
            commands::get_balance,
            commands::get_mcp_config,
            commands::upsert_mcp_server_in_config,
            commands::delete_mcp_server_in_config,
            commands::set_mcp_enabled,
            commands::get_mcp_servers,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::toggle_mcp_app,
            commands::import_mcp_from_apps,
            commands::get_prompts,
            commands::upsert_prompt,
            commands::delete_prompt,
            commands::enable_prompt,
            commands::import_prompt_from_file,
            commands::get_current_prompt_file_content,
            commands::fetch_models_for_config,
            commands::test_api_endpoints,
            commands::get_custom_endpoints,
            commands::add_custom_endpoint,
            commands::remove_custom_endpoint,
            commands::update_endpoint_last_used,
            commands::get_app_config_dir_override,
            commands::set_app_config_dir_override,
            commands::update_providers_sort_order,
            commands::export_config_to_file,
            commands::import_config_from_file,
            commands::webdav_test_connection,
            commands::webdav_sync_upload,
            commands::webdav_sync_download,
            commands::webdav_sync_save_settings,
            commands::webdav_sync_fetch_remote_info,
            commands::save_file_dialog,
            commands::open_file_dialog,
            commands::open_zip_file_dialog,
            commands::create_db_backup,
            commands::list_db_backups,
            commands::restore_db_backup,
            commands::rename_db_backup,
            commands::delete_db_backup,
            commands::sync_current_providers_live,
            commands::parse_deeplink,
            commands::merge_deeplink_config,
            commands::import_from_deeplink,
            commands::import_from_deeplink_unified,
            update_tray_menu,
            commands::check_env_conflicts,
            commands::delete_env_vars,
            commands::restore_env_backup,
            commands::get_installed_skills,
            commands::get_skill_backups,
            commands::delete_skill_backup,
            commands::install_skill_unified,
            commands::uninstall_skill_unified,
            commands::restore_skill_backup,
            commands::toggle_skill_app,
            commands::scan_unmanaged_skills,
            commands::import_skills_from_apps,
            commands::discover_available_skills,
            commands::check_skill_updates,
            commands::update_skill,
            commands::migrate_skill_storage,
            commands::search_skills_sh,
            commands::get_skills,
            commands::get_skills_for_app,
            commands::install_skill,
            commands::install_skill_for_app,
            commands::uninstall_skill,
            commands::uninstall_skill_for_app,
            commands::get_skill_repos,
            commands::add_skill_repo,
            commands::remove_skill_repo,
            commands::install_skills_from_zip,
            commands::set_auto_launch,
            commands::get_auto_launch_status,
            commands::start_proxy_server,
            commands::stop_proxy_with_restore,
            commands::get_proxy_takeover_status,
            commands::set_proxy_takeover_for_app,
            commands::get_proxy_status,
            commands::get_proxy_config,
            commands::update_proxy_config,
            commands::get_global_proxy_config,
            commands::update_global_proxy_config,
            commands::get_proxy_config_for_app,
            commands::update_proxy_config_for_app,
            commands::get_default_cost_multiplier,
            commands::set_default_cost_multiplier,
            commands::get_pricing_model_source,
            commands::set_pricing_model_source,
            commands::is_proxy_running,
            commands::is_live_takeover_active,
            commands::switch_proxy_provider,
            commands::get_provider_health,
            commands::reset_circuit_breaker,
            commands::get_circuit_breaker_config,
            commands::update_circuit_breaker_config,
            commands::get_circuit_breaker_stats,
            commands::get_failover_queue,
            commands::get_available_providers_for_failover,
            commands::add_to_failover_queue,
            commands::remove_from_failover_queue,
            commands::get_auto_failover_enabled,
            commands::set_auto_failover_enabled,
            commands::get_usage_summary,
            commands::get_usage_trends,
            commands::get_provider_stats,
            commands::get_model_stats,
            commands::get_request_logs,
            commands::get_request_detail,
            commands::get_model_pricing,
            commands::update_model_pricing,
            commands::delete_model_pricing,
            commands::check_provider_limits,
            commands::sync_session_usage,
            commands::get_usage_data_sources,
            commands::stream_check_provider,
            commands::stream_check_all_providers,
            commands::get_stream_check_config,
            commands::save_stream_check_config,
            commands::list_sessions,
            commands::get_session_messages,
            commands::delete_session,
            commands::delete_sessions,
            commands::launch_session_terminal,
            commands::get_tool_versions,
            commands::open_provider_terminal,
            commands::get_universal_providers,
            commands::get_universal_provider,
            commands::upsert_universal_provider,
            commands::delete_universal_provider,
            commands::sync_universal_provider,
            commands::import_opencode_providers_from_live,
            commands::get_opencode_live_provider_ids,
            commands::import_openclaw_providers_from_live,
            commands::get_openclaw_live_provider_ids,
            commands::get_openclaw_live_provider,
            commands::scan_openclaw_config_health,
            commands::get_openclaw_default_model,
            commands::set_openclaw_default_model,
            commands::get_openclaw_model_catalog,
            commands::set_openclaw_model_catalog,
            commands::get_openclaw_agents_defaults,
            commands::set_openclaw_agents_defaults,
            commands::get_openclaw_env,
            commands::set_openclaw_env,
            commands::get_openclaw_tools,
            commands::set_openclaw_tools,
            commands::get_global_proxy_url,
            commands::set_global_proxy_url,
            commands::test_proxy_url,
            commands::get_upstream_proxy_status,
            commands::scan_local_proxies,
            commands::set_window_theme,
            commands::auth_start_login,
            commands::auth_poll_for_account,
            commands::auth_list_accounts,
            commands::auth_get_status,
            commands::auth_remove_account,
            commands::auth_set_default_account,
            commands::auth_logout,
            commands::copilot_start_device_flow,
            commands::copilot_poll_for_auth,
            commands::copilot_poll_for_account,
            commands::copilot_list_accounts,
            commands::copilot_remove_account,
            commands::copilot_set_default_account,
            commands::copilot_get_auth_status,
            commands::copilot_logout,
            commands::copilot_is_authenticated,
            commands::copilot_get_token,
            commands::copilot_get_token_for_account,
            commands::copilot_get_models,
            commands::copilot_get_models_for_account,
            commands::copilot_get_usage,
            commands::copilot_get_usage_for_account,
            commands::read_omo_local_file,
            commands::get_current_omo_provider_id,
            commands::disable_current_omo,
            commands::read_omo_slim_local_file,
            commands::get_current_omo_slim_provider_id,
            commands::disable_current_omo_slim,
            commands::read_workspace_file,
            commands::write_workspace_file,
            commands::list_daily_memory_files,
            commands::read_daily_memory_file,
            commands::write_daily_memory_file,
            commands::delete_daily_memory_file,
            commands::search_daily_memory_files,
            commands::open_workspace_directory,
            commands::enter_lightweight_mode,
            commands::exit_lightweight_mode,
            commands::is_lightweight_mode,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle: &AppHandle, event| {
        match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                // 阻止自动退出以保持托盘后台运行
                api.prevent_exit();
            }
            tauri::RunEvent::Exit => {
                log::info!("应用正在退出...");
                let app_handle = app_handle.clone();
                // v1: 在 Exit 事件中做清理
                // 注意 v1 的 RunEvent::Exit 是最终的，不能异步等待
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                if let Some(window) = get_main_window(app_handle) {
                    #[cfg(target_os = "windows")]
                    {
                        let _ = window.set_skip_taskbar(false);
                    }
                    let _ = unminimize_window(&window);
                    let _ = window.show();
                    let _ = window.set_focus();
                    tray::apply_tray_policy(app_handle, true);
                } else if crate::lightweight::is_lightweight_mode() {
                    if let Err(e) = crate::lightweight::exit_lightweight_mode(app_handle) {
                        log::error!("退出轻量模式重建窗口失败: {e}");
                    }
                }
            }
            _ => {}
        }
    });
}

// ============================================================
// 应用退出清理
// ============================================================

/// 应用退出前的清理工作
///
/// 在应用退出前检查代理服务器状态，如果正在运行则停止代理并恢复 Live 配置。
/// 确保 Claude Code/Codex/Gemini 的配置不会处于损坏状态。
/// 使用 stop_with_restore_keep_state 保留 settings 表中的代理状态，下次启动时自动恢复。
pub async fn cleanup_before_exit(app_handle: &AppHandle) {
    if let Some(state) = app_handle.try_state::<store::AppState>() {
        let proxy_service = &state.proxy_service;

        // 退出时也需要兜底：代理可能已崩溃/未运行，但 Live 接管残留仍在（占位符/备份）。
        let has_backups = match state.db.has_any_live_backup().await {
            Ok(v) => v,
            Err(e) => {
                log::error!("退出时检查 Live 备份失败: {e}");
                false
            }
        };
        let live_taken_over = proxy_service.detect_takeover_in_live_configs();
        let needs_restore = has_backups || live_taken_over;

        if needs_restore {
            log::info!("检测到接管残留，开始恢复 Live 配置（保留代理状态）...");
            // 使用 keep_state 版本，保留 settings 表中的代理状态
            if let Err(e) = proxy_service.stop_with_restore_keep_state().await {
                log::error!("退出时恢复 Live 配置失败: {e}");
            } else {
                log::info!("已恢复 Live 配置（代理状态已保留，下次启动将自动恢复）");
            }
            return;
        }

        // 非接管模式：代理在运行则仅停止代理
        if proxy_service.is_running().await {
            log::info!("检测到代理服务器正在运行，开始停止...");
            if let Err(e) = proxy_service.stop().await {
                log::error!("退出时停止代理失败: {e}");
            }
            log::info!("代理服务器清理完成");
        }
    }
}

// ============================================================
// 启动时恢复代理状态
// ============================================================

/// 启动时根据 proxy_config 表中的代理状态自动恢复代理服务
///
/// 检查 `proxy_config.enabled` 字段，如果有任一应用的状态为 `true`，
/// 则自动启动代理服务并接管对应应用的 Live 配置。
async fn restore_proxy_state_on_startup(state: &store::AppState) {
    // 收集需要恢复接管的应用列表（从 proxy_config.enabled 读取）
    let mut apps_to_restore = Vec::new();
    for app_type in ["claude", "codex", "gemini"] {
        if let Ok(config) = state.db.get_proxy_config_for_app(app_type).await {
            if config.enabled {
                apps_to_restore.push(app_type);
            }
        }
    }

    if apps_to_restore.is_empty() {
        log::debug!("启动时无需恢复代理状态");
        return;
    }

    log::info!("检测到上次代理状态需要恢复，应用列表: {apps_to_restore:?}");

    // 逐个恢复接管状态
    for app_type in apps_to_restore {
        match state
            .proxy_service
            .set_takeover_for_app(app_type, true)
            .await
        {
            Ok(()) => {
                log::info!("✓ 已恢复 {app_type} 的代理接管状态");
            }
            Err(e) => {
                log::error!("✗ 恢复 {app_type} 的代理接管状态失败: {e}");
                // 失败时清除该应用的状态，避免下次启动再次尝试
                if let Err(clear_err) = state
                    .proxy_service
                    .set_takeover_for_app(app_type, false)
                    .await
                {
                    log::error!("清除 {app_type} 代理状态失败: {clear_err}");
                }
            }
        }
    }
}

fn initialize_common_config_snippets(state: &store::AppState) {
    // Auto-extract common config snippets from clean live files when snippet is missing.
    // This must run before proxy takeover is restored on startup, otherwise we'd read
    // proxy-placeholder configs instead of the user's actual live settings.
    for app_type in crate::app_config::AppType::all() {
        if !state
            .db
            .should_auto_extract_config_snippet(app_type.as_str())
            .unwrap_or(false)
        {
            continue;
        }

        let settings = match crate::services::provider::ProviderService::read_live_settings(
            app_type.clone(),
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };

        match crate::services::provider::ProviderService::extract_common_config_snippet_from_settings(
            app_type.clone(),
            &settings,
        ) {
            Ok(snippet) if !snippet.is_empty() && snippet != "{}" => {
                match state.db.set_config_snippet(app_type.as_str(), Some(snippet)) {
                    Ok(()) => {
                        let _ = state.db.set_config_snippet_cleared(app_type.as_str(), false);
                        log::info!(
                            "✓ Auto-extracted common config snippet for {}",
                            app_type.as_str()
                        );
                    }
                    Err(e) => log::warn!(
                        "✗ Failed to save config snippet for {}: {e}",
                        app_type.as_str()
                    ),
                }
            }
            Ok(_) => log::debug!(
                "○ Live config for {} has no extractable common fields",
                app_type.as_str()
            ),
            Err(e) => log::warn!(
                "✗ Failed to extract config snippet for {}: {e}",
                app_type.as_str()
            ),
        }
    }

    let should_run_legacy_migration = state
        .db
        .is_legacy_common_config_migrated()
        .map(|done| !done)
        .unwrap_or(true);

    if should_run_legacy_migration {
        for app_type in [
            crate::app_config::AppType::Claude,
            crate::app_config::AppType::Codex,
            crate::app_config::AppType::Gemini,
        ] {
            if let Err(e) = crate::services::provider::ProviderService::migrate_legacy_common_config_usage_if_needed(
                state,
                app_type.clone(),
            ) {
                log::warn!(
                    "✗ Failed to migrate legacy common-config usage for {}: {e}",
                    app_type.as_str()
                );
            }
        }

        if let Err(e) = state.db.set_legacy_common_config_migrated(true) {
            log::warn!("✗ Failed to persist legacy common-config migration flag: {e}");
        }
    }
}

// ============================================================
// 迁移错误对话框辅助函数
// ============================================================

/// 检测是否为中文环境
fn is_chinese_locale() -> bool {
    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .map(|lang| lang.starts_with("zh"))
        .unwrap_or(false)
}

/// 显示迁移错误对话框
/// 返回 true 表示用户选择重试，false 表示用户选择退出
fn show_migration_error_dialog(app: &AppHandle, error: &str) -> bool {
    let title = if is_chinese_locale() {
        "配置迁移失败"
    } else {
        "Migration Failed"
    };

    let message = if is_chinese_locale() {
        format!(
            "从旧版本迁移配置时发生错误：\n\n{error}\n\n\
            您的数据尚未丢失，旧配置文件仍然保留。\n\
            建议回退到旧版本 CC Switch 以保护数据。\n\n\
            点击「重试」重新尝试迁移\n\
            点击「退出」关闭程序（可回退版本后重新打开）"
        )
    } else {
        format!(
            "An error occurred while migrating configuration:\n\n{error}\n\n\
            Your data is NOT lost - the old config file is still preserved.\n\
            Consider rolling back to an older CC Switch version.\n\n\
            Click 'Retry' to attempt migration again\n\
            Click 'Exit' to close the program"
        )
    };

    let retry_text = if is_chinese_locale() { "重试" } else { "Retry" };
    let exit_text = if is_chinese_locale() { "退出" } else { "Exit" };

    // Tauri v1: use tauri::api::dialog::blocking::ask (returns true for Yes/Ok)
    // We map "Retry" to true and "Exit" to false
    // Since v1 dialog API is limited, we use message dialog with OkCancel
    tauri::api::dialog::blocking::MessageDialogBuilder::new(title, &message)
        .kind(tauri::api::dialog::MessageDialogKind::Error)
        .buttons(tauri::api::dialog::MessageDialogButtons::OkCancel)
        .show()
}

/// 显示数据库初始化/Schema 迁移失败对话框
/// 返回 true 表示用户选择重试，false 表示用户选择退出
fn show_database_init_error_dialog(
    app: &AppHandle,
    db_path: &std::path::Path,
    error: &str,
) -> bool {
    let title = if is_chinese_locale() {
        "数据库初始化失败"
    } else {
        "Database Initialization Failed"
    };

    let message = if is_chinese_locale() {
        format!(
            "初始化数据库或迁移数据库结构时发生错误：\n\n{error}\n\n\
            数据库文件路径：\n{db}\n\n\
            您的数据尚未丢失，应用不会自动删除数据库文件。\n\
            常见原因包括：数据库版本过新、文件损坏、权限不足、磁盘空间不足等。\n\n\
            建议：\n\
            1) 先备份整个配置目录（包含 cc-switch.db）\n\
            2) 如果提示\"数据库版本过新\"，请升级到更新版本\n\
            3) 如果刚升级出现异常，可回退旧版本导出/备份后再升级\n\n\
            点击「重试」重新尝试初始化\n\
            点击「退出」关闭程序",
            db = db_path.display()
        )
    } else {
        format!(
            "An error occurred while initializing or migrating the database:\n\n{error}\n\n\
            Database file path:\n{db}\n\n\
            Your data is NOT lost - the app will not delete the database automatically.\n\
            Common causes include: newer database version, corrupted file, permission issues, or low disk space.\n\n\
            Suggestions:\n\
            1) Back up the entire config directory (including cc-switch.db)\n\
            2) If you see \"database version is newer\", please upgrade CC Switch\n\
            3) If this happened right after upgrading, consider rolling back to export/backup then upgrade again\n\n\
            Click 'Retry' to attempt initialization again\n\
            Click 'Exit' to close the program",
            db = db_path.display()
        )
    };

    let retry_text = if is_chinese_locale() { "重试" } else { "Retry" };
    let exit_text = if is_chinese_locale() { "退出" } else { "Exit" };

    tauri::api::dialog::blocking::MessageDialogBuilder::new(title, &message)
        .kind(tauri::api::dialog::MessageDialogKind::Error)
        .buttons(tauri::api::dialog::MessageDialogButtons::OkCancel)
        .show()
}
