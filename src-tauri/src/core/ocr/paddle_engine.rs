use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use image::RgbaImage;
use ppocr_rs::{ModelHub, ModelPaths, OcrLite, PpOcrVersion};

use crate::core::error::SnipLingoError;
use crate::core::ocr::text_cleaner::clean_ocr_text_with_options;
use crate::core::ocr::{DetailedOcrResult, OcrLineBlock, OcrRect, OcrEngine};

static PADDLE_OCR_INSTANCE: OnceLock<Mutex<OcrLite>> = OnceLock::new();
static PADDLE_INIT_MUTEX: Mutex<()> = Mutex::new(());

pub struct PaddleOcrEngine {
    version: PpOcrVersion,
    enhance_contrast: bool,
    preserve_line_breaks: bool,
}

impl PaddleOcrEngine {
    pub fn new() -> Self {
        Self {
            version: PpOcrVersion::V6Tiny,
            enhance_contrast: true,
            preserve_line_breaks: false,
        }
    }

    pub fn with_options(version: PpOcrVersion, enhance_contrast: bool, preserve_line_breaks: bool) -> Self {
        Self {
            version,
            enhance_contrast,
            preserve_line_breaks,
        }
    }

    pub fn version(&self) -> PpOcrVersion {
        self.version
    }

    /// 获取模型存储缓存目录
    pub fn get_models_dir() -> PathBuf {
        if let Ok(app_data) = std::env::var("APPDATA") {
            let dir = PathBuf::from(app_data)
                .join("SnipLingo")
                .join("models")
                .join("paddleocr");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }
        std::env::temp_dir().join("sniplingo_paddleocr")
    }

    /// 检测本地是否已经存在完整的 PaddleOCR 模型权重与字典文件 (无需发起网络下载)
    pub fn find_local_models(version: PpOcrVersion) -> Option<ModelPaths> {
        let dir_name = match version {
            PpOcrVersion::V6Tiny => "pp_ocrv6_tiny",
            PpOcrVersion::V6Small => "pp_ocrv6_small",
            PpOcrVersion::V6Medium => "pp_ocrv6_medium",
        };

        // 1. 优先检测应用程序自身目录或 resources 目录是否已内置打包模型
        let mut candidates = Vec::new();
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                candidates.push(exe_dir.join("models").join(dir_name));
                candidates.push(exe_dir.join("resources").join("models").join(dir_name));
                candidates.push(exe_dir.join(dir_name));
                if let Some(target_dir) = exe_dir.parent() {
                    candidates.push(target_dir.join("resources").join("models").join(dir_name));
                }
            }
        }
        candidates.push(PathBuf::from("resources").join("models").join(dir_name));
        candidates.push(PathBuf::from("src-tauri").join("resources").join("models").join(dir_name));

        for cand in candidates {
            let det = cand.join("det.onnx");
            let rec = cand.join("rec.onnx");
            let dict = cand.join("dict.txt");
            if det.exists() && rec.exists() && dict.exists() {
                log::info!("找到本地 PaddleOCR 模型: {}", cand.display());
                return Some(ModelPaths {
                    det_onnx: det,
                    rec_onnx: rec,
                    dict_txt: dict,
                    rec_yml: cand.join("rec_inference.yml"),
                });
            }
        }

        // 2. 检测用户数据缓存目录 (%APPDATA%\SnipLingo\models\paddleocr)
        let dir = Self::get_models_dir();
        let cached_dir = dir.join(dir_name);
        let cached_det = cached_dir.join("det.onnx");
        let cached_rec = cached_dir.join("rec.onnx");
        let cached_dict = cached_dir.join("dict.txt");
        if cached_det.exists() && cached_rec.exists() && cached_dict.exists() {
            return Some(ModelPaths {
                det_onnx: cached_det,
                rec_onnx: cached_rec,
                dict_txt: cached_dict,
                rec_yml: cached_dir.join("rec_inference.yml"),
            });
        }

        None
    }

    /// 本地是否存在模型文件
    pub fn has_local_models(version: PpOcrVersion) -> bool {
        Self::find_local_models(version).is_some()
    }

    /// 确保模型已就绪：优先检测本地目录，缺失时才从网络下载
    pub fn ensure_models(version: PpOcrVersion) -> Result<ModelPaths, SnipLingoError> {
        if let Some(paths) = Self::find_local_models(version) {
            return Ok(paths);
        }

        // 3. 本地无模型时，执行自动静默下载
        let dir = Self::get_models_dir();
        std::fs::create_dir_all(&dir).map_err(|e| {
            SnipLingoError::OcrFailed(format!("创建 PaddleOCR 模型目录失败: {}", e))
        })?;

        let hub = ModelHub::new(&dir);
        match hub.ensure(version) {
            Ok(paths) => Ok(paths),
            Err(e) => {
                log::warn!("PaddleOCR 默认源下载失败，尝试使用国内高速镜像源: {}", e);
                Self::download_from_mirror(&dir, version)
            }
        }
    }

    /// 预热 PaddleOCR 引擎：加载 ONNX Runtime 运行库与模型 Session，
    /// 并用一张微型空白图像（32x32）执行一次极速推理，
    /// 使 ONNX Runtime 提前完成内存池分配与计算图编译。
    pub fn warmup(&self) -> Result<(), SnipLingoError> {
        log::info!("正在后台静默预热 PaddleOCR 引擎...");
        let start = std::time::Instant::now();
        let _ = self.get_ocr_engine_instance()?;
        let dummy = RgbaImage::new(32, 32);
        let _ = self.recognize_detailed(&dummy);
        log::info!("PaddleOCR 引擎预热成功，耗时: {}ms", start.elapsed().as_millis());
        Ok(())
    }

    /// 镜像源备用下载
    fn download_from_mirror(base_dir: &std::path::Path, version: PpOcrVersion) -> Result<ModelPaths, SnipLingoError> {
        let (det_repo, rec_repo, dir_name) = match version {
            PpOcrVersion::V6Tiny => (
                "PP-OCRv6_tiny_det_onnx",
                "PP-OCRv6_tiny_rec_onnx",
                "pp_ocrv6_tiny",
            ),
            PpOcrVersion::V6Small => (
                "PP-OCRv6_small_det_onnx",
                "PP-OCRv6_small_rec_onnx",
                "pp_ocrv6_small",
            ),
            PpOcrVersion::V6Medium => (
                "PP-OCRv6_medium_det_onnx",
                "PP-OCRv6_medium_rec_onnx",
                "pp_ocrv6_medium",
            ),
        };

        let target_dir = base_dir.join(dir_name);
        std::fs::create_dir_all(&target_dir).map_err(|e| {
            SnipLingoError::OcrFailed(format!("创建目录失败: {}", e))
        })?;

        let det_path = target_dir.join("det.onnx");
        let rec_path = target_dir.join("rec.onnx");
        let rec_yml = target_dir.join("rec_inference.yml");
        let dict_path = target_dir.join("dict.txt");

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| SnipLingoError::OcrFailed(format!("创建 HTTP 客户端失败: {}", e)))?;

        // 1. 下载 det.onnx
        if !det_path.exists() || std::fs::metadata(&det_path).map(|m| m.len()).unwrap_or(0) == 0 {
            let url = format!("https://hf-mirror.com/PaddlePaddle/{}/resolve/main/inference.onnx", det_repo);
            Self::download_file(&client, &url, &det_path)?;
        }

        // 2. 下载 rec.onnx
        if !rec_path.exists() || std::fs::metadata(&rec_path).map(|m| m.len()).unwrap_or(0) == 0 {
            let url = format!("https://hf-mirror.com/PaddlePaddle/{}/resolve/main/inference.onnx", rec_repo);
            Self::download_file(&client, &url, &rec_path)?;
        }

        // 3. 下载 rec_inference.yml
        if !rec_yml.exists() || std::fs::metadata(&rec_yml).map(|m| m.len()).unwrap_or(0) == 0 {
            let url = format!("https://hf-mirror.com/PaddlePaddle/{}/resolve/main/inference.yml", rec_repo);
            Self::download_file(&client, &url, &rec_yml)?;
        }

        // 4. 从 YAML 提取词典 dict.txt
        if !dict_path.exists() || std::fs::metadata(&dict_path).map(|m| m.len()).unwrap_or(0) == 0 {
            Self::extract_dict(&rec_yml, &dict_path)?;
        }

        Ok(ModelPaths {
            det_onnx: det_path,
            rec_onnx: rec_path,
            dict_txt: dict_path,
            rec_yml,
        })
    }

    fn download_file(client: &reqwest::blocking::Client, url: &str, dest: &std::path::Path) -> Result<(), SnipLingoError> {
        log::info!("正在下载 PaddleOCR 模型文件: {} -> {}", url, dest.display());
        let resp = client.get(url).send().map_err(|e| {
            SnipLingoError::OcrFailed(format!("下载模型失败 {}: {}", url, e))
        })?;

        if !resp.status().is_success() {
            return Err(SnipLingoError::OcrFailed(format!(
                "下载模型 HTTP 状态码异常 {}: {}",
                url,
                resp.status()
            )));
        }

        let bytes = resp.bytes().map_err(|e| {
            SnipLingoError::OcrFailed(format!("读取模型数据失败: {}", e))
        })?;

        std::fs::write(dest, bytes).map_err(|e| {
            SnipLingoError::OcrFailed(format!("写入模型文件失败: {}", e))
        })?;

        Ok(())
    }

    /// 从 rec_inference.yml 提取 character_dict 到 dict.txt
    fn extract_dict(yml_path: &std::path::Path, dict_path: &std::path::Path) -> Result<(), SnipLingoError> {
        let content = std::fs::read_to_string(yml_path).map_err(|e| {
            SnipLingoError::OcrFailed(format!("读取 inference.yml 失败: {}", e))
        })?;

        let mut chars = Vec::new();
        let mut in_dict = false;

        for line in content.lines() {
            if !in_dict {
                if line.trim_start().starts_with("character_dict:") {
                    in_dict = true;
                }
                continue;
            }

            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("- ") {
                let rest_trimmed = rest.trim_end_matches('\r');
                let ch = if rest_trimmed.starts_with('\'') && rest_trimmed.ends_with('\'') && rest_trimmed.len() >= 2 {
                    rest_trimmed[1..rest_trimmed.len() - 1].replace("''", "'")
                } else {
                    rest_trimmed.to_string()
                };
                chars.push(ch);
            } else if !trimmed.is_empty() && !trimmed.starts_with('-') {
                break;
            }
        }

        if chars.is_empty() {
            return Err(SnipLingoError::OcrFailed("character_dict 在 YAML 中未找到".to_string()));
        }

        let mut output = String::new();
        for c in chars {
            output.push_str(&c);
            output.push('\n');
        }

        std::fs::write(dict_path, output).map_err(|e| {
            SnipLingoError::OcrFailed(format!("保存 dict.txt 失败: {}", e))
        })?;

        Ok(())
    }

    /// 初始化 ONNX Runtime 环境
    fn init_ort_environment() {
        let mut candidates = Vec::new();
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                candidates.push(exe_dir.join("onnxruntime.dll"));
                candidates.push(exe_dir.join("resources").join("onnxruntime.dll"));
                if let Some(target_dir) = exe_dir.parent() {
                    candidates.push(target_dir.join("resources").join("onnxruntime.dll"));
                    candidates.push(target_dir.join("onnxruntime.dll"));
                }
            }
        }
        candidates.push(PathBuf::from("resources").join("onnxruntime.dll"));
        candidates.push(PathBuf::from("src-tauri").join("resources").join("onnxruntime.dll"));
        candidates.push(Self::get_models_dir().join("onnxruntime.dll"));

        for cand in candidates {
            if cand.exists() {
                log::info!("正在加载 ONNX Runtime 运行库: {}", cand.display());
                let _ = ort::init_from(cand.to_string_lossy().to_string()).commit();
                return;
            }
        }
        let _ = ort::init().commit();
    }

    /// 获取或初始化共享的 OcrLite 单例（带双重检查锁与失败可重试保证）
    fn get_ocr_engine_instance(&self) -> Result<&'static Mutex<OcrLite>, SnipLingoError> {
        if let Some(instance) = PADDLE_OCR_INSTANCE.get() {
            return Ok(instance);
        }

        let _init_guard = PADDLE_INIT_MUTEX.lock().map_err(|_| {
            SnipLingoError::OcrFailed("获取 PaddleOCR 初始化锁失败".into())
        })?;

        if let Some(instance) = PADDLE_OCR_INSTANCE.get() {
            return Ok(instance);
        }

        let t_init = std::time::Instant::now();
        Self::init_ort_environment();
        let paths = Self::ensure_models(self.version)?;

        let mut ocr = OcrLite::new();
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(4))
            .unwrap_or(2);

        ocr.init_models_no_angle(
            paths.det_onnx.to_str().ok_or_else(|| SnipLingoError::OcrFailed("det path invalid".into()))?,
            paths.rec_onnx.to_str().ok_or_else(|| SnipLingoError::OcrFailed("rec path invalid".into()))?,
            paths.dict_txt.to_str().ok_or_else(|| SnipLingoError::OcrFailed("dict path invalid".into()))?,
            num_threads,
        ).map_err(|e| SnipLingoError::OcrFailed(format!("PaddleOCR 模型初始化失败: {}", e)))?;

        log::debug!("[性能] PaddleOCR 实例首次初始化完成，耗时: {}ms", t_init.elapsed().as_millis());
        let _ = PADDLE_OCR_INSTANCE.set(Mutex::new(ocr));
        Ok(PADDLE_OCR_INSTANCE.get().unwrap())
    }
}

impl OcrEngine for PaddleOcrEngine {
    fn recognize_detailed(&self, img: &RgbaImage) -> Result<DetailedOcrResult, SnipLingoError> {
        let t_start = std::time::Instant::now();

        // 1. 预处理增强并直接生成推理所需的 RGB 格式 (单趟转换，不分配多余中间 RGBA 缓冲)
        let t_prep = std::time::Instant::now();
        let rgb_img = crate::core::ocr::preprocess::preprocess_image_to_rgb(img, self.enhance_contrast);
        let prep_ms = t_prep.elapsed().as_millis();

        // 2. 获取单例并执行推理
        let t_infer = std::time::Instant::now();
        let engine_lock = self.get_ocr_engine_instance()?;
        let mut engine = engine_lock.lock().map_err(|_| {
            SnipLingoError::OcrFailed("获取 PaddleOCR 实例锁失败".to_string())
        })?;

        let result = engine.detect(
            &rgb_img,
            50,   // padding
            1024, // max_side_len
            0.5,  // box_score_thresh
            0.3,  // box_thresh
            1.6,  // un_clip_ratio
            false, // do_angle (without angle cls)
            false, // most_angle
        ).map_err(|e| SnipLingoError::OcrFailed(format!("PaddleOCR 文本识别失败: {}", e)))?;
        let infer_ms = t_infer.elapsed().as_millis();

        // 3. 几何排序（自然阅读顺序：由上至下，同水平行由左至右）
        let mut raw_blocks = result.text_blocks;
        raw_blocks.sort_by(|a, b| {
            let a_y = a.box_points.iter().map(|p| p.y).min().unwrap_or(0);
            let b_y = b.box_points.iter().map(|p| p.y).min().unwrap_or(0);
            let a_x = a.box_points.iter().map(|p| p.x).min().unwrap_or(0);
            let b_x = b.box_points.iter().map(|p| p.x).min().unwrap_or(0);

            // 若垂直坐标差在 12 像素以内，视为同一行，按水平坐标从左向右排
            if (a_y as i64 - b_y as i64).abs() < 12 {
                a_x.cmp(&b_x)
            } else {
                a_y.cmp(&b_y)
            }
        });

        let mut blocks = Vec::new();
        let mut valid_texts = Vec::new();

        for b in raw_blocks {
            if b.text_score < 0.5 || b.text.trim().is_empty() {
                continue;
            }

            let min_x = b.box_points.iter().map(|p| p.x).min().unwrap_or(0).max(0) as u32;
            let min_y = b.box_points.iter().map(|p| p.y).min().unwrap_or(0).max(0) as u32;
            let max_x = b.box_points.iter().map(|p| p.x).max().unwrap_or(0).max(0) as u32;
            let max_y = b.box_points.iter().map(|p| p.y).max().unwrap_or(0).max(0) as u32;
            let width = max_x.saturating_sub(min_x).max(1);
            let height = max_y.saturating_sub(min_y).max(1);

            valid_texts.push(b.text.clone());
            blocks.push(OcrLineBlock {
                text: b.text,
                rect: OcrRect { x: min_x, y: min_y, width, height },
                confidence: b.text_score,
            });
        }

        let raw_text = valid_texts.join("\n");
        if raw_text.trim().is_empty() {
            return Err(SnipLingoError::NoTextDetected);
        }

        // 4. 应用针对中文优化的排版清洁器
        let cleaned = clean_ocr_text_with_options(&raw_text, self.preserve_line_breaks);

        log::debug!(
            "[性能] PaddleOCR 识别完成 (图片: {}x{}): 预处理 {}ms, 推理 {}ms, 总耗时 {}ms",
            img.width(),
            img.height(),
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
    fn test_extract_dict_logic() {
        let sample_yml = r#"
Global:
  model_type: rec
PostProcess:
  name: CTCLabelDecode
  character_dict:
    - 'a'
    - 'b'
    - '中'
    - '文'
"#;
        let tmp_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("test_tmp");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let tmp_yml = tmp_dir.join("test_rec_inf.yml");
        let tmp_dict = tmp_dir.join("test_dict.txt");

        std::fs::write(&tmp_yml, sample_yml).unwrap();
        PaddleOcrEngine::extract_dict(&tmp_yml, &tmp_dict).unwrap();

        let dict_content = std::fs::read_to_string(&tmp_dict).unwrap();
        let lines: Vec<&str> = dict_content.lines().collect();
        assert_eq!(lines, vec!["a", "b", "中", "文"]);

        let _ = std::fs::remove_file(tmp_yml);
        let _ = std::fs::remove_file(tmp_dict);
    }

    #[test]
    fn test_paddle_ocr_ensure_and_infer() {
        let engine = PaddleOcrEngine::new();
        // 构造一个简单的测试图像
        let img = RgbaImage::from_pixel(100, 40, image::Rgba([255, 255, 255, 255]));
        let res = engine.recognize(&img);
        eprintln!("PaddleOCR blank image test result: {:?}", res);
        assert!(matches!(res, Err(SnipLingoError::NoTextDetected)));
    }

    #[test]
    fn test_paddle_ocr_warmup_and_find_models() {
        let engine = PaddleOcrEngine::new();
        if PaddleOcrEngine::has_local_models(engine.version()) {
            let found = PaddleOcrEngine::find_local_models(engine.version());
            assert!(found.is_some());
            let warmup_res = engine.warmup();
            assert!(warmup_res.is_ok());
        }
    }

    #[test]
    fn test_paddle_ocr_concurrent_init() {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    let engine = PaddleOcrEngine::new();
                    let img = RgbaImage::from_pixel(50, 20, image::Rgba([255, 255, 255, 255]));
                    let _ = engine.recognize(&img);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
    }
}
