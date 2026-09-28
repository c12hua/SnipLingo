use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;
use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbaImage};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};

use crate::core::capture::{crop_captured_image, SafeCaptureState, SelectionRect};
use crate::core::config::load_config;

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct PinData {
    pub label: String,
    pub image_data: String,
    pub width: u32,
    pub height: u32,
    pub has_shadow: bool,
    pub opacity: u32,
}

#[derive(Default)]
pub struct PinStorage {
    pub pins: HashMap<String, (RgbaImage, PinData)>,
}

pub type SafePinStorage = Mutex<PinStorage>;

pub const POOLED_PIN_LABELS: &[&str] = &[
    "pin", "pin_1", "pin_2", "pin_3", "pin_4", "pin_5", "pin_6", "pin_7",
];

/// 截取当前选区并作为无框窗口钉在桌面上
pub fn pin_selection_image(app: &AppHandle, rect: &SelectionRect) -> Result<String, String> {
    // 无论后续执行如何，优先确保截图 overlay 退出让出桌面
    if let Some(capture_win) = app.get_webview_window("capture") {
        let _ = capture_win.hide();
    }

    let capture_state = app.try_state::<SafeCaptureState>().ok_or("未找到截图状态")?;
    let (cropped, mon_x, mon_y, scale_factor) = {
        let mut lock = capture_state.lock().map_err(|_| "锁获取失败".to_string())?;
        let state = lock.as_ref().ok_or("当前无可用的截屏数据")?;
        let crop_res = crop_captured_image(&state.original_image, rect);
        let mon_x = state.monitor_x;
        let mon_y = state.monitor_y;
        let scale_factor = state.scale_factor;
        *lock = None;
        let cropped = crop_res?;
        (cropped, mon_x, mon_y, scale_factor)
    };

    // 编码为无损 PNG base64
    let dynamic_img = DynamicImage::ImageRgba8(cropped.clone());
    let mut png_bytes: Vec<u8> = Vec::new();
    dynamic_img
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .map_err(|e| format!("图像编码失败: {}", e))?;

    let base64_str = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png_bytes)
    );

    let config = load_config();
    let has_shadow = config.pin_shadow;

    // 物理坐标与尺寸定位
    let phys_x = mon_x + rect.x as i32;
    let phys_y = mon_y + rect.y as i32;
    let phys_w = rect.width;
    let phys_h = rect.height;

    // 若开启阴影，预留轻微四周外边距防止阴影裁切；若关闭，100% 精确零间隙物理贴合
    let shadow_padding = if has_shadow {
        (8.0 * scale_factor).round() as i32
    } else {
        0
    };

    let win_x = phys_x - shadow_padding;
    let win_y = phys_y - shadow_padding;
    let win_w = phys_w + (shadow_padding * 2) as u32;
    let win_h = phys_h + (shadow_padding * 2) as u32;

    // 智能窗口池分配：优先从预热池中取当前未占用的窗口（零初始化延迟）
    let target_label = {
        let mut chosen = None;
        if let Some(storage) = app.try_state::<SafePinStorage>() {
            if let Ok(lock) = storage.lock() {
                for label in POOLED_PIN_LABELS {
                    let is_occupied = lock.pins.contains_key(*label);
                    let is_vis = app
                        .get_webview_window(label)
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    if !is_occupied && !is_vis {
                        chosen = Some((*label).to_string());
                        break;
                    }
                }
            }
        }
        chosen.unwrap_or_else(|| {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("pin_{}", timestamp)
        })
    };

    let config = load_config();
    let has_shadow = config.pin_shadow;
    let opacity = config.pin_opacity;

    let pin_data = PinData {
        label: target_label.clone(),
        image_data: base64_str,
        width: phys_w,
        height: phys_h,
        has_shadow,
        opacity,
    };

    // 存储当前贴图缓存
    if let Some(storage) = app.try_state::<SafePinStorage>() {
        let mut lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        lock.pins.insert(target_label.clone(), (cropped, pin_data.clone()));
    }

    let target_win = if let Some(existing_win) = app.get_webview_window(&target_label) {
        existing_win
    } else {
        let sf = scale_factor as f64;
        let logical_x = (win_x as f64) / sf;
        let logical_y = (win_y as f64) / sf;
        let logical_w = (win_w as f64) / sf;
        let logical_h = (win_h as f64) / sf;

        WebviewWindowBuilder::new(app, &target_label, WebviewUrl::App(PathBuf::from("pin.html")))
            .title("SnipLingo 贴图")
            .position(logical_x, logical_y)
            .inner_size(logical_w, logical_h)
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .resizable(false)
            .visible(false)
            .build()
            .map_err(|e| format!("创建贴图窗口失败: {}", e))?
    };

    let _ = target_win.set_position(Position::Physical(PhysicalPosition::new(win_x, win_y)));
    let _ = target_win.set_size(Size::Physical(PhysicalSize::new(win_w, win_h)));
    let _ = app.emit_to(&target_label, "load-pin", pin_data);
    let _ = target_win.show();
    let _ = target_win.set_focus();

    log::info!("贴图窗口 [{}] 显示成功", target_label);
    Ok(target_label)
}

/// 获取指定贴图窗口的数据
pub fn get_pin_data_for(app: &AppHandle, label: &str) -> Result<PinData, String> {
    if let Some(storage) = app.try_state::<SafePinStorage>() {
        let lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        if let Some((_, data)) = lock.pins.get(label) {
            return Ok(data.clone());
        }
    }
    Err("未找到贴图数据".to_string())
}

/// 销毁指定贴图窗口
pub fn destroy_pin(app: &AppHandle, label: &str) -> Result<(), String> {
    if let Some(storage) = app.try_state::<SafePinStorage>() {
        if let Ok(mut lock) = storage.lock() {
            lock.pins.remove(label);
        }
    }
    if let Some(win) = app.get_webview_window(label) {
        if POOLED_PIN_LABELS.contains(&label) {
            let _ = win.hide();
        } else {
            let _ = win.destroy();
        }
    }
    Ok(())
}

/// 销毁所有贴图窗口
pub fn destroy_all_pins(app: &AppHandle) -> Result<(), String> {
    if let Some(storage) = app.try_state::<SafePinStorage>() {
        if let Ok(mut lock) = storage.lock() {
            for label in POOLED_PIN_LABELS {
                if let Some(win) = app.get_webview_window(label) {
                    let _ = win.hide();
                }
            }
            for label in lock.pins.keys().cloned().collect::<Vec<_>>() {
                if !POOLED_PIN_LABELS.contains(&label.as_str()) {
                    if let Some(win) = app.get_webview_window(&label) {
                        let _ = win.destroy();
                    }
                }
            }
            lock.pins.clear();
        }
    }
    Ok(())
}

/// 复制贴图图像到剪贴板
pub fn copy_pin_to_clipboard(app: &AppHandle, label: &str) -> Result<(), String> {
    let cropped = if let Some(storage) = app.try_state::<SafePinStorage>() {
        let lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        lock.pins.get(label).map(|(img, _)| img.clone())
    } else {
        None
    }
    .ok_or_else(|| "贴图不存在".to_string())?;

    let res = crate::core::clipboard::copy_rgba_image_to_clipboard(&cropped);
    if res.is_ok() {
        let _ = app.emit_to(label, "pin-copied", label.to_string());
    }
    res
}

/// 翻译贴图对应的文字
pub fn translate_pin(app: &AppHandle, label: &str) -> Result<(), String> {
    let (cropped, win_pos) = {
        let win = app.get_webview_window(label);
        let pos = win.and_then(|w| w.outer_position().ok());
        let storage = app.try_state::<SafePinStorage>().ok_or("未找到贴图存储")?;
        let lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        let (img, _) = lock.pins.get(label).ok_or("贴图数据不存在")?;
        (img.clone(), pos)
    };

    // 1. 根据当前配置使用指定 OCR 引擎提取文字
    let config = load_config();
    let ocr = crate::core::ocr::get_ocr_engine(&config);
    let raw_text = ocr.recognize(&cropped).map_err(|e| e.to_string())?;
    let cleaned_text = crate::core::ocr::text_cleaner::clean_ocr_text_with_options(&raw_text, config.preserve_line_breaks);

    if cleaned_text.trim().is_empty() {
        let _ = app.emit_to(label, "pin-toast", "选区内未发现可识别文字".to_string());
        return Err("选区内未发现可识别的文字内容".to_string());
    }

    // 2. 调起翻译浮窗
    let x = win_pos.map(|p| p.x).unwrap_or(100);
    let y = win_pos.map(|p| p.y).unwrap_or(100);

    if let Some(res_win) = app.get_webview_window("result") {
        let _ = res_win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
        let _ = res_win.show();
        let _ = res_win.set_focus();

        let app_handle = app.clone();
        let text = cleaned_text.clone();
        tauri::async_runtime::spawn(async move {
            let res = crate::core::translate::execute_translation(&config, &text).await;
            match res {
                Ok(trans_text) => {
                    let payload = crate::commands::translate_cmd::TranslationPayload {
                        original_text: text,
                        translated_text: trans_text,
                    };
                    let _ = app_handle.emit("translation-result", payload);
                }
                Err(err) => {
                    let _ = app_handle.emit("translation-error", err);
                }
            }
        });
    }

    Ok(())
}
