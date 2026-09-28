use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// 全局共享 HTTP 连接池，开启 TCP Keep-Alive 和 TCP_NODELAY，复用 TLS 会话，避免重复握手
pub fn get_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .tcp_nodelay(true)
            .tcp_keepalive(Some(Duration::from_secs(90)))
            .pool_idle_timeout(Some(Duration::from_secs(120)))
            .pool_max_idle_per_host(10)
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(15))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// 针对特定服务商的连接预热 (Pre-warming)
/// 在用户唤醒全屏截屏时在后台静默发起，提前完成 DNS 解析与 TLS 1.3 握手
pub fn prewarm_provider(provider: &str, base_url: Option<&str>) {
    let client = get_http_client();
    let url = match provider.to_lowercase().as_str() {
        "google" => "https://translate.googleapis.com/translate_a/single?client=gtx",
        "baidu" => "https://fanyi-api.baidu.com",
        "deepl" => "https://api-free.deepl.com",
        _ => {
            // OpenAI / DeepSeek 等自定义 Base URL
            base_url.unwrap_or("https://api.openai.com/v1")
        }
    };

    let url = url.to_string();
    tauri::async_runtime::spawn(async move {
        // 使用非常轻量的 HEAD/GET 请求握手建连，忽略结果
        let _ = client.head(&url).timeout(Duration::from_secs(3)).send().await;
    });
}

// ======================== LRU 翻译缓存 ========================

const MAX_CACHE_SIZE: usize = 200;

struct TranslationCache {
    map: HashMap<String, String>,
    order: VecDeque<String>,
}

impl TranslationCache {
    fn new() -> Self {
        Self {
            map: HashMap::with_capacity(MAX_CACHE_SIZE),
            order: VecDeque::with_capacity(MAX_CACHE_SIZE),
        }
    }

    fn get(&mut self, key: &str) -> Option<String> {
        if let Some(val) = self.map.get(key) {
            // 移到最新位置
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.to_string());
            return Some(val.clone());
        }
        None
    }

    fn insert(&mut self, key: String, val: String) {
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), val);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key);
            return;
        }

        if self.order.len() >= MAX_CACHE_SIZE {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }

        self.map.insert(key.clone(), val);
        self.order.push_back(key);
    }
}

static CACHE: OnceLock<Mutex<TranslationCache>> = OnceLock::new();

fn get_cache() -> &'static Mutex<TranslationCache> {
    CACHE.get_or_init(|| Mutex::new(TranslationCache::new()))
}

/// 计算缓存唯一指纹 key: provider:source_lang:target_lang:text
pub fn compute_cache_key(provider: &str, source_lang: &str, target_lang: &str, text: &str) -> String {
    format!("{}:{}:{}:{}", provider.to_lowercase(), source_lang, target_lang, text.trim())
}

pub fn get_cached(key: &str) -> Option<String> {
    if let Ok(mut lock) = get_cache().lock() {
        return lock.get(key);
    }
    None
}

pub fn set_cached(key: String, val: String) {
    if let Ok(mut lock) = get_cache().lock() {
        lock.insert(key, val);
    }
}

// ======================== Single-Flight 并发请求合并 ========================

use tokio::sync::broadcast;
use crate::core::error::SnipLingoError;

static IN_FLIGHT: OnceLock<Mutex<HashMap<String, broadcast::Sender<Result<String, SnipLingoError>>>>> = OnceLock::new();

fn get_in_flight() -> &'static Mutex<HashMap<String, broadcast::Sender<Result<String, SnipLingoError>>>> {
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

pub enum InFlightStatus {
    Wait(broadcast::Receiver<Result<String, SnipLingoError>>),
    Leader(broadcast::Sender<Result<String, SnipLingoError>>),
}

/// 注册或加入一个正在进行的并发翻译请求
pub fn register_or_join_in_flight(key: &str) -> InFlightStatus {
    let mut map = get_in_flight().lock().unwrap();
    if let Some(sender) = map.get(key) {
        InFlightStatus::Wait(sender.subscribe())
    } else {
        let (sender, _) = broadcast::channel(1);
        map.insert(key.to_string(), sender.clone());
        InFlightStatus::Leader(sender)
    }
}

/// 结束进行中的翻译请求并从注册表中清理
pub fn finish_in_flight(key: &str) {
    let mut map = get_in_flight().lock().unwrap();
    map.remove(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_lru() {
        let key1 = compute_cache_key("google", "auto", "zh-CN", "hello");
        set_cached(key1.clone(), "你好".to_string());
        assert_eq!(get_cached(&key1), Some("你好".to_string()));
    }

    #[test]
    fn test_single_flight_coalescing() {
        let test_key = "test_provider:en:zh-CN:concurrent_word";

        // 第一个请求成为 Leader
        let status1 = register_or_join_in_flight(test_key);
        let leader_sender = match status1 {
            InFlightStatus::Leader(sender) => sender,
            _ => panic!("首次注册应该成为 Leader"),
        };

        // 第二个相同 key 请求成为 Wait
        let status2 = register_or_join_in_flight(test_key);
        let mut waiter = match status2 {
            InFlightStatus::Wait(rx) => rx,
            _ => panic!("并发请求应该成为 Wait 共享者"),
        };

        // Leader 广播结果
        let expected_text = "并发合并测试结果".to_string();
        leader_sender.send(Ok(expected_text.clone())).unwrap();
        finish_in_flight(test_key);

        // Waiter 应该顺利接收到相同的结果
        let received = waiter.try_recv().unwrap();
        assert_eq!(received.unwrap(), expected_text);

        // finish_in_flight 之后，再次注册应重新成为新的 Leader
        let status3 = register_or_join_in_flight(test_key);
        match status3 {
            InFlightStatus::Leader(_) => {
                finish_in_flight(test_key);
            }
            _ => panic!("完成后的后续请求应该重新成为 Leader"),
        }
    }
}

