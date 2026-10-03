use std::time::Duration;
use tauri::{AppHandle, Manager};

/// 在应用启动 setup 阶段调用：
/// 为所有预创建窗口（capture、result、各个池化 pin 窗口）禁用 DWM 缩放动画，
/// 确保任何窗口被唤醒弹出时均为 0 延迟、0 缩放闪烁的原生瞬间上屏体验
pub fn init_window_animations(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    {
        // 1. capture 截图遮罩窗口
        if let Some(win) = app.get_webview_window("capture") {
            crate::core::capture::disable_window_animations(&win);
        }

        // 2. result 翻译结果窗口
        if let Some(win) = app.get_webview_window("result") {
            crate::core::capture::disable_window_animations(&win);
        }

        // 3. 贴图池窗口 (pin, pin_1 ... pin_7)
        for &label in crate::core::pin::POOLED_PIN_LABELS {
            if let Some(win) = app.get_webview_window(label) {
                crate::core::capture::disable_window_animations(&win);
            }
        }
        log::info!("已完成 capture、result 与全部池化 pin 窗口的 DWM 动画禁用优化");
    }
}

/// 启动全局后台异步静默预热管道
/// 在托盘与主程序初始化完成 1200ms 后于独立工作线程执行，不抢占主进程启动资源
pub fn start_background_prewarm() {
    let _ = std::thread::Builder::new()
        .name("sniplingo-prewarm".to_string())
        .spawn(move || {
            // 延时 1200ms 避开启动初期的高峰，确保用户能瞬间看到托盘图标
            std::thread::sleep(Duration::from_millis(1200));
            let t_prewarm_total = std::time::Instant::now();
            log::info!("正在启动 SnipLingo 全局后台静默预热流程...");

            // 0. DXGI 抓屏路径预热：D3D 设备创建 + 首次复制会话开销（实测 ~500ms）
            //    挪到启动期，避免用户启动后第一次按快捷键时撞上冷启动延迟
            let t_dxgi = std::time::Instant::now();
            if let Ok(monitors) = xcap::Monitor::all() {
                if let Some(m) = monitors.first() {
                    if let (Ok(x), Ok(y), Ok(w), Ok(h)) =
                        (m.x(), m.y(), m.width(), m.height())
                    {
                        let _ = crate::core::capture::capture_monitor_dxgi(x, y, w, h);
                    }
                }
            }
            log::debug!(
                "[阶段0性能基线] 后台预热 0/4: DXGI 抓屏预热耗时 {:?}",
                t_dxgi.elapsed()
            );

            // 1. 系统剪贴板 OLE 运行环境预热
            let t_cb = std::time::Instant::now();
            crate::core::clipboard::warmup_clipboard();
            log::debug!("[阶段0性能基线] 后台预热 1/3: 剪贴板预热耗时 {:?}", t_cb.elapsed());

            // 2. 翻译网络长连接池预热 (DNS + TLS 1.3 握手提前建连)
            let t_net = std::time::Instant::now();
            let config = crate::core::config::load_config();
            crate::core::translate::prewarm_configured_engine(&config);
            log::debug!("[阶段0性能基线] 后台预热 2/3: 翻译网络连接预热耗时 {:?}", t_net.elapsed());

            // 3. OCR 引擎与语言字典预热 (PaddleOCR / Windows Media OCR)
            let t_ocr = std::time::Instant::now();
            crate::core::ocr::warmup_ocr_engine(&config);
            log::debug!("[阶段0性能基线] 后台预热 3/3: OCR 引擎及模型预热耗时 {:?}", t_ocr.elapsed());

            log::info!(
                "SnipLingo 全局后台静默预热全部顺利完成，总预热耗时 {:?}",
                t_prewarm_total.elapsed()
            );
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prewarm_sequence_execution() {
        // 测试剪贴板预热
        crate::core::clipboard::warmup_clipboard();

        // 测试翻译引擎预热
        let config = crate::core::config::load_config();
        crate::core::translate::prewarm_configured_engine(&config);

        // 测试 OCR 引擎预热
        crate::core::ocr::warmup_ocr_engine(&config);
    }
}
