use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

// ponytail: 配置保存/首次开发配置迁移共用进程锁；多进程仍为最后一次完整写入生效，
// 若将来允许多实例同时编辑配置，再增加文件锁或版本冲突检测。
static CONFIG_SAVE: Mutex<Option<(PathBuf, bool)>> = Mutex::new(None);
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AppConfig {
    pub app_language: String, // 整个工具显示语言: "zh-CN" | "en" | "zh-TW"
    pub auto_start: bool, // 开机自启动
    pub silent_start: bool, // 静默启动：启动后直接最小化至系统托盘，不弹出主界面
    pub hotkey: String, // 全局截图唤起快捷键
    pub action_copy_shortcut: String, // 截屏选区操作: 复制图片
    pub action_ocr_shortcut: String, // 截屏选区操作: 获取文本 (OCR)
    pub action_qrcode_shortcut: String, // 截屏选区操作: 识别二维码
    pub action_translate_shortcut: String, // 截屏选区操作: 翻译
    pub action_pin_shortcut: String, // 截屏选区操作: 钉住
    pub pin_shadow: bool, // 钉住截图是否开启立体阴影边框
    pub pin_opacity: u32, // 贴图不透明度百分比: 10..=100
    pub ocr_engine: String, // "windows" | "ocrs"
    pub ocr_lang: String, // "auto" | "zh-Hans" | "en" | "ja" | "ko" | "zh-Hant"
    pub preserve_line_breaks: bool, // 代码/日志排版优化：是否保持换行
    pub enhance_contrast: bool, // 暗色主题/低对比度文字增强
    pub qrcode_open_in_browser: bool, // 二维码识别：false=复制内容到剪贴板 true=网址直接用默认浏览器打开
    pub in_place_translate: bool, // 是否在截图选区上直接覆盖显示译文卡片
    pub provider: String, // "google" | "baidu" | "openai" | "deepl"
    pub api_key: String, // for OpenAI, DeepL, or Baidu App ID (旧版通用兼容)
    pub secret_key: Option<String>, // for Baidu Secret Key (旧版通用兼容)
    pub base_url: Option<String>, // e.g. "https://api.openai.com/v1" (旧版通用兼容)
    pub model: Option<String>, // e.g. "gpt-4o-mini" (旧版通用兼容)
    // 多引擎独立凭据记忆（切换服务商时不互相覆盖）
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub deepl_api_key: Option<String>,
    pub baidu_app_id: Option<String>,
    pub baidu_secret_key: Option<String>,
    pub source_lang: String, // 翻译源语言: "auto" | "zh-CN" | "en" | ...
    pub target_lang: String, // 翻译目标语言: "zh-CN" | "en" | ...
}

impl AppConfig {
    /// 获取 OpenAI 引擎凭据（优先取独立存储，平滑回退至通用字段）
    pub fn get_openai_credentials(&self) -> (String, Option<String>, Option<String>) {
        let key = self
            .openai_api_key
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(if self.provider == "openai" { &self.api_key } else { "" })
            .to_string();
        let base_url = self
            .openai_base_url
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| self.base_url.clone().filter(|s| !s.is_empty()))
            .or_else(|| Some("https://api.openai.com/v1".to_string()));
        let model = self
            .openai_model
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| self.model.clone().filter(|s| !s.is_empty()))
            .or_else(|| Some("gpt-4o-mini".to_string()));
        (key, base_url, model)
    }

    /// 获取 DeepL 引擎凭据（优先取独立存储，平滑回退至通用字段）
    pub fn get_deepl_credentials(&self) -> String {
        self.deepl_api_key
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(if self.provider == "deepl" { &self.api_key } else { "" })
            .to_string()
    }

    /// 获取百度翻译引擎凭据（优先取独立存储，平滑回退至通用字段）
    pub fn get_baidu_credentials(&self) -> (String, Option<String>) {
        let app_id = self
            .baidu_app_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(if self.provider == "baidu" { &self.api_key } else { "" })
            .to_string();
        let secret = self
            .baidu_secret_key
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| self.secret_key.clone().filter(|s| !s.is_empty()));
        (app_id, secret)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_language: "zh-CN".to_string(),
            auto_start: false,
            silent_start: true,
            hotkey: "F4".to_string(),
            action_copy_shortcut: "Ctrl+C".to_string(),
            action_ocr_shortcut: "Ctrl+T".to_string(),
            action_qrcode_shortcut: "Ctrl+Q".to_string(),
            action_translate_shortcut: "Ctrl+S".to_string(),
            action_pin_shortcut: "Ctrl+P".to_string(),
            pin_shadow: false,
            pin_opacity: 100,
            ocr_engine: "windows".to_string(),
            ocr_lang: "auto".to_string(),
            preserve_line_breaks: false,
            enhance_contrast: true,
            qrcode_open_in_browser: false,
            in_place_translate: true,
            provider: "google".to_string(), // 默认启用谷歌翻译 (免配置 Key，开箱即用)
            api_key: "".to_string(),
            secret_key: None,
            base_url: Some("https://api.openai.com/v1".to_string()),
            model: Some("gpt-4o-mini".to_string()),
            openai_api_key: None,
            openai_base_url: None,
            openai_model: None,
            deepl_api_key: None,
            baidu_app_id: None,
            baidu_secret_key: None,
            source_lang: "auto".to_string(),
            target_lang: "zh-CN".to_string(),
        }
    }
}

fn get_config_path() -> PathBuf {
    let filename = if cfg!(debug_assertions) {
        "config.dev.json"
    } else {
        "config.json"
    };
    if let Ok(app_data) = std::env::var("APPDATA") {
        let dir = PathBuf::from(app_data).join("SnipLingo");
        let _ = fs::create_dir_all(&dir);
        dir.join(filename)
    } else {
        PathBuf::from(filename)
    }
}

fn read_config(path: &Path) -> Result<Option<AppConfig>, String> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).map(Some).map_err(|e| {
            // 解析错误可能包含凭据原文，只报告位置，不输出错误中的内容。
            format!("配置文件 {} 格式错误（第 {} 行，第 {} 列），请先修复或备份移走该文件", path.display(), e.line(), e.column())
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取配置文件 {} 失败: {}", path.display(), e)),
    }
}

pub fn load_config() -> AppConfig {
    let path = get_config_path();
    match read_config(&path) {
        Ok(Some(config)) => return config,
        Ok(None) => {}
        Err(e) => {
            log::warn!("{}；暂用默认配置，原文件未修改", e);
            return AppConfig::default();
        }
    }
    // 仅当开发配置不存在时继承正式版偏好；损坏配置不能被默认值/正式版配置覆盖。
    if cfg!(debug_assertions) {
        if let Ok(app_data) = std::env::var("APPDATA") {
            let prod_path = PathBuf::from(app_data).join("SnipLingo").join("config.json");
            match read_config(&prod_path) {
                Ok(Some(mut config)) => {
                    config.auto_start = false;
                    if let Ok(_guard) = CONFIG_SAVE.lock() {
                        // 等锁期间另一线程可能已保存开发配置，不能再覆盖它。
                        match read_config(&path) {
                            Ok(Some(saved)) => return saved,
                            Ok(None) => {
                                if let Err(e) = write_config(&path, &config) {
                                    log::warn!("初始化开发配置失败: {}", e);
                                }
                            }
                            Err(e) => {
                                log::warn!("{}；暂用默认配置，原文件未修改", e);
                                return AppConfig::default();
                            }
                        }
                    }
                    return config;
                }
                Ok(None) => {}
                Err(e) => log::warn!("无法继承正式版配置: {}", e),
            }
        }
    }
    AppConfig::default()
}

#[cfg(target_os = "windows")]
pub fn sync_autostart_registry(enabled: bool) {
    // 关键防御：开发/调试模式下禁止写入系统 Run 注册表，防止覆写已安装稳定版的开机启动项
    if cfg!(debug_assertions) {
        log::info!("开发调试模式下跳过开机自启动注册表同步，保持稳定版开机启动项不被覆盖");
        return;
    }

    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy().to_string();
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if enabled {
            let _ = std::process::Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "SnipLingo",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{}\"", exe_str),
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        } else {
            let _ = std::process::Command::new("reg")
                .args([
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "SnipLingo",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn sync_autostart_registry(_enabled: bool) {}

fn write_config(path: &Path, config: &AppConfig) -> Result<(), String> {
    // 读取异常不是“首次保存”：拒绝覆盖无法读取/解析的原配置。
    read_config(path)?;
    let json = serde_json::to_vec_pretty(config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    let (temp_path, mut file) = loop {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let temp_path = path.with_extension(format!("{}.{}.tmp", std::process::id(), id));
        match fs::OpenOptions::new().write(true).create_new(true).open(&temp_path) {
            Ok(file) => break (temp_path, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("创建配置临时文件失败: {}", e)),
        }
    };
    let result = (|| -> io::Result<()> {
        file.write_all(&json)?;
        file.sync_all()?;
        drop(file);
        // 同目录替换，Windows 的 std::fs::rename 也支持替换已有文件。
        fs::rename(&temp_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result.map_err(|e| format!("保存配置失败，原配置未修改: {}", e))
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let mut saved = CONFIG_SAVE.lock().map_err(|_| "配置保存锁不可用".to_string())?;
    let path = get_config_path();
    write_config(&path, config)?;

    // 每个配置路径首次保存或开机启动设置变化时同步，普通设置修改不再启动 reg.exe。
    let autostart = (path, config.auto_start);
    if saved.as_ref() != Some(&autostart) {
        sync_autostart_registry(config.auto_start);
        *saved = Some(autostart);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_atomic_save_and_failures() {
        let dir = std::env::temp_dir().join(format!(
            "sniplingo-config-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("config.json");
        assert!(read_config(&path).unwrap().is_none());

        let config = AppConfig::default();
        write_config(&path, &config).unwrap();
        assert_eq!(read_config(&path).unwrap().unwrap().hotkey, config.hotkey);
        let changed = AppConfig { hotkey: "F8".to_string(), ..config };
        write_config(&path, &changed).unwrap();
        assert_eq!(read_config(&path).unwrap().unwrap().hotkey, "F8");

        // 并发读写只能看到完整的旧/新 JSON；每次保存的临时文件相互隔离。
        std::thread::scope(|scope| {
            for i in 0..4 {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..4 {
                        let config = AppConfig { pin_opacity: 90 + i, ..AppConfig::default() };
                        write_config(path, &config).unwrap();
                        assert!((90..=100).contains(&read_config(path).unwrap().unwrap().pin_opacity));
                    }
                });
            }
        });
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let original = fs::read(&path).unwrap();
            // 允许读写但不允许删除/替换，强制在临时文件写好之后 rename 失败。
            let held = fs::OpenOptions::new().read(true).share_mode(3).open(&path).unwrap();
            assert!(write_config(&path, &changed).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
            drop(held);
        }

        // 已损坏的文件必须保留，解析错误也不能泄露其中的凭据。
        let invalid = r#"{"auto_start":"private-test-value"}"#;
        fs::write(&path, invalid).unwrap();
        let error = write_config(&path, &changed).unwrap_err();
        assert!(!error.contains("private-test-value"));
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        assert!(write_config(&dir.join("missing/config.json"), &changed).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_config_default_and_serialization() {
        let config = AppConfig::default();
        assert_eq!(config.provider, "google");
        assert_eq!(config.source_lang, "auto");
        assert_eq!(config.target_lang, "zh-CN");
        assert_eq!(config.app_language, "zh-CN");
        assert_eq!(config.auto_start, false);
        assert_eq!(config.silent_start, true);
        assert_eq!(config.action_copy_shortcut, "Ctrl+C");
        assert_eq!(config.action_ocr_shortcut, "Ctrl+T");
        assert_eq!(config.action_translate_shortcut, "Ctrl+S");
        assert_eq!(config.action_pin_shortcut, "Ctrl+P");
        assert_eq!(config.pin_opacity, 100);
        assert_eq!(config.ocr_engine, "windows");
        assert_eq!(config.in_place_translate, true);

        let json = serde_json::to_string(&config).expect("序列化应该成功");
        let deserialized: AppConfig = serde_json::from_str(&json).expect("反序列化应该成功");
        assert_eq!(deserialized.provider, config.provider);
        assert_eq!(deserialized.silent_start, true);
        assert_eq!(deserialized.base_url, config.base_url);
        assert_eq!(deserialized.source_lang, "auto");
        assert_eq!(deserialized.target_lang, "zh-CN");
        assert_eq!(deserialized.pin_opacity, 100);
        assert_eq!(deserialized.ocr_engine, "windows");
        assert_eq!(deserialized.in_place_translate, true);
        assert_eq!(deserialized.action_copy_shortcut, "Ctrl+C");
        assert_eq!(deserialized.action_ocr_shortcut, "Ctrl+T");
    }

    #[test]
    fn test_multi_engine_credentials_isolation() {
        let mut config = AppConfig::default();
        config.openai_api_key = Some("sk-openai-isolated".to_string());
        config.openai_base_url = Some("https://api.deepseek.com/v1".to_string());
        config.openai_model = Some("deepseek-chat".to_string());
        config.deepl_api_key = Some("deepl-isolated-key".to_string());
        config.baidu_app_id = Some("baidu-12345".to_string());
        config.baidu_secret_key = Some("baidu-secret-xyz".to_string());

        // 测试隔离提取
        let (oa_key, oa_url, oa_model) = config.get_openai_credentials();
        assert_eq!(oa_key, "sk-openai-isolated");
        assert_eq!(oa_url.as_deref(), Some("https://api.deepseek.com/v1"));
        assert_eq!(oa_model.as_deref(), Some("deepseek-chat"));

        assert_eq!(config.get_deepl_credentials(), "deepl-isolated-key");

        let (bd_id, bd_sec) = config.get_baidu_credentials();
        assert_eq!(bd_id, "baidu-12345");
        assert_eq!(bd_sec.as_deref(), Some("baidu-secret-xyz"));

        // 测试反向平滑兼容（旧版本无独立字段时，自动回退到通用 api_key/secret_key）
        let legacy_openai = AppConfig {
            provider: "openai".to_string(),
            api_key: "sk-legacy-key".to_string(),
            base_url: Some("https://legacy.url".to_string()),
            model: Some("legacy-model".to_string()),
            ..AppConfig::default()
        };
        let (leg_k, leg_u, leg_m) = legacy_openai.get_openai_credentials();
        assert_eq!(leg_k, "sk-legacy-key");
        assert_eq!(leg_u.as_deref(), Some("https://legacy.url"));
        assert_eq!(leg_m.as_deref(), Some("legacy-model"));
    }

    #[test]
    fn test_config_path_debug_separation() {
        let path = get_config_path();
        if cfg!(debug_assertions) {
            assert!(path.to_string_lossy().ends_with("config.dev.json"));
        } else {
            assert!(path.to_string_lossy().ends_with("config.json"));
        }
    }
}
