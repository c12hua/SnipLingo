use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub fn build_tray_menu(app: &AppHandle, hotkey_str: &str) -> Result<Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let hotkey_display = if hotkey_str.trim().is_empty() {
        "F4"
    } else {
        hotkey_str.trim()
    };
    let capture_title = format!("开始截图 ({})", hotkey_display);
    let capture_item = MenuItem::with_id(app, "capture", &capture_title, true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let about_title = format!("关于 SnipLingo (v{})", env!("CARGO_PKG_VERSION"));
    let about_item = MenuItem::with_id(app, "about", &about_title, true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 SnipLingo", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&capture_item, &show_item, &sep1, &about_item, &sep2, &quit_item])?;
    Ok(menu)
}

pub fn update_tray_hotkey(app: &AppHandle, new_hotkey: &str) -> Result<(), String> {
    let hotkey_display = if new_hotkey.trim().is_empty() {
        "F4"
    } else {
        new_hotkey.trim()
    };

    if let Some(tray) = app.tray_by_id("main_tray") {
        let menu = build_tray_menu(app, hotkey_display).map_err(|e| e.to_string())?;
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
        let tooltip = format!("SnipLingo - 桌面截图翻译助手 ({})", hotkey_display);
        let _ = tray.set_tooltip(Some(tooltip));
        log::info!("系统托盘菜单与提示已热更新，当前截图快捷键: [{}]", hotkey_display);
    }

    Ok(())
}

pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = crate::core::config::load_config();
    let hotkey_str = if config.hotkey.trim().is_empty() {
        "F4".to_string()
    } else {
        config.hotkey.clone()
    };
    let menu = build_tray_menu(app, &hotkey_str)?;
    let tooltip = format!("SnipLingo - 桌面截图翻译助手 ({})", hotkey_str);

    let icon = match app.default_window_icon() {
        Some(icon) => icon.clone(),
        None => {
            let decoded = image::load_from_memory(include_bytes!("../../icons/32x32.png"))
                .map_err(|e| format!("加载内置图标失败: {}", e))?
                .to_rgba8();
            let (w, h) = (decoded.width(), decoded.height());
            tauri::image::Image::new_owned(decoded.into_raw(), w, h)
        }
    };

    let _tray = TrayIconBuilder::with_id("main_tray")
        .icon(icon)
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "capture" => {
                    if let Err(e) = crate::core::capture::trigger_capture(app) {
                        log::error!("截图唤起失败: {}", e);
                    }
                }
                "show" | "about" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    log::info!("系统托盘初始化成功，当前截图快捷键: [{}]", hotkey_str);

    Ok(())
}
