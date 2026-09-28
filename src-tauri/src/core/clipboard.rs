use arboard::{Clipboard, ImageData};
use image::RgbaImage;
use std::borrow::Cow;

/// 将 RGBA 原始位图写入 Windows 系统剪贴板 (支持微信、Word、画图、Photoshop 等原生粘贴)
pub fn copy_rgba_image_to_clipboard(img: &RgbaImage) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("初始化系统剪贴板失败: {}", e))?;
    let image_data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: Cow::Borrowed(img.as_raw()),
    };
    clipboard
        .set_image(image_data)
        .map_err(|e| format!("写入系统剪贴板失败: {}", e))?;
    Ok(())
}

/// 将文本写入系统剪贴板 (预留供翻译结果复制使用)
pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("初始化系统剪贴板失败: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("写入系统剪贴板失败: {}", e))?;
    Ok(())
}

/// 预热 Windows 系统剪贴板 OLE 运行环境，消除首次点击复制时的 OLE 初始化耗时
pub fn warmup_clipboard() {
    log::info!("正在后台静默预热系统剪贴板 OLE 运行环境...");
    let _ = Clipboard::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_clipboard() {
        warmup_clipboard();
    }
}
