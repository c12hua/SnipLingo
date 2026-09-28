use crate::core::capture::{crop_captured_image, SafeCaptureState, SelectionRect};
use crate::core::config::load_config;
use crate::core::error::SnipLingoError;
use crate::core::ocr::{OcrLineBlock, OcrRect};
use crate::core::translate::execute_translation;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Position, State};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TranslationPayload {
    pub original_text: String,
    pub translated_text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InPlaceBlock {
    pub id: usize,
    pub original_text: String,
    pub translated_text: String,
    pub rect: OcrRect,
    pub font_size_px: f32,
    pub bg_color: String,
    pub text_color: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InPlaceTranslationResult {
    pub blocks: Vec<InPlaceBlock>,
    pub image_data: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

struct IntermediateBlock {
    pub lines: Vec<OcrLineBlock>,
    pub rect: OcrRect,
}

fn sample_block_style(img: &image::RgbaImage, rect: &OcrRect) -> (String, String, f32) {
    let img_w = img.width();
    let img_h = img.height();

    let mut samples = Vec::new();
    let margin = 3i32;

    let xs = [
        rect.x as i32 + 2,
        rect.x as i32 + (rect.width as i32 / 2),
        (rect.x + rect.width) as i32 - 2,
    ];
    let top_y = rect.y as i32 - margin;
    let bottom_y = (rect.y + rect.height) as i32 + margin;

    for &x in &xs {
        if top_y >= 0 && top_y < img_h as i32 && x >= 0 && x < img_w as i32 {
            samples.push(img.get_pixel(x as u32, top_y as u32));
        }
        if bottom_y >= 0 && bottom_y < img_h as i32 && x >= 0 && x < img_w as i32 {
            samples.push(img.get_pixel(x as u32, bottom_y as u32));
        }
    }

    let ys = [
        rect.y as i32 + 2,
        rect.y as i32 + (rect.height as i32 / 2),
        (rect.y + rect.height) as i32 - 2,
    ];
    let left_x = rect.x as i32 - margin;
    let right_x = (rect.x + rect.width) as i32 + margin;
    for &y in &ys {
        if left_x >= 0 && left_x < img_w as i32 && y >= 0 && y < img_h as i32 {
            samples.push(img.get_pixel(left_x as u32, y as u32));
        }
        if right_x >= 0 && right_x < img_w as i32 && y >= 0 && y < img_h as i32 {
            samples.push(img.get_pixel(right_x as u32, y as u32));
        }
    }

    let (bg_color, text_color) = if !samples.is_empty() {
        let (mut sum_r, mut sum_g, mut sum_b) = (0u32, 0u32, 0u32);
        for p in &samples {
            sum_r += p[0] as u32;
            sum_g += p[1] as u32;
            sum_b += p[2] as u32;
        }
        let count = samples.len() as u32;
        let avg_r = (sum_r / count) as u8;
        let avg_g = (sum_g / count) as u8;
        let avg_b = (sum_b / count) as u8;

        let luminance = 0.299 * (avg_r as f32) + 0.587 * (avg_g as f32) + 0.114 * (avg_b as f32);
        let text_col = if luminance > 135.0 {
            "#0f172a".to_string()
        } else {
            "#f8fafc".to_string()
        };
        (
            format!("rgb({}, {}, {})", avg_r, avg_g, avg_b),
            text_col,
        )
    } else {
        ("rgb(255, 255, 255)".to_string(), "#0f172a".to_string())
    };

    let font_size = ((rect.height as f32) * 0.72).clamp(12.0, 48.0);
    (bg_color, text_color, font_size)
}

fn group_lines_into_blocks(mut lines: Vec<OcrLineBlock>) -> Vec<IntermediateBlock> {
    if lines.is_empty() {
        return Vec::new();
    }

    // Sort lines top-to-bottom, then left-to-right
    lines.sort_by(|a, b| {
        let diff_y = a.rect.y as i32 - b.rect.y as i32;
        if diff_y.abs() < 6 {
            a.rect.x.cmp(&b.rect.x)
        } else {
            a.rect.y.cmp(&b.rect.y)
        }
    });

    let mut blocks: Vec<IntermediateBlock> = Vec::new();

    for line in lines {
        if line.text.trim().is_empty() {
            continue;
        }

        let mut merged = false;
        if let Some(last_block) = blocks.last_mut() {
            let block_bottom = last_block.rect.y + last_block.rect.height;
            let avg_line_h = (last_block.rect.height / (last_block.lines.len() as u32)).max(12);
            let v_gap = line.rect.y as i32 - block_bottom as i32;

            let overlap_x = (line.rect.x < last_block.rect.x + last_block.rect.width)
                && (line.rect.x + line.rect.width > last_block.rect.x);
            let align_x = (line.rect.x as i32 - last_block.rect.x as i32).abs() < (avg_line_h as i32 * 3);

            if v_gap >= -((avg_line_h / 2) as i32)
                && v_gap <= (avg_line_h as f32 * 1.5) as i32
                && (overlap_x || align_x)
            {
                let min_x = last_block.rect.x.min(line.rect.x);
                let min_y = last_block.rect.y.min(line.rect.y);
                let max_x = (last_block.rect.x + last_block.rect.width).max(line.rect.x + line.rect.width);
                let max_y = (last_block.rect.y + last_block.rect.height).max(line.rect.y + line.rect.height);

                last_block.rect.x = min_x;
                last_block.rect.y = min_y;
                last_block.rect.width = max_x - min_x;
                last_block.rect.height = max_y - min_y;
                last_block.lines.push(line.clone());
                merged = true;
            }
        }

        if !merged {
            let r = line.rect.clone();
            blocks.push(IntermediateBlock {
                rect: r,
                lines: vec![line],
            });
        }
    }

    blocks
}

#[tauri::command]
pub async fn translate_selection(
    app: AppHandle,
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
) -> Result<TranslationPayload, SnipLingoError> {
    let t_total = std::time::Instant::now();

    // 1. 获取锁并裁剪内存中原始位图
    let t_crop = std::time::Instant::now();
    let cropped = {
        let lock = state.lock().map_err(|_| {
            SnipLingoError::CaptureFailed("获取截屏缓存锁失败".to_string())
        })?;
        let capture_state = lock
            .as_ref()
            .ok_or_else(|| SnipLingoError::CaptureFailed("未找到当前的截图底图缓存".to_string()))?;
        let crop_res = crop_captured_image(&capture_state.original_image, &rect);
        crop_res.map_err(SnipLingoError::CaptureFailed)?
    };
    log::debug!("[性能] 选区裁剪耗时: {}ms (尺寸: {}x{})", t_crop.elapsed().as_millis(), cropped.width(), cropped.height());

    // 2. 读取配置，并将同步 CPU 重计算 (OCR 推理) 移出异步运行时放入 blocking worker (Stage 2 & 3)
    let config = load_config();
    let cfg = config.clone();
    let text = tokio::task::spawn_blocking(move || {
        let ocr = crate::core::ocr::get_ocr_engine(&cfg);
        ocr.recognize(&cropped)
    })
    .await
    .map_err(|e| SnipLingoError::OcrFailed(format!("OCR 任务执行异常: {}", e)))??;

    // 渐进式极速上屏：OCR 识别完成瞬间立即通知前端渲染原文，用户体感延迟直降为 0
    let _ = app.emit("ocr-ready", &text);

    // 3. 执行翻译 (长连接复用 + 内存高速缓存 + Single-Flight 并发合并)
    let translation = execute_translation(&config, &text).await?;

    log::debug!("[性能] translate_selection 全链路耗时: {}ms", t_total.elapsed().as_millis());

    Ok(TranslationPayload {
        original_text: text,
        translated_text: translation,
    })
}

#[tauri::command]
pub async fn retry_translate(
    app: AppHandle,
    original_text: String,
) -> Result<TranslationPayload, SnipLingoError> {
    let _ = app.emit("ocr-ready", &original_text);
    let config = load_config();
    let translation = execute_translation(&config, &original_text).await?;

    Ok(TranslationPayload {
        original_text,
        translated_text: translation,
    })
}

#[tauri::command]
pub fn show_result_window(
    app: AppHandle,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("result") {
        if let (Some(px), Some(py)) = (x, y) {
            let _ = win.set_position(Position::Physical(PhysicalPosition::new(px, py)));
        }
        let _ = win.show();
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn close_result_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("result") {
        let _ = win.hide();
    }
    crate::core::capture::clear_capture_state(&app);
    Ok(())
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    Ok(())
}

struct PreparedInPlace {
    grouped: Vec<IntermediateBlock>,
    image_data: String,
    width: u32,
    height: u32,
    block_styles: Vec<(String, String, f32)>,
}

#[tauri::command]
pub async fn translate_in_place(
    _app: AppHandle,
    state: State<'_, SafeCaptureState>,
    rect: SelectionRect,
) -> Result<InPlaceTranslationResult, SnipLingoError> {
    let t_total = std::time::Instant::now();

    // 1. 获取锁并裁剪内存中原始位图
    let t_crop = std::time::Instant::now();
    let (cropped, scale_factor) = {
        let lock = state.lock().map_err(|_| {
            SnipLingoError::CaptureFailed("获取截屏缓存锁失败".to_string())
        })?;
        let capture_state = lock
            .as_ref()
            .ok_or_else(|| SnipLingoError::CaptureFailed("未找到当前的截图底图缓存".to_string()))?;
        let crop_res = crop_captured_image(&capture_state.original_image, &rect);
        let cropped = crop_res.map_err(SnipLingoError::CaptureFailed)?;
        (cropped, capture_state.scale_factor)
    };
    log::debug!("[性能] translate_in_place 选区裁剪耗时: {}ms (尺寸: {}x{})", t_crop.elapsed().as_millis(), cropped.width(), cropped.height());

    let config = load_config();
    let cfg = config.clone();

    // 2. 将 CPU 密集的 OCR 识别、直接内存 PNG 编码 (免 clone)、段落聚合与样式采样放入 blocking worker (Stage 2, 3, 4)
    let prepared = tokio::task::spawn_blocking(move || -> Result<PreparedInPlace, SnipLingoError> {
        let ocr = crate::core::ocr::get_ocr_engine(&cfg);
        let detailed = ocr.recognize_detailed(&cropped)?;

        if detailed.blocks.is_empty() {
            return Err(SnipLingoError::NoTextDetected);
        }

        // 直接从 cropped 原始字节流编码 PNG，无需 DynamicImage 复制和包装 (Stage 4)
        let t_enc = std::time::Instant::now();
        let mut png_bytes: Vec<u8> = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        use image::ImageEncoder;
        encoder.write_image(
            cropped.as_raw(),
            cropped.width(),
            cropped.height(),
            image::ExtendedColorType::Rgba8,
        ).map_err(|e| SnipLingoError::CaptureFailed(format!("图像编码失败: {}", e)))?;

        use base64::Engine;
        let image_data = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&png_bytes)
        );
        drop(png_bytes); // 立即释放大型中间字节缓冲
        log::debug!("[性能] translate_in_place 图像 PNG/Base64 编码耗时: {}ms", t_enc.elapsed().as_millis());

        let grouped = group_lines_into_blocks(detailed.blocks);

        let mut block_styles = Vec::with_capacity(grouped.len());
        for block in &grouped {
            let (bg_color, text_color, mut font_size_px) = sample_block_style(&cropped, &block.rect);
            if block.lines.len() > 1 {
                let avg_h = block.rect.height as f32 / block.lines.len() as f32;
                font_size_px = (avg_h * 0.72).clamp(12.0, 48.0);
            }
            block_styles.push((bg_color, text_color, font_size_px));
        }

        Ok(PreparedInPlace {
            grouped,
            image_data,
            width: cropped.width(),
            height: cropped.height(),
            block_styles,
        })
    })
    .await
    .map_err(|e| SnipLingoError::OcrFailed(format!("图像处理线程异常: {}", e)))??;

    // 3. 并发执行翻译请求 (Single-Flight 自动合并相同段落)
    let mut tasks = Vec::new();
    for (idx, (block, (bg_color, text_color, font_size_px))) in prepared.grouped.into_iter().zip(prepared.block_styles).enumerate() {
        let raw_block_text = block
            .lines
            .iter()
            .map(|l| l.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        let cleaned_block_text = crate::core::ocr::text_cleaner::clean_ocr_text_with_options(
            &raw_block_text,
            false,
        );

        let cfg = config.clone();
        let rect = block.rect;
        tasks.push(tauri::async_runtime::spawn(async move {
            let trans_res = execute_translation(&cfg, &cleaned_block_text).await;
            (
                idx,
                cleaned_block_text,
                trans_res,
                rect,
                font_size_px,
                bg_color,
                text_color,
            )
        }));
    }

    let mut result_blocks = Vec::new();
    for task in tasks {
        if let Ok((id, original_text, trans_res, rect, font_size_px, bg_color, text_color)) = task.await {
            let translated_text = match trans_res {
                Ok(t) => t,
                Err(_) => original_text.clone(),
            };
            result_blocks.push(InPlaceBlock {
                id,
                original_text,
                translated_text,
                rect,
                font_size_px,
                bg_color,
                text_color,
            });
        }
    }

    result_blocks.sort_by_key(|b| b.id);

    log::debug!("[性能] translate_in_place 全流程耗时: {}ms (文本块数: {})", t_total.elapsed().as_millis(), result_blocks.len());

    Ok(InPlaceTranslationResult {
        blocks: result_blocks,
        image_data: prepared.image_data,
        width: prepared.width,
        height: prepared.height,
        scale_factor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn test_group_lines_into_blocks_merge_adjacent() {
        let line1 = OcrLineBlock {
            text: "Hello".to_string(),
            rect: OcrRect { x: 10, y: 10, width: 50, height: 20 },
            confidence: 0.95,
        };
        let line2 = OcrLineBlock {
            text: "World".to_string(),
            rect: OcrRect { x: 12, y: 32, width: 48, height: 20 },
            confidence: 0.96,
        };
        let line3 = OcrLineBlock {
            text: "Far away paragraph".to_string(),
            rect: OcrRect { x: 10, y: 120, width: 100, height: 20 },
            confidence: 0.94,
        };

        let blocks = group_lines_into_blocks(vec![line1, line2, line3]);
        assert_eq!(blocks.len(), 2);
        // First block merged line1 and line2
        assert_eq!(blocks[0].lines.len(), 2);
        assert_eq!(blocks[0].rect.x, 10);
        assert_eq!(blocks[0].rect.y, 10);
        assert_eq!(blocks[0].rect.height, 42); // 32 + 20 - 10

        // Second block is standalone line3
        assert_eq!(blocks[1].lines.len(), 1);
        assert_eq!(blocks[1].rect.y, 120);
    }

    #[test]
    fn test_sample_block_style_dark_and_light() {
        // Light background image (all white)
        let mut light_img = RgbaImage::new(100, 100);
        for pixel in light_img.pixels_mut() {
            *pixel = Rgba([255, 255, 255, 255]);
        }
        let rect = OcrRect { x: 20, y: 20, width: 40, height: 20 };
        let (bg, text_col, font_size) = sample_block_style(&light_img, &rect);
        assert!(bg.contains("255"));
        assert_eq!(text_col, "#0f172a"); // Dark text on light background
        assert!(font_size >= 12.0);

        // Dark background image (all dark)
        let mut dark_img = RgbaImage::new(100, 100);
        for pixel in dark_img.pixels_mut() {
            *pixel = Rgba([20, 20, 20, 255]);
        }
        let (dark_bg, dark_text_col, _) = sample_block_style(&dark_img, &rect);
        assert!(dark_bg.contains("20"));
        assert_eq!(dark_text_col, "#f8fafc"); // Light text on dark background
    }
}
