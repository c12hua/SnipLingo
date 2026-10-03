use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use image::RgbaImage;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size};
use xcap::Monitor;

/// 本次截图是否已经呈现过覆盖层（前端画好冻结帧后主动呈现，兜底超时也会调用，需幂等）
static CAPTURE_PRESENTED: AtomicBool = AtomicBool::new(false);

/// 本次截图热键触发的时刻（epoch 毫秒），用于分段性能日志
static CAPTURE_START_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 距本次截图热键触发已过去多少毫秒（未在截图中返回 0）
pub fn since_capture_start_ms() -> u64 {
    let t0 = CAPTURE_START_MS.load(Ordering::Relaxed);
    if t0 == 0 {
        return 0;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now.saturating_sub(t0)
}

#[derive(Clone, serde::Serialize)]
pub struct CapturePayload {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    /// 抓屏瞬间前台是否被开始菜单/搜索等系统界面占着：
    /// 是 → 前端必须先铺好冻结画面再呈现（否则系统界面会浮在遮罩上）；
    /// 否 → 前端可以立即呈现，冻结画面随后并行铺上（体感与旧版一致）。
    pub shell_in_front: bool,
}

#[cfg(target_os = "windows")]
pub fn disable_window_animations(win: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
    use windows::Win32::Foundation::HWND;
    use windows::core::BOOL;
    if let Ok(hwnd) = win.hwnd() {
        unsafe {
            let disable = BOOL::from(true);
            let _ = DwmSetWindowAttribute(
                HWND(hwnd.0),
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable as *const _ as *const _,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct SelectionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct CaptureState {
    pub original_image: RgbaImage,
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub scale_factor: f32,
}

pub type SafeCaptureState = Mutex<Option<CaptureState>>;

/// 安全无损裁剪内存中的全屏原始位图
pub fn crop_captured_image(
    original: &RgbaImage,
    rect: &SelectionRect,
) -> Result<RgbaImage, String> {
    let orig_w = original.width();
    let orig_h = original.height();

    if rect.width == 0 || rect.height == 0 {
        return Err("选区宽高必须大于 0".to_string());
    }

    if rect.x >= orig_w || rect.y >= orig_h {
        return Err("选区起始坐标超出屏幕范围".to_string());
    }

    // 边界安全裁切约束
    let x = rect.x;
    let y = rect.y;
    let width = rect.width.min(orig_w - x);
    let height = rect.height.min(orig_h - y);

    if width == 0 || height == 0 {
        return Err("裁剪有效区域为空".to_string());
    }

    let cropped = image::imageops::crop_imm(original, x, y, width, height).to_image();
    Ok(cropped)
}

#[cfg(target_os = "windows")]
fn get_current_cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
        Some((pt.x, pt.y))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn get_current_cursor_pos() -> Option<(i32, i32)> {
    None
}

/// DXGI Desktop Duplication 抓取指定显示器（SDR、横屏时可用），失败或不受支持返回 None。
/// 实测本机 GDI BitBlt 全屏 ~200ms，DXGI ~10ms，是热键→遮罩延迟的大头。
/// ponytail: 每次触发重建 Duplication 会话（毫秒级）以规避监视器热插拔/分辨率变化导致的会话失效；
/// HDR(FP16)/竖屏旋转/多 GPU 副屏/安全桌面等场景直接回退 xcap 的 GDI 路径。
#[cfg(target_os = "windows")]
pub fn capture_monitor_dxgi(x: i32, y: i32, width: u32, height: u32) -> Option<RgbaImage> {
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11Texture2D,
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ,
        D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIResource,
        DXGI_OUTDUPL_FRAME_INFO,
    };
    use windows::core::Interface;

    // D3D 设备为自由线程 COM 对象，进程内缓存可省去每次热键 ~5ms 的设备创建
    struct SendDevice(ID3D11Device);
    unsafe impl Send for SendDevice {}
    static D3D_DEVICE: OnceLock<Option<SendDevice>> = OnceLock::new();

    unsafe {
        // 1. 枚举显卡输出，按桌面矩形定位目标显示器
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut target = None;
        for a in 0..8u32 {
            let Ok(adapter) = factory.EnumAdapters1(a) else { break };
            for o in 0..8u32 {
                let Ok(output) = adapter.EnumOutputs(o) else { break };
                let Ok(desc) = output.GetDesc() else { continue };
                if desc.DesktopCoordinates.left == x
                    && desc.DesktopCoordinates.top == y
                    && (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) == width as i32
                    && (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) == height as i32
                {
                    target = Some((output, desc.Rotation));
                    break;
                }
            }
            if target.is_some() {
                break;
            }
        }
        let (output, rotation) = target?;
        if !matches!(rotation, DXGI_MODE_ROTATION_IDENTITY | DXGI_MODE_ROTATION_UNSPECIFIED) {
            return None;
        }

        // 2. 设备（缓存，失败结果同样缓存避免反复重试）+ 复制会话
        let device = D3D_DEVICE
            .get_or_init(|| {
                let mut device = None;
                let _ = D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    None,
                );
                device.map(SendDevice)
            })
            .as_ref()?;
        let dup = output.cast::<IDXGIOutput1>().ok()?.DuplicateOutput(&device.0).ok()?;

        // 3. 取当前合成帧。已知驱动怪癖（本机 100% 复现）：复制会话建立后若桌面
        //    没有发生过新的 DWM 合成，首帧 AcquireNextFrame 会立即返回但缓冲未填充
        //    （全零，与文档"首帧即完整桌面"相悖）。因此对每帧做内容采样校验，
        //    空帧释放后等待下一次真实合成重试（DWM 任意动画/时钟刷新都会触发），
        //    最多 5 次（约 1.2s），仍为空则返回 None 回退 GDI。
        //    ponytail: 静止桌面极端情况下会白白等满 1.2s 再走 GDI（~200ms），
        //    升级路径是常驻复制会话并在后台持续攒帧。
        let context = device.0.GetImmediateContext().ok()?;
        let stride = width as usize * 4;
        let mut rows: Option<Vec<u8>> = None;
        let timeouts_ms = [30u32, 300, 300, 300, 300];
        for (i, &timeout_ms) in timeouts_ms.iter().enumerate() {
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            dup.AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource).ok()?;
            let Some(texture) = resource
                .as_ref()
                .and_then(|r| r.cast::<ID3D11Texture2D>().ok())
            else {
                let _ = dup.ReleaseFrame();
                continue;
            };

            let mut tex_desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut tex_desc);
            if tex_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                || tex_desc.Width != width
                || tex_desc.Height != height
            {
                let _ = dup.ReleaseFrame();
                return None;
            }

            // 经 staging 纹理读回 CPU
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: Default::default(),
                ..tex_desc
            };
            let mut staging = None;
            device.0.CreateTexture2D(&staging_desc, None, Some(&mut staging)).ok()?;
            let staging = staging?;
            context.CopyResource(&staging, &texture);
            let _ = dup.ReleaseFrame();

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).ok()?;
            let data = std::slice::from_raw_parts(
                mapped.pData as *const u8,
                mapped.RowPitch as usize * height as usize,
            );
            // 内容采样：真实桌面 BGRA 帧（alpha 恒为 255）不可能全零
            let has_content = data.iter().step_by(4096).any(|&b| b != 0);
            if has_content {
                let mut buffer = Vec::with_capacity(stride * height as usize);
                for row in 0..height as usize {
                    let base = row * mapped.RowPitch as usize;
                    buffer.extend_from_slice(&data[base..base + stride]);
                }
                rows = Some(buffer);
            }
            context.Unmap(&staging, 0);
            if rows.is_some() {
                if i > 0 {
                    log::info!("DXGI 首帧为空缓冲，重试 {} 次后取到有效帧", i);
                }
                break;
            }
            // 本机空首帧 100% 复现：主动请求整屏重绘，强制 DWM 立刻合成出新帧，
            // 否则重试只能干等桌面上下一次自然变化（时钟/动画，最长数百毫秒）
            if i + 1 < timeouts_ms.len() {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(None, None, false);
            }
        }
        let Some(mut buffer) = rows else {
            return None;
        };

        // 4. BGRA → RGBA 原地交换
        for px in buffer.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        RgbaImage::from_raw(width, height, buffer)
    }
}

/// 截取目标显示器，并将底图推送给 capture 窗口
pub fn trigger_capture(app: &AppHandle) -> Result<(), String> {
    let t_start = std::time::Instant::now();
    CAPTURE_PRESENTED.store(false, Ordering::SeqCst);
    CAPTURE_START_MS.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        Ordering::Relaxed,
    );

    // 异步预热翻译网络连接 (趁用户划选选区的 1~3 秒人工间隙提前完成 DNS 与 TLS 握手)
    let config = crate::core::config::load_config();
    crate::core::translate::prewarm_configured_engine(&config);

    // 抓屏之前先看一眼前台：开始菜单/搜索是否正占着前台。
    // 这决定前端要不要为"冻结画面铺好"多等一次传输（系统界面在时必须要等，否则它会浮在遮罩上）。
    #[cfg(target_os = "windows")]
    let shell_in_front = unsafe {
        is_system_shell_surface(windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow())
    };
    #[cfg(not(target_os = "windows"))]
    let shell_in_front = false;

    let monitors = Monitor::all().map_err(|e| format!("获取显示器失败: {}", e))?;
    if monitors.is_empty() {
        return Err("未检测到有效显示器".to_string());
    }

    // 智能多显示器定位：优先匹配光标当前所在显示器，否则降级为主显示器/首个显示器
    let cursor_pos = get_current_cursor_pos();
    let target_monitor = if let Some((cx, cy)) = cursor_pos {
        monitors.iter().find(|m| {
            if let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) = (m.x(), m.y(), m.width(), m.height()) {
                cx >= mx && cx < mx + (mw as i32) && cy >= my && cy < my + (mh as i32)
            } else {
                false
            }
        })
    } else {
        None
    }
    .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
    .or_else(|| monitors.first())
    .ok_or_else(|| "无法获取目标显示器".to_string())?;

    let mon_x = target_monitor.x().map_err(|e| e.to_string())?;
    let mon_y = target_monitor.y().map_err(|e| e.to_string())?;
    let mon_w = target_monitor.width().map_err(|e| e.to_string())?;
    let mon_h = target_monitor.height().map_err(|e| e.to_string())?;
    let scale_factor = target_monitor.scale_factor().map_err(|e| e.to_string())?;

    // 1. 静默抓取全屏图像（此时 capture-window 仍处于隐藏状态，彻底避免包含自身浮层）
    //    优先 DXGI Desktop Duplication，不支持/失败时回退 xcap 的 GDI 路径
    let t_cap_start = std::time::Instant::now();
    #[cfg(target_os = "windows")]
    let (rgba_img, engine) = match capture_monitor_dxgi(mon_x, mon_y, mon_w, mon_h) {
        Some(img) => (img, "DXGI"),
        None => (
            target_monitor
                .capture_image()
                .map_err(|e| format!("截取屏幕图像失败: {}", e))?,
            "GDI回退",
        ),
    };
    #[cfg(not(target_os = "windows"))]
    let (rgba_img, engine) = (
        target_monitor
            .capture_image()
            .map_err(|e| format!("截取屏幕图像失败: {}", e))?,
        "GDI",
    );
    let t_cap_duration = t_cap_start.elapsed();
    log::info!(
        "[计时] 底层截屏完成({}): 耗时 {:?} (自热键 {}ms, 画面 {}x{})",
        engine,
        t_cap_duration,
        since_capture_start_ms(),
        rgba_img.width(),
        rgba_img.height()
    );

    // 2. 将无损原始位图直接暂存在 AppState 中，彻底消除 CPU 耗时的 JPEG 压缩和 Base64 编码，实现急速响应
    if let Some(state) = app.try_state::<SafeCaptureState>() {
        let mut lock = state.lock().map_err(|_| "锁获取失败".to_string())?;
        *lock = Some(CaptureState {
            original_image: rgba_img,
            monitor_x: mon_x,
            monitor_y: mon_y,
            scale_factor,
        });
    }

    // 3. 定位并展示 capture 窗口（禁用 DWM 缩放跃动动画）
    if let Some(capture_win) = app.get_webview_window("capture") {
        #[cfg(target_os = "windows")]
        disable_window_animations(&capture_win);

        let _ = capture_win.set_position(Position::Physical(PhysicalPosition::new(mon_x, mon_y)));
        let _ = capture_win.set_size(Size::Physical(PhysicalSize::new(mon_w, mon_h)));

        // 发送屏幕几何参数给前端：前端据此把冻结画面铺满窗口，并初始化选区
        let payload = CapturePayload {
            width: mon_w,
            height: mon_h,
            scale_factor,
            shell_in_front,
        };
        let _ = capture_win.emit("screenshot-captured", payload);

        // 关键：此刻【不】显示窗口。等前端把冻结画面画好后再呈现（见 present_capture_window），
        // 否则半透明遮罩会透出活的桌面——开始菜单 / 搜索等系统界面会"浮"在遮罩之上。
        // 这里保留一个兜底超时，防止前端异常时覆盖层永不出现。
        let fallback = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let _ = present_capture_window(&fallback, shell_in_front);
        });

        log::debug!(
            "[阶段0性能基线] 截图热键触发至 capture 窗口显示: {:?} (底层截屏耗时: {:?}, 分辨率: {}x{})",
            t_start.elapsed(),
            t_cap_duration,
            mon_w,
            mon_h
        );

        Ok(())
    } else {
        Err("未找到 capture 窗口".to_string())
    }
}

/// 呈现截图覆盖层（幂等）：显示 → 置顶 → 夺回前台 → 等系统界面退场。
/// shell_in_front=true 表示抓屏瞬间前台被开始菜单/搜索等系统界面占着：
/// 会注入 ESC 请其退场，并【轮询确认真正退场后】才返回——前端据此再让画面淡入，
/// 消除"遮罩先出现、系统界面还浮着"的中间态（普通窗口永远盖不住它们，Z-band 所限）。
/// 由前端在冻结画面绘制完成后调用；若前端异常，trigger_capture 的兜底超时也会调用。
pub fn present_capture_window(app: &AppHandle, shell_in_front: bool) -> bool {
    if CAPTURE_PRESENTED.swap(true, Ordering::SeqCst) {
        return false;
    }
    let Some(win) = app.get_webview_window("capture") else {
        return false;
    };

    log::info!("[计时] 前端请求呈现覆盖层: 自热键 {}ms", since_capture_start_ms());

    #[cfg(target_os = "windows")]
    disable_window_animations(&win);

    // 先让窗口可见（此时前端容器仍是 opacity:0，用户看不到任何东西），
    // 隐藏窗口无法被激活，必须先 show 才能抢前台。
    let _ = win.show();
    let _ = win.set_focus();

    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = win.hwnd() {
        let raw = hwnd.0 as isize;
        // 只能在【拥有该窗口的主线程】上抢前台：从 IPC 线程调用时，
        // 即使 AttachThreadInput 也会被前台锁拒绝（实测 SetForegroundWindow 返回 false）
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        if app
            .run_on_main_thread(move || unsafe {
                let _ = force_overlay_foreground(raw);
                let shell_now = shell_in_front
                    || is_system_shell_surface(windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow());
                // 仅当开始菜单/搜索仍占着前台时才请它退场。
                // 绝不能在自己的窗口拿到焦点后注入 ESC——overlay 前端会把 ESC 当作退出指令。
                if is_system_shell_surface(windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()) {
                    tap_key(0x1B);
                }
                let _ = tx.send(shell_now);
            })
            .is_ok()
        {
            let shell_now = rx
                .recv_timeout(std::time::Duration::from_millis(150))
                .unwrap_or(shell_in_front);
            if shell_now {
                // 事件确认替代固定等待：轮询直到系统界面真正退场（上限 600ms 兜底，
                // ESC 失效时按旧行为继续呈现，只是画面会短暂被系统界面压住）
                let t_wait = std::time::Instant::now();
                let deadline = t_wait + std::time::Duration::from_millis(600);
                let mut shell_remaining;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    shell_remaining = unsafe {
                        is_system_shell_surface(
                            windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
                        )
                    };
                    if !shell_remaining || std::time::Instant::now() >= deadline {
                        break;
                    }
                }
                log::info!(
                    "系统界面退场确认: 耗时 {}ms{}",
                    t_wait.elapsed().as_millis(),
                    if shell_remaining {
                        " (超时未退场，按旧有兜底继续)"
                    } else {
                        ""
                    }
                );
                // 菜单退场后焦点可能落回原应用：收尾再夺回一次前台
                let _ = app.run_on_main_thread(move || {
                    let _ = force_overlay_foreground(raw);
                });
            }
            return shell_now;
        }
    }

    false
}

/// Windows：在主线程上把覆盖层抬到最前并夺回前台（不含 ESC/退场确认，见 present_capture_window）。
/// 前台锁（Foreground Lock）会拒绝后台进程直接抢焦点，因此按下面顺序逐级尝试：
/// 1. SetWindowPos 置顶
/// 2. 附加到当前前台线程 + SetForegroundWindow + SetActiveWindow
/// 3. 注入一次 Alt 轻敲解除前台锁后重试
/// 4. SwitchToThisWindow（Alt+Tab 内部用的就是它）
#[cfg(target_os = "windows")]
fn force_overlay_foreground(raw: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, SetWindowPos, SwitchToThisWindow, HWND_TOPMOST, SWP_NOMOVE,
        SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    let h = HWND(raw as *mut core::ffi::c_void);

    unsafe {
        let _ = SetWindowPos(
            h,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );

        let mut ours = attempt_set_foreground(h);
        if !ours {
            tap_key(0x12); // VK_MENU：轻敲 Alt，让系统认可本进程刚产生过输入
            ours = attempt_set_foreground(h);
        }
        if !ours {
            SwitchToThisWindow(h, true);
            ours = GetForegroundWindow() == h;
        }

        log::info!(
            "截图覆盖层呈现完成: 夺回前台={} 前台是否已是覆盖层={}",
            ours,
            GetForegroundWindow() == h
        );

        ours
    }
}

/// 附加到当前前台线程后尝试抢前台（这是绕过前台锁的标准做法）
#[cfg(target_os = "windows")]
unsafe fn attempt_set_foreground(h: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetActiveWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
    let cur_thread = GetCurrentThreadId();
    let attached = fg_thread != 0
        && fg_thread != cur_thread
        && AttachThreadInput(cur_thread, fg_thread, true).as_bool();

    let _ = BringWindowToTop(h);
    let ok = SetForegroundWindow(h).as_bool();
    if ok {
        let _ = SetActiveWindow(h);
    }
    if attached {
        let _ = AttachThreadInput(cur_thread, fg_thread, false);
    }
    ok || GetForegroundWindow() == h
}

/// 注入一次按键轻敲（只按一下、立刻抬起）
#[cfg(target_os = "windows")]
unsafe fn tap_key(vk: u8) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    };
    keybd_event(vk, 0, KEYBD_EVENT_FLAGS(0), 0);
    keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
}

/// 判断窗口是否属于 Windows 的沉浸式系统界面（开始菜单 / 搜索 / 操作中心等）
#[cfg(target_os = "windows")]
fn is_system_shell_surface(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    if hwnd.0.is_null() {
        return false;
    }
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return false;
    }
    let class = String::from_utf16_lossy(&buf[..len as usize]);
    class.contains("Windows.UI.Core.CoreWindow") || class.contains("XamlExplorerHostIslandWindow")
}

/// 释放全屏底图大内存，保持应用常驻运行时的极低内存占用
pub fn clear_capture_state(app: &AppHandle) {
    if let Some(state) = app.try_state::<SafeCaptureState>() {
        if let Ok(mut lock) = state.lock() {
            *lock = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_crop_captured_image_valid() {
        let mut img = RgbaImage::new(100, 100);
        img.put_pixel(10, 10, Rgba([255, 0, 0, 255]));

        let rect = SelectionRect {
            x: 10,
            y: 10,
            width: 20,
            height: 30,
        };

        let cropped = crop_captured_image(&img, &rect).expect("裁剪应该成功");
        assert_eq!(cropped.width(), 20);
        assert_eq!(cropped.height(), 30);
        assert_eq!(*cropped.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn test_crop_captured_image_clamped_boundary() {
        let img = RgbaImage::new(100, 100);
        let rect = SelectionRect {
            x: 80,
            y: 80,
            width: 50,
            height: 50,
        };

        let cropped = crop_captured_image(&img, &rect).expect("应自动约束至图像边界");
        assert_eq!(cropped.width(), 20); // 100 - 80 = 20
        assert_eq!(cropped.height(), 20); // 100 - 80 = 20
    }

    #[test]
    fn test_crop_captured_image_zero_size() {
        let img = RgbaImage::new(100, 100);
        let rect = SelectionRect {
            x: 10,
            y: 10,
            width: 0,
            height: 10,
        };
        assert!(crop_captured_image(&img, &rect).is_err());
    }

    #[test]
    fn test_crop_captured_image_out_of_bounds() {
        let img = RgbaImage::new(100, 100);
        let rect = SelectionRect {
            x: 200,
            y: 200,
            width: 10,
            height: 10,
        };
        assert!(crop_captured_image(&img, &rect).is_err());
    }

    /// 真实桌面冒烟测试：DXGI 路径应能抓到与 GDI 同尺寸的完整帧（需交互桌面会话）。
    /// 测试进程默认非 DPI-aware，桌面坐标会被缩放虚拟化（125% 下 1920→1536），
    /// 而 Tauri 应用进程由 tao 声明 PerMonitorV2 无此问题——测试里需显式对齐。
    #[cfg(target_os = "windows")]
    #[test]
    fn dxgi_capture_smoke() {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }

        let monitor = xcap::Monitor::all()
            .expect("枚举显示器失败")
            .into_iter()
            .next()
            .expect("无可用显示器");
        let (x, y, w, h) = (
            monitor.x().unwrap(),
            monitor.y().unwrap(),
            monitor.width().unwrap(),
            monitor.height().unwrap(),
        );

        let t = std::time::Instant::now();
        let img = capture_monitor_dxgi(x, y, w, h).expect("DXGI 抓屏应成功");
        println!("DXGI 抓屏耗时: {:?} ({}x{})", t.elapsed(), img.width(), img.height());
        assert_eq!(img.width(), w);
        assert_eq!(img.height(), h);
        // 空缓冲怪癖的回归检查：真实桌面帧不可能全零
        assert!(
            img.as_raw().iter().any(|&b| b != 0),
            "DXGI 帧内容为空（空缓冲未兜底）"
        );

        // 热缓存后第二次应明显更快（设备与会话路径均已就绪）
        let t2 = std::time::Instant::now();
        assert!(capture_monitor_dxgi(x, y, w, h).is_some());
        println!("DXGI 热抓耗时: {:?}", t2.elapsed());
    }
}
