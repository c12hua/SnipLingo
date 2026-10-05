pub mod paddle_engine;
pub mod preprocess;
pub mod text_cleaner;
pub mod windows_media;

use crate::core::config::AppConfig;
use crate::core::error::SnipLingoError;
use image::RgbaImage;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct OcrRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct OcrLineBlock {
    pub text: String,
    pub rect: OcrRect,
    pub confidence: f32,
}

/// 先按全序排列，再以每行首个 y 为锚点分组，避免两两“接近”比较不满足传递性。
// ponytail: 沿用固定像素同行容差；多栏/倾斜版面需要时再升级为版面分析。
pub fn sort_lines_reading_order(lines: &mut [OcrLineBlock], tolerance: u32) {
    lines.sort_by_key(|line| (line.rect.y, line.rect.x));
    let mut start = 0;
    while start < lines.len() {
        let y = lines[start].rect.y;
        let end = start + lines[start..].partition_point(|line| line.rect.y - y < tolerance.max(1));
        lines[start..end].sort_by_key(|line| (line.rect.x, line.rect.y));
        start = end;
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DetailedOcrResult {
    pub full_text: String,
    pub blocks: Vec<OcrLineBlock>,
}

/// 可插拔 OCR 引擎 Trait
pub trait OcrEngine: Send + Sync {
    /// 保留识别行的原文；合并行只在翻译请求入口进行。
    fn recognize(&self, img: &RgbaImage) -> Result<String, SnipLingoError> {
        self.recognize_detailed(img).map(|res| res.full_text)
    }

    /// 结构化细节识别（返回文字与原始像素坐标）
    fn recognize_detailed(&self, img: &RgbaImage) -> Result<DetailedOcrResult, SnipLingoError>;
}

pub use paddle_engine::PaddleOcrEngine;
pub use windows_media::WindowsMediaOcr;

/// 统一 OCR 引擎工厂：根据配置动态提供选定的识别引擎实例 (避免引擎内部重复加载配置)
pub fn get_ocr_engine(config: &AppConfig) -> Box<dyn OcrEngine> {
    match config.ocr_engine.as_str() {
        "paddleocr" => Box::new(PaddleOcrEngine::with_options(
            ppocr_rs::PpOcrVersion::V6Tiny,
            config.enhance_contrast,
        )),
        _ => Box::new(WindowsMediaOcr::with_options(
            config.ocr_lang.clone(),
            config.enhance_contrast,
        )),
    }
}

/// 后台静默预热选定或可用的 OCR 引擎
pub fn warmup_ocr_engine(config: &AppConfig) {
    // 1. 始终轻量预热 Windows Media OCR（COM 运行时与已安装语言枚举，耗时仅 ~10ms）
    windows_media::WindowsMediaOcr::warmup(&config.source_lang);

    // 2. 若用户启用了 PaddleOCR，且本地模型文件已就绪，则预热 ONNX Runtime 推理 Session 与计算图
    if config.ocr_engine == "paddleocr" {
        let engine = PaddleOcrEngine::new();
        if PaddleOcrEngine::has_local_models(engine.version()) {
            if let Err(e) = engine.warmup() {
                log::warn!("PaddleOCR 静默预热失败 (不影响正常运行): {}", e);
            }
        } else {
            log::info!("PaddleOCR 本地模型尚未下载，跳过后台静默推理预热");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_order_has_stable_rows() {
        for tolerance in [6, 12] {
            let mut lines: Vec<_> = [(10, 8), (30, 0), (20, 4)].into_iter().map(|(x, y)| OcrLineBlock {
                text: x.to_string(), rect: OcrRect { x, y: y * tolerance / 6, width: 8, height: 4 }, confidence: 1.0,
            }).collect();
            sort_lines_reading_order(&mut lines, tolerance);
            assert_eq!(lines.iter().map(|l| l.rect.x).collect::<Vec<_>>(), [20, 30, 10]);
            sort_lines_reading_order(&mut lines, tolerance);
            assert_eq!(lines.iter().map(|l| l.rect.x).collect::<Vec<_>>(), [20, 30, 10]);
        }
        sort_lines_reading_order(&mut [], 0);
    }
}
