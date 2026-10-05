use tauri::State;
use crate::core::capture::{capture_for, crop_captured_image, is_current_capture, SafeCaptureState, SelectionRect};
use crate::core::error::SnipLingoError;

#[tauri::command]
pub async fn extract_text_from_selection(
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
    capture_id: u64,
) -> Result<String, SnipLingoError> {
    let t_total = std::time::Instant::now();
    let cropped = {
        let lock = state.lock().map_err(|_| SnipLingoError::CaptureFailed("获取截屏缓存锁失败".to_string()))?;
        let capture = capture_for(&lock, capture_id).map_err(SnipLingoError::CaptureFailed)?;
        crop_captured_image(&capture.original_image, &rect).map_err(SnipLingoError::CaptureFailed)?
    };
    let config = crate::core::config::load_config();
    let text = tokio::task::spawn_blocking(move || {
        let ocr = crate::core::ocr::get_ocr_engine(&config);
        let text = ocr.recognize(&cropped)?;
        if text.trim().is_empty() { return Err(SnipLingoError::NoTextDetected) }
        if !is_current_capture(capture_id) {
            return Err(SnipLingoError::CaptureFailed("截图已结束".to_string()));
        }
        crate::core::clipboard::copy_text_to_clipboard(&text)
            .map_err(SnipLingoError::OcrFailed)?;
        Ok(text)
    }).await.map_err(|e| SnipLingoError::OcrFailed(format!("OCR 线程执行异常: {}", e)))??;

    // 前端提示后通过 cancel_capture 结束会话，提示期间仍允许重选。
    log::debug!("[性能] extract_text_from_selection 全流程耗时: {}ms", t_total.elapsed().as_millis());
    log::info!("OCR 成功识别并提取文本到剪贴板 ({} 字符)", text.chars().count());
    Ok(text)
}
