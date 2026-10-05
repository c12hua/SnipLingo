pub mod baidu;
pub mod client;
pub mod deepl;
pub mod google;
pub mod openai;

use crate::core::config::AppConfig;
use crate::core::error::SnipLingoError;
use client::{
    compute_cache_key, get_cached, prewarm_provider, register_or_join_in_flight,
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

// ponytail: 所有服务共享 4 个在途请求，不保证各账户 QPS；确有需求再按服务商细分。
static REQUEST_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

async fn call_provider(config: &AppConfig, trimmed: &str) -> Result<String, SnipLingoError> {
    let _permit = REQUEST_SLOTS.acquire().await.map_err(|_| {
        SnipLingoError::NetworkError("翻译请求队列已关闭".into())
    })?;
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
    // 合并行仅用于请求和缓存，识别原文与服务返回的译文均保持原有换行。
    let prepared = crate::core::ocr::text_cleaner::clean_ocr_text_with_options(text, config.preserve_line_breaks);
    let trimmed = prepared.trim();
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
    let cache_key = compute_cache_key(config, trimmed);
    if let Some(cached_result) = get_cached(&cache_key) {
        log::debug!("[性能] 命中翻译内存缓存");
        return Ok(cached_result);
    }

    // 2. 检查或加入正在进行的并发请求 (Single-Flight 合并)
    match register_or_join_in_flight(&cache_key) {
        InFlightStatus::Wait(mut rx) => {
            log::debug!("[性能] 命中并发进行中翻译请求，复用等待 (Single-Flight)");
            match rx.recv().await {
                Ok(res) => res,
                Err(_) => Err(SnipLingoError::NetworkError("并发翻译请求已被取消或中断".into())),
            }
        }
        InFlightStatus::Leader(leader) => {
            let t0 = std::time::Instant::now();
            // 首次查缓存与注册之间，上一位 Leader 可能已经完成。
            let result = match get_cached(&cache_key) {
                Some(cached) => Ok(cached),
                None => call_provider(config, trimmed).await,
            };
            log::debug!("[性能] 翻译请求完成 (耗时 {}ms)", t0.elapsed().as_millis());

            // 先发布缓存，再广播并移除在途记录，避免完成瞬间重复请求。
            if let Ok(ref val) = result {
                set_cached(cache_key, val.clone());
            }
            leader.finish(result.clone());
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::task::{Context, Waker};

    #[test]
    fn line_merging_only_changes_translation_input() {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let original = "First line.\nSecond line.\n\nAnother paragraph.";
        let translated = "第一行译文。\n第二行译文。\n\n另一个段落。";
        for preserve_line_breaks in [false, true] {
            let config = AppConfig { provider: "openai".into(), preserve_line_breaks, ..AppConfig::default() };
            let expected_input = if preserve_line_breaks { original } else { "First line. Second line.\n\nAnother paragraph." };
            set_cached(compute_cache_key(&config, expected_input), translated.into());
            assert_eq!(runtime.block_on(execute_translation(&config, original)).unwrap(), translated);
        }
    }

    #[test]
    fn test_request_limit_cache_bypass_and_cancellation() {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let config = AppConfig { provider: "openai".into(), ..AppConfig::default() };
        let slots = REQUEST_SLOTS.try_acquire_many(4).unwrap();
        assert!(REQUEST_SLOTS.try_acquire().is_err());
        let mut cx = Context::from_waker(Waker::noop());

        // 空密钥会在取得许可后本地失败，不会发送任何网络请求。
        let mut bypass = Box::pin(execute_translation_bypass_cache(&config, "test-bypass"));
        assert!(bypass.as_mut().poll(&mut cx).is_pending());
        drop(bypass);

        let key = compute_cache_key(&config, "test-cancel-pending-request");
        let mut request = Box::pin(execute_translation(&config, "test-cancel-pending-request"));
        assert!(request.as_mut().poll(&mut cx).is_pending());
        let InFlightStatus::Wait(mut waiter) = register_or_join_in_flight(&key) else {
            panic!("限并发排队中的请求仍应合并");
        };
        drop(request);
        assert!(matches!(waiter.try_recv().unwrap(), Err(SnipLingoError::NetworkError(_))));
        assert!(matches!(register_or_join_in_flight(&key), InFlightStatus::Leader(_)));

        let cached_key = compute_cache_key(&config, "test-cached-with-full-queue");
        set_cached(cached_key, "cached".into());
        assert_eq!(runtime.block_on(execute_translation(&config, "test-cached-with-full-queue")).unwrap(), "cached");
        drop(slots);
        assert!(matches!(runtime.block_on(execute_translation_bypass_cache(&config, "test-bypass")), Err(SnipLingoError::MissingApiKey(_))));
        assert_eq!(REQUEST_SLOTS.available_permits(), 4);
    }
}

