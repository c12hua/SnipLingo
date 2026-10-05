use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use crate::core::capture::{capture_for, crop_captured_image, end_capture, is_current_capture, SafeCaptureState, SelectionRect};
use crate::core::pin::SafePinStorage;

/// 生成带当前时间戳的默认截图文件名，例如 SnipLingo_20260928_145520.png
pub fn generate_default_screenshot_filename() -> String {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;
        let st = unsafe { GetLocalTime() };
        format!(
            "SnipLingo_{:04}{:02}{:02}_{:02}{:02}{:02}.png",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("SnipLingo_{}.png", now)
    }
}

/// 将 RgbaImage 写入指定路径，根据文件扩展名自动选用对应的编码器与色彩模式
pub fn save_rgba_image_to_path(img: &image::RgbaImage, path: &std::path::Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => {
            // JPEG 不支持 Alpha 透明通道，将透明度复合在纯白背景上，消除黑色底色
            let mut rgb_img = image::RgbImage::new(img.width(), img.height());
            for (x, y, pixel) in img.enumerate_pixels() {
                let alpha = pixel[3] as f32 / 255.0;
                let r = ((pixel[0] as f32 * alpha) + (255.0 * (1.0 - alpha))).round() as u8;
                let g = ((pixel[1] as f32 * alpha) + (255.0 * (1.0 - alpha))).round() as u8;
                let b = ((pixel[2] as f32 * alpha) + (255.0 * (1.0 - alpha))).round() as u8;
                rgb_img.put_pixel(x, y, image::Rgb([r, g, b]));
            }
            rgb_img
                .save_with_format(path, image::ImageFormat::Jpeg)
                .map_err(|e| format!("保存 JPEG 格式失败: {}", e))?;
        }
        "bmp" => {
            img.save_with_format(path, image::ImageFormat::Bmp)
                .map_err(|e| format!("保存 BMP 格式失败: {}", e))?;
        }
        _ => {
            img.save_with_format(path, image::ImageFormat::Png)
                .map_err(|e| format!("保存 PNG 格式失败: {}", e))?;
        }
    }
    Ok(())
}

/// 弹出系统原生文件保存对话框（在工作线程中调用，避免阻塞 Tokio 异步运行时）
async fn prompt_save_file_dialog(default_name: String) -> Result<Option<PathBuf>, String> {
    tokio::task::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title("保存截图")
            .set_file_name(&default_name)
            .add_filter("PNG 图像 (*.png)", &["png"])
            .add_filter("JPEG 图像 (*.jpg; *.jpeg)", &["jpg", "jpeg"])
            .add_filter("BMP 图像 (*.bmp)", &["bmp"])
            .save_file()
    })
    .await
    .map_err(|e| format!("保存对话框启动失败: {}", e))
}

pub(crate) fn restore_capture_window(app: &AppHandle, capture_id: u64) {
    let main_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !is_current_capture(capture_id) { return }
        let Some(win) = main_app.get_webview_window("capture") else { return };
        #[cfg(target_os = "windows")]
        crate::core::capture::disable_window_animations(&win);

        let _ = win.show();
        let _ = win.set_focus();

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, SetForegroundWindow, SetWindowPos, HWND_TOPMOST,
                SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            };
            use windows::Win32::Foundation::HWND;
            if let Ok(hwnd) = win.hwnd() {
                unsafe {
                    let h = HWND(hwnd.0);
                    let _ = SetWindowPos(
                        h,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                    let _ = BringWindowToTop(h);
                    let _ = SetForegroundWindow(h);
                }
            }
        }
    });
}

async fn save_image_in_background(img: image::RgbaImage, path: PathBuf) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        save_rgba_image_to_path(&img, &path)?;
        Ok(path.to_string_lossy().into_owned())
    }).await.map_err(|e| format!("保存图片任务失败: {}", e))?
}

async fn save_capture_image(app: &AppHandle, img: image::RgbaImage, capture_id: u64) -> Result<Option<String>, String> {
    if !is_current_capture(capture_id) { return Err("截图已结束".to_string()) }
    end_capture(app, capture_id, false)?;
    let result = async {
        match prompt_save_file_dialog(generate_default_screenshot_filename()).await? {
            Some(path) => save_image_in_background(img, path).await.map(Some),
            None => Ok(None),
        }
    }.await;
    match &result {
        Ok(Some(_)) => end_capture(app, capture_id, true)?,
        _ => restore_capture_window(app, capture_id),
    }
    result
}

#[tauri::command]
pub async fn save_selection_to_file(
    app: AppHandle,
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
    capture_id: u64,
) -> Result<Option<String>, String> {
    let cropped = {
        let lock = state.lock().map_err(|_| "获取截屏缓存锁失败".to_string())?;
        crop_captured_image(&capture_for(&lock, capture_id)?.original_image, &rect)?
    };
    save_capture_image(&app, cropped, capture_id).await
}

#[tauri::command]
pub async fn save_data_url_to_file(
    app: AppHandle,
    data_url: String,
    capture_id: u64,
) -> Result<Option<String>, String> {
    if !is_current_capture(capture_id) { return Err("截图已结束".to_string()) }
    let img = tokio::task::spawn_blocking(move || -> Result<image::RgbaImage, String> {
        let base64_data = data_url.split_once(',').map_or(data_url.as_str(), |(_, data)| data);
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_data.trim()).map_err(|e| format!("Base64 解码失败: {}", e))?;
        image::load_from_memory(&bytes)
            .map(|img| img.to_rgba8()).map_err(|e| format!("解析图片格式失败: {}", e))
    }).await.map_err(|e| format!("图片解码任务失败: {}", e))??;
    save_capture_image(&app, img, capture_id).await
}

#[tauri::command]
pub async fn save_pin_image_cmd(
    app: AppHandle,
    label: String,
) -> Result<Option<String>, String> {
    save_pin_image_internal(&app, &label).await
}

pub async fn save_pin_image_internal(
    app: &AppHandle,
    label: &str,
) -> Result<Option<String>, String> {
    let cropped = {
        let storage = app.try_state::<SafePinStorage>().ok_or("未找到贴图存储")?;
        let lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        let (img, _) = lock.pins.get(label).ok_or("贴图数据不存在")?;
        img.clone()
    };

    let default_name = generate_default_screenshot_filename();
    let chosen_path = prompt_save_file_dialog(default_name).await?;

    if let Some(path) = chosen_path {
        let display_path = save_image_in_background(cropped, path).await?;
        let _ = app.emit_to(label, "pin-toast", "图片已保存".to_string());
        log::info!("贴图 [{}] 已成功保存到文件: {}", label, display_path);
        Ok(Some(display_path))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_default_screenshot_filename() {
        let name = generate_default_screenshot_filename();
        assert!(name.starts_with("SnipLingo_"));
        assert!(name.ends_with(".png"));
    }

    #[test]
    fn test_save_rgba_image_to_path_png_and_bmp() {
        let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        let tmp_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test_tmp");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let tmp_png = tmp_dir.join("test_sniplingo_save.png");
        let tmp_bmp = tmp_dir.join("test_sniplingo_save.bmp");
        let tmp_jpg = tmp_dir.join("test_sniplingo_save.jpg");

        assert!(save_rgba_image_to_path(&img, &tmp_png).is_ok());
        assert!(save_rgba_image_to_path(&img, &tmp_bmp).is_ok());
        assert!(save_rgba_image_to_path(&img, &tmp_jpg).is_ok());

        assert!(tmp_png.exists());
        assert!(tmp_bmp.exists());
        assert!(tmp_jpg.exists());

        let _ = std::fs::remove_file(tmp_png);
        let _ = std::fs::remove_file(tmp_bmp);
        let _ = std::fs::remove_file(tmp_jpg);
    }
}
