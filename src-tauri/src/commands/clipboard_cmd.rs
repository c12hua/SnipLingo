use tauri::{AppHandle, Manager, State};
use crate::core::capture::{crop_captured_image, SafeCaptureState, SelectionRect};
use crate::core::clipboard::copy_rgba_image_to_clipboard;

#[tauri::command]
pub fn copy_selection_to_clipboard(
    app: AppHandle,
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
) -> Result<(), String> {
    // 1. 根据物理像素矩形进行无损内存裁剪
    let cropped = {
        let lock = state.lock().map_err(|_| "获取截屏缓存锁失败".to_string())?;
        let capture_state = lock
            .as_ref()
            .ok_or_else(|| "未找到当前的截图底图缓存".to_string())?;
        crop_captured_image(&capture_state.original_image, &rect)?
    };

    // 2. 将位图写入 Windows 系统剪贴板
    copy_rgba_image_to_clipboard(&cropped)?;

    // 3. 隐藏选区窗口并清理截屏底图缓存
    if let Some(win) = app.get_webview_window("capture") {
        let _ = win.hide();
    }
    crate::core::capture::clear_capture_state(&app);

    log::info!(
        "选区图片已写入系统剪贴板 ({} × {} px)",
        cropped.width(),
        cropped.height()
    );

    Ok(())
}

#[tauri::command]
pub fn copy_translated_image_cmd(
    app: AppHandle,
    data_url: String,
) -> Result<(), String> {
    let base64_data = if let Some(idx) = data_url.find(',') {
        &data_url[idx + 1..]
    } else {
        &data_url
    };
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.trim())
        .map_err(|e| format!("Base64 解码失败: {}", e))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("解析图片格式失败: {}", e))?
        .to_rgba8();

    copy_rgba_image_to_clipboard(&img)?;

    if let Some(win) = app.get_webview_window("capture") {
        let _ = win.hide();
    }
    crate::core::capture::clear_capture_state(&app);

    log::info!(
        "合成译图已写入系统剪贴板 ({} × {} px)",
        img.width(),
        img.height()
    );

    Ok(())
}
