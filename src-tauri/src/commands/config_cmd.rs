use crate::core::config::{load_config, save_config, AppConfig};
use tauri::AppHandle;

#[tauri::command]
pub fn get_config() -> AppConfig {
    load_config()
}

#[tauri::command]
pub fn save_config_cmd(app: AppHandle, config: AppConfig) -> Result<(), String> {
    let old_config = load_config();
    if old_config.hotkey != config.hotkey {
        crate::core::hotkey::update_capture_shortcut(&app, &config.hotkey)?;
        let _ = crate::core::tray::update_tray_hotkey(&app, &config.hotkey);
    }
    save_config(&config)
}

#[tauri::command]
pub async fn test_api_connection(config: AppConfig) -> Result<String, String> {
    let test_word = "Hello";
    let start = std::time::Instant::now();
    let res = crate::core::translate::execute_translation_bypass_cache(&config, test_word)
        .await
        .map_err(|e| format!("连接测试失败: {}", e))?;
    let duration = start.elapsed().as_millis();
    Ok(format!("连通测试成功！响应耗时: {}ms，测试译文: 「{}」", duration, res.trim()))
}
