use tauri::State;
use crate::core::capture::{crop_captured_image, SafeCaptureState, SelectionRect};
use crate::core::error::SnipLingoError;

#[tauri::command]
pub async fn extract_text_from_selection(
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
) -> Result<String, SnipLingoError> {
    let t_total = std::time::Instant::now();

    // 1. 获取锁并取得内存中的无损底图并裁剪选区
    let t_crop = std::time::Instant::now();
    let cropped = {
        let lock = state.lock().map_err(|_| {
            SnipLingoError::CaptureFailed("获取截屏缓存锁失败".to_string())
        })?;
        let capture_state = lock
            .as_ref()
            .ok_or_else(|| SnipLingoError::CaptureFailed("未找到当前的截图底图缓存".to_string()))?;
        crop_captured_image(&capture_state.original_image, &rect)
            .map_err(SnipLingoError::CaptureFailed)?
    };
    log::debug!("[性能] extract_text 选区裁剪耗时: {}ms (尺寸: {}x{})", t_crop.elapsed().as_millis(), cropped.width(), cropped.height());

    // 2. 将 CPU 密集型的 OCR 识别移至 blocking worker 线程 (Stage 2)
    let config = crate::core::config::load_config();
    let text = tokio::task::spawn_blocking(move || {
        let ocr = crate::core::ocr::get_ocr_engine(&config);
        ocr.recognize(&cropped)
    })
    .await
    .map_err(|e| SnipLingoError::OcrFailed(format!("OCR 线程执行异常: {}", e)))??;

    if text.trim().is_empty() {
        return Err(SnipLingoError::NoTextDetected);
    }

    // 3. 将提取到的文本自动写入系统剪贴板 (原生可靠)
    if let Err(e) = crate::core::clipboard::copy_text_to_clipboard(&text) {
        log::warn!("写入系统剪贴板失败: {}", e);
    }

    log::debug!("[性能] extract_text_from_selection 全流程耗时: {}ms", t_total.elapsed().as_millis());
    log::info!("OCR 成功识别并提取文本到剪贴板 ({} 字符)", text.chars().count());
    Ok(text)
}
