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

/// 把当前冻结的整屏底图以【原始 RGBA 字节】交给前端铺底。
/// 用原始字节而不是 PNG：省掉编码/解码，4K 屏也能在几十毫秒内铺满遮罩底层。
#[tauri::command]
pub fn get_capture_frame(
    state: tauri::State<crate::core::capture::SafeCaptureState>,
) -> Result<tauri::ipc::Response, String> {
    log::info!(
        "[计时] 前端开始取冻结帧: 自热键 {}ms",
        crate::core::capture::since_capture_start_ms()
    );
    let lock = state.lock().map_err(|_| "获取截屏缓存锁失败".to_string())?;
    let capture = lock
        .as_ref()
        .ok_or_else(|| "未找到当前的截图底图缓存".to_string())?;
    Ok(tauri::ipc::Response::new(
        capture.original_image.as_raw().clone(),
    ))
}

/// 前端把冻结画面铺好之后调用：显示并激活覆盖层。
/// shellInFront=true 表示抓屏瞬间开始菜单/搜索占着前台：后端会请其退场并【确认退场完成后】
/// 才返回，前端据此再淡入，避免"遮罩先出现、系统界面还浮着"的中间态。
#[tauri::command]
pub fn show_capture_overlay(app: AppHandle, shell_in_front: Option<bool>) {
    crate::core::capture::present_capture_window(&app, shell_in_front.unwrap_or(false));
}
