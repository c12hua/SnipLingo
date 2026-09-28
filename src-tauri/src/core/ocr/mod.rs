pub mod paddle_engine;
pub mod preprocess;
pub mod text_cleaner;
pub mod windows_media;

use crate::core::config::AppConfig;
use crate::core::error::SnipLingoError;
use image::RgbaImage;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OcrRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct OcrLineBlock {
    pub text: String,
    pub rect: OcrRect,
    pub confidence: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct DetailedOcrResult {
    pub full_text: String,
    pub blocks: Vec<OcrLineBlock>,
}

/// 可插拔 OCR 引擎 Trait
pub trait OcrEngine: Send + Sync {
    /// 纯文本识别（向后兼容）
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
            config.preserve_line_breaks,
        )),
        _ => Box::new(WindowsMediaOcr::with_options(
            config.ocr_lang.clone(),
            config.enhance_contrast,
            config.preserve_line_breaks,
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
