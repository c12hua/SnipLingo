use tauri::{AppHandle, Manager};
use tauri::menu::{ContextMenu, Menu, MenuItem, PredefinedMenuItem};
use crate::core::capture::SelectionRect;
use crate::core::pin::{
    copy_pin_to_clipboard, destroy_all_pins, destroy_pin, get_pin_data_for, pin_selection_image,
    translate_pin, PinData,
};

#[tauri::command]
pub fn pin_screenshot(app: AppHandle, rect: SelectionRect) -> Result<String, String> {
    pin_selection_image(&app, &rect)
}

#[tauri::command]
pub fn get_pin_data(app: AppHandle, label: String) -> Result<PinData, String> {
    get_pin_data_for(&app, &label)
}

#[tauri::command]
pub fn destroy_pin_window(app: AppHandle, label: String) -> Result<(), String> {
    destroy_pin(&app, &label)
}

#[tauri::command]
pub fn destroy_all_pins_cmd(app: AppHandle) -> Result<(), String> {
    destroy_all_pins(&app)
}

#[tauri::command]
pub fn copy_pin_image_cmd(app: AppHandle, label: String) -> Result<(), String> {
    copy_pin_to_clipboard(&app, &label)
}

#[tauri::command]
pub fn translate_pin_cmd(app: AppHandle, label: String) -> Result<(), String> {
    translate_pin(&app, &label)
}

#[tauri::command]
pub fn show_pin_context_menu(app: AppHandle, label: String) -> Result<(), String> {
    let win = app.get_webview_window(&label).ok_or("未找到贴图窗口")?;

    let item_translate = MenuItem::with_id(&app, format!("pin_trans:{}", label), "翻译此截图", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let item_copy = MenuItem::with_id(&app, format!("pin_copy:{}", label), "复制图片 (Ctrl+C)", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let config = crate::core::config::load_config();
    let save_title = match config.app_language.as_str() {
        "en" => "Save",
        "zh-TW" => "儲存",
        _ => "保存",
    };
    let item_save = MenuItem::with_id(&app, format!("pin_save:{}", label), save_title, true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let sep1 = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let item_destroy = MenuItem::with_id(&app, format!("pin_del:{}", label), "销毁此贴图 (ESC / Del)", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let item_destroy_all = MenuItem::with_id(&app, "pin_del_all", "❌ 销毁全部贴图", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(&app, &[
        &item_translate,
        &item_copy,
        &item_save,
        &sep1,
        &item_destroy,
        &item_destroy_all,
    ]).map_err(|e| e.to_string())?;

    let window = win.as_ref().window().clone();
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}
