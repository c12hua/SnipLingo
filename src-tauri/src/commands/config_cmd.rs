use crate::core::config::{load_config, save_config, AppConfig};
use tauri::AppHandle;

#[tauri::command]
pub fn get_config() -> AppConfig {
    load_config()
}

#[tauri::command]
pub fn save_config_cmd(app: AppHandle, config: AppConfig) -> Result<(), String> {
    let old_config = load_config();
    let hotkey_changed = old_config.hotkey != config.hotkey;
    if hotkey_changed {
        crate::core::hotkey::update_capture_shortcut(&app, &config.hotkey)?;
    }
    if let Err(error) = save_config(&config) {
        if hotkey_changed {
            if let Err(e) = crate::core::hotkey::update_capture_shortcut(&app, &old_config.hotkey) {
                log::error!("恢复原截图快捷键失败: {}", e);
            }
        }
        return Err(error);
    }
    if hotkey_changed {
        let _ = crate::core::tray::update_tray_hotkey(&app, &config.hotkey);
    }
    Ok(())
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
