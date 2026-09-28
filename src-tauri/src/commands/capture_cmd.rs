use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn close_capture(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("capture") {
        #[cfg(target_os = "windows")]
        crate::core::capture::disable_window_animations(&win);
        let _ = win.hide();
    }
    // 注意：不要在此处清空截屏底图缓存！
    // 前端 closeOverlay() 是在 pin_screenshot (钉住) 或 show_result_window (打开新窗口翻译)
    // 之前调用的，必须保留底图缓存供后续指令读取裁剪。
    Ok(())
}

#[tauri::command]
pub fn cancel_capture(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("capture") {
        #[cfg(target_os = "windows")]
        crate::core::capture::disable_window_animations(&win);
        let _ = win.hide();
    }
    crate::core::capture::clear_capture_state(&app);
    Ok(())
}
