use crate::core::error::SnipLingoError;
use crate::core::ocr::preprocess::preprocess_image_for_ocr;
use crate::core::ocr::text_cleaner::clean_ocr_text_with_options;
use crate::core::ocr::{DetailedOcrResult, OcrLineBlock, OcrRect, OcrEngine};
use image::RgbaImage;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine as WinOcrEngine;
use windows::Storage::Streams::DataWriter;

pub struct WindowsMediaOcr {
    ocr_lang: String,
    enhance_contrast: bool,
    preserve_line_breaks: bool,
}

impl WindowsMediaOcr {
    pub fn new() -> Self {
        Self {
            ocr_lang: "auto".to_string(),
            enhance_contrast: true,
            preserve_line_breaks: false,
        }
    }

    pub fn with_options(ocr_lang: String, enhance_contrast: bool, preserve_line_breaks: bool) -> Self {
        Self {
            ocr_lang,
            enhance_contrast,
            preserve_line_breaks,
        }
    }

    /// 获取本机 Windows 系统当前已安装且支持的 OCR 语言列表
    pub fn available_languages() -> Vec<String> {
        let mut list = Vec::new();
        if let Ok(langs) = WinOcrEngine::AvailableRecognizerLanguages() {
            if let Ok(size) = langs.Size() {
                for i in 0..size {
                    if let Ok(lang) = langs.GetAt(i) {
                        if let Ok(tag) = lang.LanguageTag() {
                            list.push(tag.to_string());
                        }
                    }
                }
            }
        }
        list
    }

    /// 预热 Windows Media OCR 运行时与语言字典
    pub fn warmup(source_lang: &str) {
        log::info!("正在后台静默预热 Windows Media OCR 引擎 (语言: {})...", source_lang);
        let start = std::time::Instant::now();
        let _ = Self::available_languages();
        let _ = Self::create_engine_for_lang(source_lang);
        log::info!("Windows Media OCR 预热成功，耗时: {}ms", start.elapsed().as_millis());
    }

    /// 根据语言标识智能创建 Windows OCR 引擎，具备多级优雅降级策略
    fn create_engine_for_lang(lang_code: &str) -> Result<WinOcrEngine, SnipLingoError> {
        if lang_code != "auto" {
            // 常见语言代码映射候选表
            let candidates: Vec<&str> = match lang_code {
                "zh-Hans" => vec!["zh-Hans-CN", "zh-CN", "zh-Hans", "zh-SG"],
                "zh-Hant" => vec!["zh-Hant-TW", "zh-Hant-HK", "zh-TW", "zh-HK", "zh-Hant"],
                "en" => vec!["en-US", "en-GB", "en", "en-AU", "en-CA"],
                "ja" => vec!["ja-JP", "ja"],
                "ko" => vec!["ko-KR", "ko"],
                other => vec![other],
            };

            for cand in candidates {
                if let Ok(lang) = Language::CreateLanguage(&HSTRING::from(cand)) {
                    if WinOcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
                        if let Ok(engine) = WinOcrEngine::TryCreateFromLanguage(&lang) {
                            return Ok(engine);
                        }
                    }
                }
            }
        }

        // 默认/自动策略：优先使用当前用户系统语言档案 (通常已包含系统当前的首选语言+英语)
        if let Ok(engine) = WinOcrEngine::TryCreateFromUserProfileLanguages() {
            return Ok(engine);
        }

        // 降级策略：尝试英语
        if let Ok(en_lang) = Language::CreateLanguage(&HSTRING::from("en-US")) {
            if WinOcrEngine::IsLanguageSupported(&en_lang).unwrap_or(false) {
                if let Ok(engine) = WinOcrEngine::TryCreateFromLanguage(&en_lang) {
                    return Ok(engine);
                }
            }
        }

        // 终极降级：尝试已安装列表中的第一个语言
        if let Ok(langs) = WinOcrEngine::AvailableRecognizerLanguages() {
            if let Ok(size) = langs.Size() {
                if size > 0 {
                    if let Ok(first_lang) = langs.GetAt(0) {
                        if let Ok(engine) = WinOcrEngine::TryCreateFromLanguage(&first_lang) {
                            return Ok(engine);
                        }
                    }
                }
            }
        }

        Err(SnipLingoError::CaptureFailed(
            "未检测到 Windows OCR 语言包，请在 Windows 设置 -> 时间和语言 中安装对应语言的 OCR 支持".to_string(),
        ))
    }
}

impl Default for WindowsMediaOcr {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrEngine for WindowsMediaOcr {
    fn recognize_detailed(&self, img: &RgbaImage) -> Result<DetailedOcrResult, SnipLingoError> {
        let t_start = std::time::Instant::now();
        let width = img.width() as i32;
        let height = img.height() as i32;

        if width <= 0 || height <= 0 {
            return Err(SnipLingoError::NoTextDetected);
        }

        // 1. 暗色背景与低对比度增强预处理 (根据配置自动强化)
        let t_prep = std::time::Instant::now();
        let processed_img = preprocess_image_for_ocr(img, self.enhance_contrast);
        let raw_bytes = processed_img.as_raw();
        let prep_ms = t_prep.elapsed().as_millis();

        // 2. 将 RGBA 字节流写入 WinRT IBuffer
        let t_infer = std::time::Instant::now();
        let writer = DataWriter::new()
            .map_err(|e| SnipLingoError::CaptureFailed(format!("创建 DataWriter 失败: {}", e)))?;
        writer
            .WriteBytes(raw_bytes)
            .map_err(|e| SnipLingoError::CaptureFailed(format!("写入位图数据失败: {}", e)))?;
        let buffer = writer
            .DetachBuffer()
            .map_err(|e| SnipLingoError::CaptureFailed(format!("获取数据缓冲失败: {}", e)))?;

        // 3. 从缓冲创建 SoftwareBitmap (Rgba8)
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Rgba8,
            width,
            height,
        )
        .map_err(|e| SnipLingoError::CaptureFailed(format!("创建 SoftwareBitmap 失败: {}", e)))?;

        // 4. 多语言识别引擎匹配与初始化
        let engine = Self::create_engine_for_lang(&self.ocr_lang)?;

        // 5. 异步调用 Windows Media OCR
        let async_op = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| SnipLingoError::CaptureFailed(format!("发起 OCR 识别失败: {}", e)))?;
        let result = async_op
            .get()
            .map_err(|e| SnipLingoError::CaptureFailed(format!("等待 OCR 结果失败: {}", e)))?;
        let raw_text = result
            .Text()
            .map_err(|e| SnipLingoError::CaptureFailed(format!("提取文本失败: {}", e)))?
            .to_string();
        let infer_ms = t_infer.elapsed().as_millis();

        // 6. 提取行级包围盒 (汇总每行词级 BoundingRect)
        let mut blocks = Vec::new();
        if let Ok(lines) = result.Lines() {
            if let Ok(line_count) = lines.Size() {
                for i in 0..line_count {
                    if let Ok(line) = lines.GetAt(i) {
                        let line_text = line.Text().map(|t| t.to_string()).unwrap_or_default();
                        if line_text.trim().is_empty() {
                            continue;
                        }

                        let mut min_x = f32::MAX;
                        let mut min_y = f32::MAX;
                        let mut max_x = 0.0f32;
                        let mut max_y = 0.0f32;
                        let mut has_words = false;

                        if let Ok(words) = line.Words() {
                            if let Ok(word_count) = words.Size() {
                                for j in 0..word_count {
                                    if let Ok(word) = words.GetAt(j) {
                                        if let Ok(rect) = word.BoundingRect() {
                                            min_x = min_x.min(rect.X);
                                            min_y = min_y.min(rect.Y);
                                            max_x = max_x.max(rect.X + rect.Width);
                                            max_y = max_y.max(rect.Y + rect.Height);
                                            has_words = true;
                                        }
                                    }
                                }
                            }
                        }

                        if has_words && max_x > min_x && max_y > min_y {
                            let x = min_x.max(0.0) as u32;
                            let y = min_y.max(0.0) as u32;
                            let w = (max_x - min_x).max(1.0) as u32;
                            let h = (max_y - min_y).max(1.0) as u32;

                            blocks.push(OcrLineBlock {
                                text: line_text,
                                rect: OcrRect { x, y, width: w, height: h },
                                confidence: 1.0,
                            });
                        }
                    }
                }
            }
        }

        // 7. 智能文本清洗与排版格式优化
        let cleaned = clean_ocr_text_with_options(&raw_text, self.preserve_line_breaks);
        if cleaned.is_empty() {
            return Err(SnipLingoError::NoTextDetected);
        }

        log::debug!(
            "[性能] Windows Media OCR 识别完成 (图片: {}x{}): 预处理 {}ms, 推理 {}ms, 总耗时 {}ms",
            width,
            height,
            prep_ms,
            infer_ms,
            t_start.elapsed().as_millis()
        );

        Ok(DetailedOcrResult {
            full_text: cleaned,
            blocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_media_ocr_warmup() {
        WindowsMediaOcr::warmup("auto");
    }
}
