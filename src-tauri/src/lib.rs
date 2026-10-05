use tauri::{Emitter, Manager, WindowEvent};

pub mod commands;
pub mod core;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(core::capture::SafeCaptureState::new(None))
        .manage(core::pin::SafePinStorage::default())
        .invoke_handler(tauri::generate_handler![
            commands::capture_cmd::close_capture,
            commands::capture_cmd::cancel_capture,
            commands::capture_cmd::get_capture_frame,
            commands::capture_cmd::show_capture_overlay,
            commands::clipboard_cmd::copy_selection_to_clipboard,
            commands::clipboard_cmd::copy_translated_image_cmd,
            commands::ocr_cmd::extract_text_from_selection,
            commands::qrcode_cmd::recognize_qrcode,
            commands::config_cmd::get_config,
            commands::config_cmd::save_config_cmd,
            commands::config_cmd::test_api_connection,
            commands::translate_cmd::translate_selection,
            commands::translate_cmd::translate_in_place,
            commands::translate_cmd::retry_translate,
            commands::translate_cmd::show_result_window,
            commands::translate_cmd::close_result_window,
            commands::translate_cmd::open_settings_window,
            commands::pin_cmd::pin_screenshot,
            commands::pin_cmd::get_pin_data,
            commands::pin_cmd::destroy_pin_window,
            commands::pin_cmd::copy_pin_image_cmd,
            commands::pin_cmd::translate_pin_cmd,
            commands::pin_cmd::show_pin_context_menu,
            commands::save_cmd::save_selection_to_file,
            commands::save_cmd::save_data_url_to_file,
            commands::save_cmd::save_pin_image_cmd,
        ])
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if let Some(label) = id.strip_prefix("pin_trans:") {
                if let Err(e) = crate::core::pin::translate_pin(app, label) {
                    let _ = app.emit_to(label, "pin-toast", e);
                }
            } else if let Some(label) = id.strip_prefix("pin_copy:") {
                if let Err(e) = crate::core::pin::copy_pin_to_clipboard(app, label) {
                    let _ = app.emit_to(label, "pin-toast", e);
                }
            } else if let Some(label) = id.strip_prefix("pin_save:") {
                let app_handle = app.clone();
                let label_str = label.to_string();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::commands::save_cmd::save_pin_image_internal(&app_handle, &label_str).await {
                        let _ = app_handle.emit_to(&label_str, "pin-toast", e);
                    }
                });
            } else if let Some(label) = id.strip_prefix("pin_del:") {
                let _ = crate::core::pin::destroy_pin(app, label);
            } else if id == "pin_del_all" {
                let _ = crate::core::pin::destroy_all_pins(app);
            }
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                let log_plugin = tauri_plugin_log::Builder::default()
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                    ])
                    .level(log::LevelFilter::Info)
                    .build();

                if let Err(e) = app.handle().plugin(log_plugin) {
                    eprintln!("警告: 初始化日志插件失败 (已忽略): {}", e);
                }
            }

            // 初始化系统托盘
            core::tray::create_tray(app.handle())
                .expect("Failed to initialize system tray");

            // 注册全局截图快捷键
            if let Err(e) = core::hotkey::register_capture_shortcut(app.handle()) {
                log::error!("注册全局截图快捷键失败: {}", e);
            }

            // 预先禁用 capture、result 及全部池化 pin 窗口的 DWM 过渡缩放动画，消除弹窗阻尼感
            core::prewarm::init_window_animations(app.handle());

            // 预先定型 capture 窗口为主屏幕全屏物理分辨率，
            // 让 WebView2 在应用启动后台即完成全屏 Viewport 和 Compositor 渲染管道初始化，
            // 彻底杜绝首次使用快捷键唤醒时 800x600 (1/4 屏幕) 闪烁展开的现象
            if let Some(win) = app.get_webview_window("capture") {
                if let Ok(monitors) = xcap::Monitor::all() {
                    if let Some(primary) = monitors
                        .iter()
                        .find(|m| m.is_primary().unwrap_or(false))
                        .or_else(|| monitors.first())
                    {
                        if let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) =
                            (primary.x(), primary.y(), primary.width(), primary.height())
                        {
                            let _ = win.set_position(tauri::Position::Physical(
                                tauri::PhysicalPosition::new(mx, my),
                            ));
                            let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
                                mw, mh,
                            )));
                        }
                    }
                }
            }

            // 启动全局后台异步静默预热管道（延时 1200ms 执行：剪贴板 OLE、翻译网络长连接池、OCR 引擎与系统语言预热）
            core::prewarm::start_background_prewarm();

            // 根据配置判断是否展示主设置窗口（开启静默启动时直接常驻系统托盘，不弹出主窗口）
            let config = crate::core::config::load_config();
            if !config.silent_start {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // 点击窗口关闭按钮时隐藏窗口，保持应用常驻托盘
                match window.label() {
                    "main" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    "capture" => {
                        api.prevent_close();
                        let _ = core::capture::end_capture(window.app_handle(), core::capture::current_capture_id(), true);
                    }
                    "result" => {
                        api.prevent_close();
                        let _ = commands::translate_cmd::close_result_window(window.app_handle().clone(), commands::translate_cmd::current_result_request_id());
                    }
                    label if label == "pin" || label.starts_with("pin_") => {
                        api.prevent_close();
                        let _ = crate::core::pin::destroy_pin(window.app_handle(), label);
                    }
                    _ => {}
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
