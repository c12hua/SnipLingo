use tauri::{AppHandle, State};
use crate::core::capture::{capture_for, crop_captured_image, end_capture, is_current_capture, SafeCaptureState, SelectionRect};
use crate::core::clipboard::copy_rgba_image_to_clipboard;

#[tauri::command]
pub async fn copy_selection_to_clipboard(
    app: AppHandle,
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
    capture_id: u64,
) -> Result<(), String> {
    let cropped = {
        let lock = state.lock().map_err(|_| "获取截屏缓存锁失败".to_string())?;
        crop_captured_image(&capture_for(&lock, capture_id)?.original_image, &rect)?
    };
    tokio::task::spawn_blocking(move || {
        if !is_current_capture(capture_id) { return Err("截图已结束".to_string()) }
        copy_rgba_image_to_clipboard(&cropped)
    }).await.map_err(|e| format!("复制图片任务失败: {}", e))??;
    end_capture(&app, capture_id, true)
}

#[tauri::command]
pub async fn copy_translated_image_cmd(
    app: AppHandle,
    data_url: String,
    capture_id: u64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if !is_current_capture(capture_id) { return Err("截图已结束".to_string()) }
        let base64_data = data_url.split_once(',').map_or(data_url.as_str(), |(_, data)| data);
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_data.trim())
            .map_err(|e| format!("Base64 解码失败: {}", e))?;
        let img = image::load_from_memory(&bytes)
            .map_err(|e| format!("解析图片格式失败: {}", e))?
            .to_rgba8();
        if !is_current_capture(capture_id) { return Err("截图已结束".to_string()) }
        copy_rgba_image_to_clipboard(&img)
    }).await.map_err(|e| format!("复制图片任务失败: {}", e))??;
    end_capture(&app, capture_id, true)
}
