pub mod baidu;
pub mod client;
pub mod deepl;
pub mod google;
pub mod openai;

use crate::core::config::AppConfig;
use crate::core::error::SnipLingoError;
pub use client::{
    compute_cache_key, finish_in_flight, get_cached, prewarm_provider, register_or_join_in_flight,
    set_cached, InFlightStatus,
};

pub fn prewarm_configured_engine(config: &AppConfig) {
    let (_, base_url, _) = config.get_openai_credentials();
    prewarm_provider(&config.provider, base_url.as_deref());
}

pub async fn execute_translation(
    config: &AppConfig,
    text: &str,
) -> Result<String, SnipLingoError> {
    execute_translation_internal(config, text, false).await
}

/// 连通性测试专用：完全穿透/绕过内存缓存与并发合并，确保每次测试均测量真实的实时网络往返延迟与接口可用性
pub async fn execute_translation_bypass_cache(
    config: &AppConfig,
    text: &str,
) -> Result<String, SnipLingoError> {
    execute_translation_internal(config, text, true).await
}

async fn call_provider(config: &AppConfig, trimmed: &str) -> Result<String, SnipLingoError> {
    match config.provider.to_lowercase().as_str() {
        "google" => {
            google::translate_google(trimmed, &config.source_lang, &config.target_lang).await
        }
        "baidu" => {
            let (app_id, secret_key) = config.get_baidu_credentials();
            baidu::translate_baidu(
                &app_id,
                secret_key.as_deref(),
                trimmed,
                &config.source_lang,
                &config.target_lang,
            )
            .await
        }
        "deepl" => {
            let key = config.get_deepl_credentials();
            deepl::translate_deepl(
                &key,
                trimmed,
                &config.source_lang,
                &config.target_lang,
            )
            .await
        }
        _ => {
            let (key, base_url, model) = config.get_openai_credentials();
            openai::translate_openai(
                &key,
                base_url.as_deref(),
                model.as_deref(),
                trimmed,
                &config.source_lang,
                &config.target_lang,
            )
            .await
        }
    }
}

async fn execute_translation_internal(
    config: &AppConfig,
    text: &str,
    bypass_cache: bool,
) -> Result<String, SnipLingoError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(SnipLingoError::NoTextDetected);
    }

    if bypass_cache {
        let t0 = std::time::Instant::now();
        let res = call_provider(config, trimmed).await;
        log::debug!("[性能] 连通性测试网络往返耗时: {}ms", t0.elapsed().as_millis());
        return res;
    }

    // 1. 检查已完成的 LRU 缓存
    let cache_key = compute_cache_key(
        &config.provider,
        &config.source_lang,
        &config.target_lang,
        trimmed,
    );
    if let Some(cached_result) = get_cached(&cache_key) {
        log::debug!("[性能] 命中翻译内存缓存 (0ms 响应): key={}", cache_key);
        return Ok(cached_result);
    }

    // 2. 检查或加入正在进行的并发请求 (Single-Flight 合并)
    match register_or_join_in_flight(&cache_key) {
        InFlightStatus::Wait(mut rx) => {
            log::debug!("[性能] 命中并发进行中翻译请求，复用等待 (Single-Flight): key={}", cache_key);
            match rx.recv().await {
                Ok(res) => res,
                Err(_) => Err(SnipLingoError::NetworkError("并发翻译请求已被取消或中断".into())),
            }
        }
        InFlightStatus::Leader(sender) => {
            let t0 = std::time::Instant::now();
            let result = call_provider(config, trimmed).await;
            let elapsed = t0.elapsed().as_millis();
            log::debug!(
                "[性能] 网络翻译完成 (耗时 {}ms, provider: {}): key={}",
                elapsed,
                config.provider,
                cache_key
            );

            // 清理进行中状态
            finish_in_flight(&cache_key);

            // 成功时写入 LRU 缓存
            if let Ok(ref val) = result {
                set_cached(cache_key, val.clone());
            }

            // 广播结果给所有等待的并发请求
            let _ = sender.send(result.clone());

            result
        }
    }
}

