use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const DEFAULT_SHORTCUT: &str = "F4";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedHotkey {
    pub vk_code: u32,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// 解析常见快捷键字符串为标准虚拟键码与修饰键掩码
pub fn parse_shortcut(s: &str) -> Option<ParsedHotkey> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut win = false;
    let mut vk_code = None;

    for part in s.split('+') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "menu" => alt = true,
            "shift" => shift = true,
            "super" | "win" | "cmd" | "meta" => win = true,
            _ => {
                if let Some(vk) = parse_key_name(&lower) {
                    vk_code = Some(vk);
                } else if trimmed.len() == 1 {
                    let ch = trimmed.chars().next().unwrap();
                    if ch.is_ascii_alphabetic() {
                        vk_code = Some(ch.to_ascii_uppercase() as u32);
                    } else if ch.is_ascii_digit() {
                        vk_code = Some(ch as u32);
                    }
                }
            }
        }
    }

    vk_code.map(|vk| ParsedHotkey {
        vk_code: vk,
        ctrl,
        alt,
        shift,
        win,
    })
}

fn parse_key_name(name: &str) -> Option<u32> {
    // F1 - F24
    if let Some(f_num) = name.strip_prefix('f') {
        if let Ok(num) = f_num.parse::<u32>() {
            if (1..=12).contains(&num) {
                return Some(0x70 + (num - 1)); // 0x70 = VK_F1
            } else if (13..=24).contains(&num) {
                return Some(0x7C + (num - 13)); // 0x7C = VK_F13
            }
        }
    }

    match name {
        "space" => Some(0x20),
        "enter" | "return" => Some(0x0D),
        "tab" => Some(0x09),
        "backspace" | "back" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        "insert" | "ins" => Some(0x2D),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" | "pgup" => Some(0x21),
        "pagedown" | "pgdn" => Some(0x22),
        "escape" | "esc" => Some(0x1B),
        "printscreen" | "snapshot" | "prtsc" | "prtscr" => Some(0x2C),
        "`" | "~" => Some(0xC0),
        "-" | "_" => Some(0xBD),
        "=" | "+" => Some(0xBB),
        "[" | "{" => Some(0xDB),
        "]" | "}" => Some(0xDD),
        "\\" | "|" => Some(0xDC),
        ";" | ":" => Some(0xBA),
        "'" | "\"" => Some(0xDE),
        "," | "<" => Some(0xBC),
        "." | ">" => Some(0xBE),
        "/" | "?" => Some(0xBF),
        _ => None,
    }
}

static LAST_TRIGGER_MS: AtomicU64 = AtomicU64::new(0);

/// 核心热键触发统一入口（集成 350ms 防抖与截图层防死锁切换）
pub fn handle_hotkey_trigger(app: &AppHandle) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let prev = LAST_TRIGGER_MS.load(Ordering::Relaxed);
    if now.saturating_sub(prev) < 350 {
        return; // 防抖，忽略 350ms 内的重复事件
    }
    LAST_TRIGGER_MS.store(now, Ordering::Relaxed);

    // 防死锁机制：若当前全屏截图层已处于展示状态，再次触发热键时直接退出截图并隐藏
    if let Some(win) = app.get_webview_window("capture") {
        if win.is_visible().unwrap_or(false) {
            #[cfg(target_os = "windows")]
            crate::core::capture::disable_window_animations(&win);
            let _ = win.hide();
            crate::core::capture::clear_capture_state(app);
            return;
        }
    }

    if let Err(e) = crate::core::capture::trigger_capture(app) {
        log::error!("截图唤起失败: {}", e);
    }
}

#[cfg(target_os = "windows")]
mod windows_hook {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::RwLock;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetTimer,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_USER,
    };

    const WM_REHOOK: u32 = WM_USER + 101;
    const TIMER_ID_REFRESH: usize = 1;
    const REFRESH_INTERVAL_MS: u32 = 45_000; // 每 45 秒自检并刷新钩子至顶层

    static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    static CURRENT_TARGET: RwLock<Option<ParsedHotkey>> = RwLock::new(None);
    static APP_HANDLE_SLOT: OnceLock<AppHandle> = OnceLock::new();

    #[inline]
    fn is_vk_pressed(vk: i32) -> bool {
        (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let msg = wparam.0 as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if is_down || is_up {
            let kbd = *(lparam.0 as *const KBDLLHOOKSTRUCT);
            let target_opt = CURRENT_TARGET.read().ok().and_then(|g| *g);

            if let Some(target) = target_opt {
                if kbd.vkCode == target.vk_code {
                    let ctrl_down = is_vk_pressed(VK_CONTROL.0 as i32);
                    let alt_down = is_vk_pressed(VK_MENU.0 as i32);
                    let shift_down = is_vk_pressed(VK_SHIFT.0 as i32);
                    let win_down =
                        is_vk_pressed(VK_LWIN.0 as i32) || is_vk_pressed(VK_RWIN.0 as i32);

                    if ctrl_down == target.ctrl
                        && alt_down == target.alt
                        && shift_down == target.shift
                        && win_down == target.win
                    {
                        if is_down {
                            if let Some(app) = APP_HANDLE_SLOT.get() {
                                let app_clone = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    handle_hotkey_trigger(&app_clone);
                                });
                            }
                        }

                        // 吞噬匹配到的按键事件（返回 1），禁止向系统后续 Hook 及前景窗口转发！
                        // 确保即使前台运行的是 Excel、资源管理器、Chrome、游戏等，也无法覆盖或截胡该按键！
                        return LRESULT(1);
                    }
                }
            }
        }

        CallNextHookEx(None, code, wparam, lparam)
    }

    pub fn init_hook_engine(app: &AppHandle, initial_hotkey: Option<ParsedHotkey>) {
        let _ = APP_HANDLE_SLOT.set(app.clone());
        if let Ok(mut lock) = CURRENT_TARGET.write() {
            *lock = initial_hotkey;
        }

        std::thread::Builder::new()
            .name("sniplingo-high-prio-hotkey".to_string())
            .spawn(move || unsafe {
                let tid = GetCurrentThreadId();
                HOOK_THREAD_ID.store(tid, Ordering::SeqCst);

                let mut hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(low_level_keyboard_proc),
                    None,
                    0,
                )
                .unwrap_or_default();

                if hook != HHOOK::default() {
                    log::info!("Windows 底层高优先级键盘钩子 (WH_KEYBOARD_LL) 启动成功");
                } else {
                    log::error!("Windows 底层键盘钩子安装失败");
                }

                // 启动周期性自检与保活定时器
                let _ = SetTimer(None, TIMER_ID_REFRESH, REFRESH_INTERVAL_MS, None);

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_REHOOK || msg.message == WM_TIMER {
                        // 重新挂接以确保当前钩子始终保持在系统钩子链最顶端（最高优先级）
                        if hook != HHOOK::default() {
                            let _ = UnhookWindowsHookEx(hook);
                        }
                        hook = SetWindowsHookExW(
                            WH_KEYBOARD_LL,
                            Some(low_level_keyboard_proc),
                            None,
                            0,
                        )
                        .unwrap_or_default();
                    }

                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }

                if hook != HHOOK::default() {
                    let _ = UnhookWindowsHookEx(hook);
                }
            })
            .expect("创建高优先级热键钩子线程失败");
    }

    pub fn set_target_hotkey(hotkey: Option<ParsedHotkey>) {
        if let Ok(mut lock) = CURRENT_TARGET.write() {
            *lock = hotkey;
        }

        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_REHOOK, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 动态更新全局截图快捷键 (以操作系统最高优先级模式运行)
pub fn update_capture_shortcut(app: &AppHandle, new_shortcut_str: &str) -> Result<(), String> {
    let parsed = parse_shortcut(new_shortcut_str)
        .ok_or_else(|| format!("快捷键格式无效 [{}]", new_shortcut_str))?;

    #[cfg(target_os = "windows")]
    {
        windows_hook::set_target_hotkey(Some(parsed));
    }

    // 双重保底机制：同时尝试向 tauri-plugin-global-shortcut 注册备用通道
    if let Ok(shortcut) = Shortcut::from_str(new_shortcut_str) {
        let global_shortcut = app.global_shortcut();
        let _ = global_shortcut.unregister_all();

        let app_handle = app.clone();
        if let Err(e) = global_shortcut.on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                handle_hotkey_trigger(&app_handle);
            }
        }) {
            log::warn!("标准 RegisterHotKey 登记警告 (底层钩子将独占最高优先级拦截): {}", e);
        }
    }

    log::info!("全局截图快捷键已更新为最高优先级模式 [{}]", new_shortcut_str);
    Ok(())
}

/// 应用启动时根据配置文件注册初始快捷键
pub fn register_capture_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = crate::core::config::load_config();
    let shortcut_str = if config.hotkey.trim().is_empty() {
        DEFAULT_SHORTCUT
    } else {
        &config.hotkey
    };

    let parsed = parse_shortcut(shortcut_str);

    #[cfg(target_os = "windows")]
    {
        windows_hook::init_hook_engine(app, parsed);
    }

    let _ = update_capture_shortcut(app, shortcut_str);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_shortcuts_parsing() {
        assert!(Shortcut::from_str("F4").is_ok());
        assert!(Shortcut::from_str("F1").is_ok());
        assert!(Shortcut::from_str("Alt+A").is_ok());
        assert!(Shortcut::from_str("Ctrl+Shift+S").is_ok());
        assert!(Shortcut::from_str("Control+Alt+Z").is_ok());
    }

    #[test]
    fn test_invalid_shortcut_parsing() {
        assert!(Shortcut::from_str("InvalidKey123").is_err());
        assert!(Shortcut::from_str("Ctrl+").is_err());
    }

    #[test]
    fn test_parse_shortcut_custom() {
        let f4 = parse_shortcut("F4").expect("F4 should parse");
        assert_eq!(f4.vk_code, 0x73);
        assert!(!f4.ctrl && !f4.alt && !f4.shift && !f4.win);

        let ca = parse_shortcut("Ctrl+Alt+A").expect("Ctrl+Alt+A should parse");
        assert_eq!(ca.vk_code, 0x41);
        assert!(ca.ctrl && ca.alt && !ca.shift && !ca.win);

        let css = parse_shortcut("Ctrl+Shift+S").expect("Ctrl+Shift+S should parse");
        assert_eq!(css.vk_code, 0x53);
        assert!(css.ctrl && !css.alt && css.shift && !css.win);

        assert!(parse_shortcut("").is_none());
        assert!(parse_shortcut("Ctrl+").is_none());
    }
}
