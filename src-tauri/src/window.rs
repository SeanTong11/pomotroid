use tauri::{AppHandle, Manager, WebviewWindow};

use crate::widget;

const MAIN_WINDOW_LABEL: &str = "main";

pub fn restore_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window not found".to_string())?;

    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    widget::hide(app);
    Ok(())
}

pub fn set_always_on_top(window: &WebviewWindow, always_on_top: bool, context: &str) {
    if let Err(e) = window.set_always_on_top(always_on_top) {
        log::warn!("[window] failed to set always-on-top={always_on_top} for {context}: {e}");
    }

    #[cfg(target_os = "linux")]
    if always_on_top {
        let window = window.clone();
        let context = context.to_string();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            if let Err(e) = window.set_always_on_top(true) {
                log::warn!("[window] failed delayed always-on-top retry for {context}: {e}");
            }
        });
    }
}
