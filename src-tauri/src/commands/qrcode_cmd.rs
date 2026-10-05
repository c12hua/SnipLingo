use tauri::State;

use crate::core::capture::{capture_for, crop_captured_image, is_current_capture, SafeCaptureState, SelectionRect};
use crate::core::error::SnipLingoError;
use crate::core::qrcode::{self, QrAction};

#[derive(serde::Serialize)]
pub struct QrCodeResult {
    pub content: String,
    pub action: QrAction,
}

/// 识别选区中的二维码：按 OCR 设置里的"二维码识别"项决定复制内容还是直接用默认浏览器打开网址
#[tauri::command]
pub async fn recognize_qrcode(
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
    capture_id: u64,
) -> Result<QrCodeResult, SnipLingoError> {
    let cropped = {
        let lock = state.lock().map_err(|_| SnipLingoError::CaptureFailed("获取截屏缓存锁失败".to_string()))?;
        let capture = capture_for(&lock, capture_id).map_err(SnipLingoError::CaptureFailed)?;
        crop_captured_image(&capture.original_image, &rect).map_err(SnipLingoError::CaptureFailed)?
    };

    let config = crate::core::config::load_config();
    let result = tokio::task::spawn_blocking(move || {
        let content = qrcode::decode_qrcode(&cropped)?;
        if !is_current_capture(capture_id) {
            return Err(SnipLingoError::CaptureFailed("截图已结束".to_string()));
        }
        let action = qrcode::qr_action_for(&content, config.qrcode_open_in_browser);
        match action {
            QrAction::Opened => qrcode::open_in_default_browser(&content).map_err(SnipLingoError::OcrFailed)?,
            QrAction::Copied => {
                crate::core::clipboard::copy_text_to_clipboard(&content).map_err(SnipLingoError::OcrFailed)?;
            }
        }
        Ok(QrCodeResult { content, action })
    })
    .await
    .map_err(|e| SnipLingoError::OcrFailed(format!("二维码识别线程异常: {}", e)))??;

    let action_text = if result.action == QrAction::Opened { "已用浏览器打开" } else { "已复制到剪贴板" };
    log::info!("二维码识别成功 ({} 字符, {})", result.content.chars().count(), action_text);
    Ok(result)
}
