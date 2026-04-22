use serde_json::Value;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::error::AppError;
use crate::v1_compat::AppHandle;

const STORE_KEY_APP_CONFIG_DIR: &str = "app_config_dir_override";

static APP_CONFIG_DIR_OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

fn override_cache() -> &'static RwLock<Option<PathBuf>> {
    APP_CONFIG_DIR_OVERRIDE.get_or_init(|| RwLock::new(None))
}

fn update_cached_override(value: Option<PathBuf>) {
    if let Ok(mut guard) = override_cache().write() {
        *guard = value;
    }
}

pub fn get_app_config_dir_override() -> Option<PathBuf> {
    override_cache().read().ok()?.clone()
}

fn read_override_from_store(app: &AppHandle) -> Option<PathBuf> {
    let mut store = tauri_plugin_store::StoreBuilder::new(app.clone(), PathBuf::from("app_paths.json"))
        .build();
    if let Err(e) = store.load() {
        log::warn!("无法加载 Store: {e}");
        return None;
    }

    match store.get(STORE_KEY_APP_CONFIG_DIR) {
        Some(Value::String(path_str)) => {
            let path_str = path_str.trim();
            if path_str.is_empty() {
                return None;
            }
            let path = resolve_path(path_str);
            if !path.exists() {
                log::warn!("Store 中配置的 app_config_dir 不存在: {path:?}");
                return None;
            }
            log::info!("使用 Store 中的 app_config_dir: {path:?}");
            Some(path)
        }
        Some(_) => {
            log::warn!("Store 中的 {STORE_KEY_APP_CONFIG_DIR} 类型不正确");
            None
        }
        None => None,
    }
}

pub fn refresh_app_config_dir_override(app: &AppHandle) -> Option<PathBuf> {
    let value = read_override_from_store(app);
    update_cached_override(value.clone());
    value
}

pub fn set_app_config_dir_to_store(
    app: &AppHandle,
    path: Option<&str>,
) -> Result<(), AppError> {
    let mut store = tauri_plugin_store::StoreBuilder::new(app.clone(), PathBuf::from("app_paths.json"))
        .build();
    let _ = store.load();

    match path {
        Some(p) => {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                let _ = store.insert(STORE_KEY_APP_CONFIG_DIR.to_string(), Value::String(trimmed.to_string()));
                log::info!("已将 app_config_dir 写入 Store: {trimmed}");
            } else {
                let _ = store.delete(STORE_KEY_APP_CONFIG_DIR);
            }
        }
        None => {
            let _ = store.delete(STORE_KEY_APP_CONFIG_DIR);
        }
    }

    store
        .save()
        .map_err(|e| AppError::Message(format!("保存 Store 失败: {e}")))?;

    refresh_app_config_dir_override(app);
    Ok(())
}

fn resolve_path(raw: &str) -> PathBuf {
    if raw == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if let Some(stripped) = raw.strip_prefix("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(raw)
}

pub fn migrate_app_config_dir_from_settings(app: &AppHandle) -> Result<(), AppError> {
    log::info!("app_config_dir 迁移功能已移除，请在设置中重新配置");
    let _ = refresh_app_config_dir_override(app);
    Ok(())
}