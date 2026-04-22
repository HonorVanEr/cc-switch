//! Linux 专用的主窗口恢复补丁（Tauri v1 兼容）

use std::time::Duration;

use crate::v1_compat::Window;

const REALIZE_WAIT: Duration = Duration::from_millis(200);
const RESIZE_GAP: Duration = Duration::from_millis(100);
const RECONCILE_WAIT: Duration = Duration::from_millis(500);

pub(crate) fn nudge_main_window(window: Window) {
    let _ = window.set_focus();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REALIZE_WAIT).await;

        let _ = window.set_focus();

        // In v1, we use inner_size() which returns Result<PhysicalSize<u32>>
        match window.inner_size() {
            Ok(original) => {
                let bumped = tauri::PhysicalSize::new(original.width.saturating_add(1), original.height);
                let _ = window.set_size(bumped);
                tokio::time::sleep(RESIZE_GAP).await;
                let _ = window.set_size(original);
                log::info!("Linux: 已对主窗口执行 focus + surface 重激活");

                tokio::time::sleep(RECONCILE_WAIT).await;
                match window.inner_size() {
                    Ok(after) => {
                        if after.width != original.width || after.height != original.height {
                            log::info!(
                                "Linux nudge 尺寸 drift: expected={}x{}, got={}x{}，已补偿",
                                original.width,
                                original.height,
                                after.width,
                                after.height
                            );
                            let _ = window.set_size(original);
                            if let Ok(final_size) = window.inner_size() {
                                if final_size.width != original.width
                                    || final_size.height != original.height
                                {
                                    log::warn!(
                                        "Linux nudge 尺寸 drift 补偿后仍不一致: expected={}x{}, got={}x{}",
                                        original.width,
                                        original.height,
                                        final_size.width,
                                        final_size.height
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Linux nudge: 对账回读 inner_size 失败: {e}");
                    }
                }
            }
            Err(e) => {
                log::warn!("Linux nudge: 读取 inner_size 失败，跳过伪 resize: {e}");
            }
        }
    });
}