use tauri::AppHandle;
use crate::core::capture::{capture_for, end_capture};

#[tauri::command]
pub fn close_capture(app: AppHandle, capture_id: u64) -> Result<(), String> {
    // 后续 pin_screenshot / translate_selection 仍需裁剪底图。
    end_capture(&app, capture_id, false)
}

#[tauri::command]
pub fn cancel_capture(app: AppHandle, capture_id: u64) -> Result<(), String> {
    end_capture(&app, capture_id, true)
}

/// 原始 RGBA 传输避免整屏 PNG 编解码；只返回调用者那一轮截图。
#[tauri::command]
pub fn get_capture_frame(
    state: tauri::State<crate::core::capture::SafeCaptureState>,
    capture_id: u64,
) -> Result<tauri::ipc::Response, String> {
    log::info!(
        "[计时] 前端开始取冻结帧: 自热键 {}ms",
        crate::core::capture::since_capture_start_ms()
    );
    let lock = state.lock().map_err(|_| "获取截屏缓存锁失败".to_string())?;
    let capture = capture_for(&lock, capture_id)?;
    Ok(tauri::ipc::Response::new(capture.original_image.as_raw().clone()))
}

/// 开始菜单退场与前台激活保留原有兜底，但不能呈现已过期的截图。
#[tauri::command]
pub fn show_capture_overlay(app: AppHandle, capture_id: u64, shell_in_front: Option<bool>) {
    crate::core::capture::present_capture_window(&app, capture_id, shell_in_front.unwrap_or(false));
}
