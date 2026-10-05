use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use base64::Engine;
use image::{ImageEncoder, RgbaImage};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};

use crate::core::capture::{capture_for, crop_captured_image, disable_window_animations, end_capture, is_current_capture, SafeCaptureState, SelectionRect};
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
pub async fn pin_selection_image(app: &AppHandle, rect: &SelectionRect, capture_id: u64) -> Result<String, String> {
    let capture_state = app.try_state::<SafeCaptureState>().ok_or("未找到截图状态")?;
    let (cropped, mon_x, mon_y, scale_factor) = {
        let lock = capture_state.lock().map_err(|_| "锁获取失败".to_string())?;
        let state = capture_for(&lock, capture_id)?;
        (crop_captured_image(&state.original_image, rect)?, state.monitor_x, state.monitor_y, state.scale_factor)
    };
    end_capture(app, capture_id, false)?;
    let (cropped, base64_str) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let mut png_bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(cropped.as_raw(), cropped.width(), cropped.height(), image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("图像编码失败: {}", e))?;
        let base64_str = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png_bytes));
        Ok((cropped, base64_str))
    }).await.map_err(|e| format!("贴图编码任务失败: {}", e))??;
    if !is_current_capture(capture_id) { return Err("截图已结束".to_string()) }

    let config = load_config();
    let has_shadow = config.pin_shadow;
    let opacity = config.pin_opacity;

    // 物理坐标与尺寸定位
    let phys_x = mon_x + rect.x as i32;
    let phys_y = mon_y + rect.y as i32;
    let phys_w = cropped.width();
    let phys_h = cropped.height();

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

    // 智能窗口池分配 + 占用登记在同一锁作用域内完成（async 命令可能并发进入，
    // 防止两次贴图选中同一 label）：优先取空闲池窗口，池窗口不再启动预创建，
    // 随实际并发贴图数按需增长（复用后常驻待用）。
    // 注意：锁内不得做任何等主线程的 dispatcher 调用（is_visible 之类）——
    // 菜单事件在主线程也要拿这把锁，持锁等待主线程会对撞死锁；
    // pins map 即占用唯一真相（所有销毁路径都经 destroy_pin 同步移除）
    let (target_label, pin_data) = {
        let storage = app.try_state::<SafePinStorage>().ok_or("未找到贴图存储")?;
        let mut lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        let mut chosen: Option<String> = None;
        for label in POOLED_PIN_LABELS {
            if !lock.pins.contains_key(*label) {
                chosen = Some((*label).to_string());
                break;
            }
        }
        let label = chosen.unwrap_or_else(|| {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                format!("pin_{}", timestamp)
            });

        let pin_data = PinData {
            label: label.clone(),
            image_data: base64_str,
            width: phys_w,
            height: phys_h,
            has_shadow,
            opacity,
        };
        lock.pins.insert(label.clone(), (cropped, pin_data.clone()));
        (label, pin_data)
    };

    let target_win = if let Some(existing_win) = app.get_webview_window(&target_label) {
        existing_win
    } else {
        // ponytail: 运行时建窗在 --process-per-site 下不可用（WebView2 已知限制），
        // 正常情况下 8 个预创建池窗口足够覆盖并发贴图，此路径仅在超限时兜底；
        // 超限时 build 失败会以 Err 返回，由前端 toast 提示
        let sf = scale_factor as f64;
        let logical_x = (win_x as f64) / sf;
        let logical_y = (win_y as f64) / sf;
        let logical_w = (win_w as f64) / sf;
        let logical_h = (win_h as f64) / sf;

        let win = WebviewWindowBuilder::new(app, &target_label, WebviewUrl::App(PathBuf::from("pin.html")))
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
            .map_err(|e| {
                let _ = destroy_pin(app, &target_label);
                format!("创建贴图窗口失败: {}", e)
            })?;
        // hwnd() 会等待主线程完成真实窗口创建；动画禁用赶在 show 之前生效
        #[cfg(target_os = "windows")]
        disable_window_animations(&win);
        win
    };

    let shown = (|| -> tauri::Result<()> {
        target_win.set_position(Position::Physical(PhysicalPosition::new(win_x, win_y)))?;
        target_win.set_size(Size::Physical(PhysicalSize::new(win_w, win_h)))?;
        app.emit_to(&target_label, "load-pin", pin_data)?;
        target_win.show()?;
        target_win.set_focus()
    })();
    if let Err(e) = shown {
        let _ = destroy_pin(app, &target_label);
        return Err(format!("显示贴图失败: {}", e));
    }
    end_capture(app, capture_id, true)?;
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
            let _ = app.emit_to(label, "clear-pin", label);
            let _ = win.hide();
        } else {
            let _ = win.destroy();
        }
    }
    Ok(())
}

/// 销毁所有贴图窗口
pub fn destroy_all_pins(app: &AppHandle) -> Result<(), String> {
    let labels = if let Some(storage) = app.try_state::<SafePinStorage>() {
        let lock = storage.lock().map_err(|_| "锁获取失败".to_string())?;
        lock.pins.keys().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    // 与单张关闭共用清理；不持有存储锁等待窗口线程。
    for label in labels { destroy_pin(app, &label)?; }
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

    use crate::commands::translate_cmd::{emit_ocr_ready, is_current_result, show_result_window, TranslationPayload};
    use crate::core::error::SnipLingoError;
    let config = load_config();
    let x = win_pos.map(|p| p.x).unwrap_or(100);
    let y = win_pos.map(|p| p.y).unwrap_or(100);
    let request_id = show_result_window(app.clone(), Some(x), Some(y), None)?;
    let app = app.clone();
    let label = label.to_string();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let cfg = config.clone();
            let text = tokio::task::spawn_blocking(move || {
                crate::core::ocr::get_ocr_engine(&cfg).recognize(&cropped)
            }).await.map_err(|e| SnipLingoError::OcrFailed(format!("OCR 任务异常: {}", e)))??;
            if !is_current_result(request_id) {
                return Err(SnipLingoError::CaptureFailed("翻译请求已过期".to_string()));
            }
            emit_ocr_ready(&app, request_id, &text);
            let translated_text = crate::core::translate::execute_translation(&config, &text).await?;
            Ok(TranslationPayload { request_id, original_text: text, translated_text })
        }.await;
        if !is_current_result(request_id) { return }
        match result {
            Ok(payload) => { let _ = app.emit_to("result", "translation-result", payload); }
            Err(error) => {
                let _ = app.emit_to(&label, "pin-toast", error.to_string());
                let _ = app.emit_to("result", "translation-error", serde_json::json!({"request_id": request_id, "error": error}));
            }
        }
    });
    Ok(())
}
