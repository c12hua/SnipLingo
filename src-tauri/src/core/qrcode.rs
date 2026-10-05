use crate::core::error::SnipLingoError;
use image::RgbaImage;

/// 二维码识别结果的处理动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QrAction {
    /// 内容已复制到剪贴板
    Copied,
    /// 已用默认浏览器打开
    Opened,
}

/// 仅 http/https 开头的内容才允许直接拉起浏览器，其余内容（文本/WiFi/名片等）一律走复制
pub fn looks_like_url(content: &str) -> bool {
    content.starts_with("http://") || content.starts_with("https://")
}

/// 根据用户设置决定识别结果的处理动作
pub fn qr_action_for(content: &str, open_in_browser: bool) -> QrAction {
    if open_in_browser && looks_like_url(content) {
        QrAction::Opened
    } else {
        QrAction::Copied
    }
}

/// 在选区图像中查找并解码二维码（rqrr 纯 Rust 解码，选区小、毫秒级完成）。
/// 所有缓冲都是选区级临时分配，rqrr 无任何全局缓存，解码完即释放，不增加常驻内存。
pub fn decode_qrcode(cropped: &RgbaImage) -> Result<String, SnipLingoError> {
    // 直接从 RGBA 引用转灰度，避免再克隆一份整选区（全屏选区可省 ~8MB 瞬时峰值）
    let gray = image::imageops::grayscale(cropped);
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    let grids = prepared.detect_grids();
    let Some(grid) = grids.first() else {
        return Err(SnipLingoError::NoQrCodeFound);
    };
    let (_meta, content) = grid
        .decode()
        .map_err(|e| SnipLingoError::OcrFailed(format!("二维码解码失败: {:?}", e)))?;
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err(SnipLingoError::NoQrCodeFound);
    }
    Ok(content)
}

/// 用系统默认浏览器打开网址（ShellExecute 不弹控制台窗口）
#[cfg(target_os = "windows")]
pub fn open_in_default_browser(url: &str) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let url = HSTRING::from(url);
    let operation = HSTRING::from("open");
    let null = PCWSTR::null();
    unsafe {
        // ShellExecuteW 返回值 > 32 表示成功
        let hinst = ShellExecuteW(
            None,
            PCWSTR::from_raw(operation.as_ptr()),
            PCWSTR::from_raw(url.as_ptr()),
            null,
            null,
            SW_SHOWNORMAL,
        );
        if hinst.0 as isize <= 32 {
            return Err(format!(
                "打开默认浏览器失败 (ShellExecute 返回 {})",
                hinst.0 as isize
            ));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn open_in_default_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开默认浏览器失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_qr_action_decision() {
        assert_eq!(qr_action_for("https://example.com", true), QrAction::Opened);
        assert_eq!(qr_action_for("http://example.com/x?y=1", true), QrAction::Opened);
        // 非 URL 内容即使开了浏览器模式也降级为复制
        assert_eq!(qr_action_for("HELLO WORLD", true), QrAction::Copied);
        assert_eq!(qr_action_for("ftp://example.com", true), QrAction::Copied);
        // 复制模式下永远是复制
        assert_eq!(qr_action_for("https://example.com", false), QrAction::Copied);
        assert!(looks_like_url("https://a.b"));
        assert!(!looks_like_url("https:/missing-slash"));
    }

    #[test]
    fn test_decode_no_qr_in_blank_image() {
        // 全屏尺寸空白图：走完 灰度→prepare→detect 全流程，验证"未检测到"路径与整屏耗时
        let img = RgbaImage::from_pixel(1920, 1080, Rgba([255, 255, 255, 255]));
        let t = std::time::Instant::now();
        let err = decode_qrcode(&img).unwrap_err();
        println!("全屏空白图解码尝试耗时: {:?}", t.elapsed());
        assert_eq!(err, SnipLingoError::NoQrCodeFound);
    }
}
