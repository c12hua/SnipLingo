use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_app_language() -> String {
    "zh-CN".to_string()
}

fn default_auto_start() -> bool {
    false
}

fn default_silent_start() -> bool {
    true
}

fn default_action_copy_shortcut() -> String {
    "Ctrl+C".to_string()
}

fn default_action_translate_shortcut() -> String {
    "Ctrl+S".to_string()
}

fn default_action_ocr_shortcut() -> String {
    "Ctrl+T".to_string()
}

fn default_action_pin_shortcut() -> String {
    "Ctrl+P".to_string()
}

fn default_hotkey() -> String {
    "F4".to_string()
}

fn default_pin_shadow() -> bool {
    false
}

fn default_pin_opacity() -> u32 {
    100
}

fn default_ocr_lang() -> String {
    "auto".to_string()
}

fn default_preserve_line_breaks() -> bool {
    false
}

fn default_enhance_contrast() -> bool {
    true
}

fn default_ocr_engine() -> String {
    "windows".to_string()
}

fn default_in_place_translate() -> bool {
    true
}

fn default_source_lang() -> String {
    "auto".to_string()
}

fn default_target_lang() -> String {
    "zh-CN".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    #[serde(default = "default_app_language")]
    pub app_language: String, // 整个工具显示语言: "zh-CN" | "en" | "zh-TW"
    #[serde(default = "default_auto_start")]
    pub auto_start: bool, // 开机自启动
    #[serde(default = "default_silent_start")]
    pub silent_start: bool, // 静默启动：启动后直接最小化至系统托盘，不弹出主界面
    #[serde(default = "default_hotkey")]
    pub hotkey: String, // 全局截图唤起快捷键
    #[serde(default = "default_action_copy_shortcut")]
    pub action_copy_shortcut: String, // 截屏选区操作: 复制图片
    #[serde(default = "default_action_ocr_shortcut")]
    pub action_ocr_shortcut: String, // 截屏选区操作: 获取文本 (OCR)
    #[serde(default = "default_action_translate_shortcut")]
    pub action_translate_shortcut: String, // 截屏选区操作: 翻译
    #[serde(default = "default_action_pin_shortcut")]
    pub action_pin_shortcut: String, // 截屏选区操作: 钉住
    #[serde(default = "default_pin_shadow")]
    pub pin_shadow: bool, // 钉住截图是否开启立体阴影边框
    #[serde(default = "default_pin_opacity")]
    pub pin_opacity: u32, // 贴图不透明度百分比: 10..=100
    #[serde(default = "default_ocr_engine")]
    pub ocr_engine: String, // "windows" | "ocrs"
    #[serde(default = "default_ocr_lang")]
    pub ocr_lang: String, // "auto" | "zh-Hans" | "en" | "ja" | "ko" | "zh-Hant"
    #[serde(default = "default_preserve_line_breaks")]
    pub preserve_line_breaks: bool, // 代码/日志排版优化：是否保持换行
    #[serde(default = "default_enhance_contrast")]
    pub enhance_contrast: bool, // 暗色主题/低对比度文字增强
    #[serde(default = "default_in_place_translate")]
    pub in_place_translate: bool, // 是否在截图选区上直接覆盖显示译文卡片
    pub provider: String, // "google" | "baidu" | "openai" | "deepl"
    pub api_key: String, // for OpenAI, DeepL, or Baidu App ID (旧版通用兼容)
    pub secret_key: Option<String>, // for Baidu Secret Key (旧版通用兼容)
    pub base_url: Option<String>, // e.g. "https://api.openai.com/v1" (旧版通用兼容)
    pub model: Option<String>, // e.g. "gpt-4o-mini" (旧版通用兼容)
    // 多引擎独立凭据记忆（切换服务商时不互相覆盖）
    #[serde(default)]
    pub openai_api_key: Option<String>,
    #[serde(default)]
    pub openai_base_url: Option<String>,
    #[serde(default)]
    pub openai_model: Option<String>,
    #[serde(default)]
    pub deepl_api_key: Option<String>,
    #[serde(default)]
    pub baidu_app_id: Option<String>,
    #[serde(default)]
    pub baidu_secret_key: Option<String>,
    #[serde(default = "default_source_lang")]
    pub source_lang: String, // 翻译源语言: "auto" | "zh-CN" | "en" | ...
    #[serde(default = "default_target_lang")]
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
            app_language: default_app_language(),
            auto_start: default_auto_start(),
            silent_start: default_silent_start(),
            hotkey: default_hotkey(),
            action_copy_shortcut: default_action_copy_shortcut(),
            action_ocr_shortcut: default_action_ocr_shortcut(),
            action_translate_shortcut: default_action_translate_shortcut(),
            action_pin_shortcut: default_action_pin_shortcut(),
            pin_shadow: default_pin_shadow(),
            pin_opacity: default_pin_opacity(),
            ocr_engine: default_ocr_engine(),
            ocr_lang: default_ocr_lang(),
            preserve_line_breaks: default_preserve_line_breaks(),
            enhance_contrast: default_enhance_contrast(),
            in_place_translate: default_in_place_translate(),
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
            source_lang: default_source_lang(),
            target_lang: default_target_lang(),
        }
    }
}

fn get_config_path() -> PathBuf {
    if let Ok(app_data) = std::env::var("APPDATA") {
        let dir = PathBuf::from(app_data).join("SnipLingo");
        let _ = fs::create_dir_all(&dir);
        dir.join("config.json")
    } else {
        PathBuf::from("config.json")
    }
}

pub fn load_config() -> AppConfig {
    let path = get_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
            return config;
        }
    }
    AppConfig::default()
}

#[cfg(target_os = "windows")]
pub fn sync_autostart_registry(enabled: bool) {
    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy().to_string();
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if enabled {
            let _ = std::process::Command::new("reg")
                .args(&[
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
                .args(&[
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

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    let json_str = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    fs::write(&path, json_str).map_err(|e| format!("写入配置文件失败: {}", e))?;

    // 同步 Windows 注册表开机启动状态
    sync_autostart_registry(config.auto_start);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
