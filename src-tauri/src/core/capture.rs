use std::sync::Mutex;
use image::RgbaImage;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size};
use xcap::Monitor;

#[derive(Clone, serde::Serialize)]
pub struct CapturePayload {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

#[cfg(target_os = "windows")]
pub fn disable_window_animations(win: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
    use windows::Win32::Foundation::HWND;
    use windows::core::BOOL;
    if let Ok(hwnd) = win.hwnd() {
        unsafe {
            let disable = BOOL::from(true);
            let _ = DwmSetWindowAttribute(
                HWND(hwnd.0),
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable as *const _ as *const _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct SelectionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct CaptureState {
    pub original_image: RgbaImage,
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

pub type SafeCaptureState = Mutex<Option<CaptureState>>;

/// 安全无损裁剪内存中的全屏原始位图
pub fn crop_captured_image(
    original: &RgbaImage,
    rect: &SelectionRect,
) -> Result<RgbaImage, String> {
    let orig_w = original.width();
    let orig_h = original.height();

    if rect.width == 0 || rect.height == 0 {
        return Err("选区宽高必须大于 0".to_string());
    }

    if rect.x >= orig_w || rect.y >= orig_h {
        return Err("选区起始坐标超出屏幕范围".to_string());
    }

    // 边界安全裁切约束
    let x = rect.x;
    let y = rect.y;
    let width = rect.width.min(orig_w - x);
    let height = rect.height.min(orig_h - y);

    if width == 0 || height == 0 {
        return Err("裁剪有效区域为空".to_string());
    }

    let cropped = image::imageops::crop_imm(original, x, y, width, height).to_image();
    Ok(cropped)
}

#[cfg(target_os = "windows")]
fn get_current_cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
        Some((pt.x, pt.y))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn get_current_cursor_pos() -> Option<(i32, i32)> {
    None
}

/// 截取目标显示器，并将底图推送给 capture 窗口
pub fn trigger_capture(app: &AppHandle) -> Result<(), String> {
    let t_start = std::time::Instant::now();

    // 异步预热翻译网络连接 (趁用户划选选区的 1~3 秒人工间隙提前完成 DNS 与 TLS 握手)
    let config = crate::core::config::load_config();
    crate::core::translate::prewarm_configured_engine(&config);

    let monitors = Monitor::all().map_err(|e| format!("获取显示器失败: {}", e))?;
    if monitors.is_empty() {
        return Err("未检测到有效显示器".to_string());
    }

    // 智能多显示器定位：优先匹配光标当前所在显示器，否则降级为主显示器/首个显示器
    let cursor_pos = get_current_cursor_pos();
    let target_monitor = if let Some((cx, cy)) = cursor_pos {
        monitors.iter().find(|m| {
            if let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) = (m.x(), m.y(), m.width(), m.height()) {
                cx >= mx && cx < mx + (mw as i32) && cy >= my && cy < my + (mh as i32)
            } else {
                false
            }
        })
    } else {
        None
    }
    .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
    .or_else(|| monitors.first())
    .ok_or_else(|| "无法获取目标显示器".to_string())?;

    let mon_x = target_monitor.x().map_err(|e| e.to_string())?;
    let mon_y = target_monitor.y().map_err(|e| e.to_string())?;
    let mon_w = target_monitor.width().map_err(|e| e.to_string())?;
    let mon_h = target_monitor.height().map_err(|e| e.to_string())?;
    let scale_factor = target_monitor.scale_factor().map_err(|e| e.to_string())?;

    // 1. 静默抓取全屏图像（此时 capture-window 仍处于隐藏状态，彻底避免包含自身浮层）
    let t_cap_start = std::time::Instant::now();
    let rgba_img = target_monitor
        .capture_image()
        .map_err(|e| format!("截取屏幕图像失败: {}", e))?;
    let t_cap_duration = t_cap_start.elapsed();

    // 2. 将无损原始位图直接暂存在 AppState 中，彻底消除 CPU 耗时的 JPEG 压缩和 Base64 编码，实现急速响应
    if let Some(state) = app.try_state::<SafeCaptureState>() {
        let mut lock = state.lock().map_err(|_| "锁获取失败".to_string())?;
        *lock = Some(CaptureState {
            original_image: rgba_img,
            monitor_x: mon_x,
            monitor_y: mon_y,
            width: mon_w,
            height: mon_h,
            scale_factor,
        });
    }

    // 3. 定位并展示 capture 窗口（禁用 DWM 缩放跃动动画）
    if let Some(capture_win) = app.get_webview_window("capture") {
        #[cfg(target_os = "windows")]
        disable_window_animations(&capture_win);

        let _ = capture_win.set_position(Position::Physical(PhysicalPosition::new(mon_x, mon_y)));
        let _ = capture_win.set_size(Size::Physical(PhysicalSize::new(mon_w, mon_h)));

        // 发送屏幕几何参数给前端，通知其初始化选区
        let payload = CapturePayload {
            width: mon_w,
            height: mon_h,
            scale_factor,
        };
        let _ = capture_win.emit("screenshot-captured", payload);

        let _ = capture_win.show();
        let _ = capture_win.set_focus();

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, SetForegroundWindow, SetWindowPos, HWND_TOPMOST,
                SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            };
            use windows::Win32::Foundation::HWND;
            if let Ok(hwnd) = capture_win.hwnd() {
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

        log::debug!(
            "[阶段0性能基线] 截图热键触发至 capture 窗口显示: {:?} (底层截屏耗时: {:?}, 分辨率: {}x{})",
            t_start.elapsed(),
            t_cap_duration,
            mon_w,
            mon_h
        );

        Ok(())
    } else {
        Err("未找到 capture 窗口".to_string())
    }
}

/// 释放全屏底图大内存，保持应用常驻运行时的极低内存占用
pub fn clear_capture_state(app: &AppHandle) {
    if let Some(state) = app.try_state::<SafeCaptureState>() {
        if let Ok(mut lock) = state.lock() {
            *lock = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_crop_captured_image_valid() {
        let mut img = RgbaImage::new(100, 100);
        img.put_pixel(10, 10, Rgba([255, 0, 0, 255]));

        let rect = SelectionRect {
            x: 10,
            y: 10,
            width: 20,
            height: 30,
        };

        let cropped = crop_captured_image(&img, &rect).expect("裁剪应该成功");
        assert_eq!(cropped.width(), 20);
        assert_eq!(cropped.height(), 30);
        assert_eq!(*cropped.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn test_crop_captured_image_clamped_boundary() {
        let img = RgbaImage::new(100, 100);
        let rect = SelectionRect {
            x: 80,
            y: 80,
            width: 50,
            height: 50,
        };

        let cropped = crop_captured_image(&img, &rect).expect("应自动约束至图像边界");
        assert_eq!(cropped.width(), 20); // 100 - 80 = 20
        assert_eq!(cropped.height(), 20); // 100 - 80 = 20
    }

    #[test]
    fn test_crop_captured_image_zero_size() {
        let img = RgbaImage::new(100, 100);
        let rect = SelectionRect {
            x: 10,
            y: 10,
            width: 0,
            height: 10,
        };
        assert!(crop_captured_image(&img, &rect).is_err());
    }

    #[test]
    fn test_crop_captured_image_out_of_bounds() {
        let img = RgbaImage::new(100, 100);
        let rect = SelectionRect {
            x: 200,
            y: 200,
            width: 10,
            height: 10,
        };
        assert!(crop_captured_image(&img, &rect).is_err());
    }
}
