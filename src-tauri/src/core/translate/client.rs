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

/// 配置相关的结构化键，避免分隔符歧义；含凭据与正文，仅驻留内存，禁止写入日志。
pub fn compute_cache_key(config: &crate::core::config::AppConfig, text: &str) -> String {
    let provider = config.provider.to_lowercase();
    let identity = match provider.as_str() {
        "google" => Vec::new(),
        "baidu" => {
            let (app_id, secret) = config.get_baidu_credentials();
            vec![app_id.trim().to_string(), secret.unwrap_or_default().trim().to_string()]
        }
        "deepl" => vec![config.get_deepl_credentials().trim().to_string()],
        _ => {
            let (key, base_url, model) = config.get_openai_credentials();
            vec![
                key.trim().to_string(),
                super::openai::endpoint(base_url.as_deref()),
                super::openai::model_name(model.as_deref()).to_string(),
            ]
        }
    };
    serde_json::to_string(&(
        provider, identity, &config.source_lang, &config.target_lang, text.trim(),
    )).expect("字符串元组应可序列化")
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
    Leader(InFlightLeader),
}

pub struct InFlightLeader {
    key: String,
    sender: broadcast::Sender<Result<String, SnipLingoError>>,
    result: Option<Result<String, SnipLingoError>>,
}

impl InFlightLeader {
    pub fn finish(mut self, result: Result<String, SnipLingoError>) {
        self.result = Some(result);
    }
}

impl Drop for InFlightLeader {
    fn drop(&mut self) {
        let mut map = get_in_flight().lock().unwrap_or_else(|e| e.into_inner());
        // 与订阅共用锁，避免完成广播后才加入的等待者漏掉结果；取消也会走这里。
        map.remove(&self.key);
        let result = self.result.take().unwrap_or_else(|| {
            Err(SnipLingoError::NetworkError("并发翻译请求已被取消或中断".into()))
        });
        let _ = self.sender.send(result);
    }
}

/// 注册或加入一个正在进行的并发翻译请求
pub fn register_or_join_in_flight(key: &str) -> InFlightStatus {
    let mut map = get_in_flight().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(sender) = map.get(key) {
        InFlightStatus::Wait(sender.subscribe())
    } else {
        let (sender, _) = broadcast::channel(1);
        map.insert(key.to_string(), sender.clone());
        InFlightStatus::Leader(InFlightLeader { key: key.to_string(), sender, result: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::AppConfig;

    #[test]
    fn test_cache_lru() {
        let mut cache = TranslationCache::new();
        for i in 0..MAX_CACHE_SIZE {
            cache.insert(i.to_string(), i.to_string());
        }
        assert_eq!(cache.get("0"), Some("0".into()));
        cache.insert("1".into(), "updated".into());
        cache.insert("new".into(), "new".into());
        assert_eq!(cache.get("0"), Some("0".into()));
        assert_eq!(cache.get("1"), Some("updated".into()));
        assert_eq!(cache.get("2"), None);
        assert_eq!(cache.map.len(), MAX_CACHE_SIZE);
        assert_eq!(cache.order.len(), MAX_CACHE_SIZE);
    }

    #[test]
    fn test_cache_key_effective_config() {
        let config = AppConfig {
            provider: "openai".into(),
            openai_api_key: Some("key-a".into()),
            ..AppConfig::default()
        };
        let key = compute_cache_key(&config, "hello");
        let mut normalized = config.clone();
        normalized.provider = "OpenAI".into();
        normalized.openai_api_key = Some(" key-a ".into());
        normalized.openai_base_url = Some(" https://api.openai.com/v1/chat/completions/ ".into());
        normalized.openai_model = Some(" gpt-4o-mini ".into());
        assert_eq!(compute_cache_key(&normalized, " hello \n"), key);

        for changed in [
            AppConfig { openai_model: Some("another-model".into()), ..config.clone() },
            AppConfig { openai_base_url: Some("http://localhost:11434/v1".into()), ..config.clone() },
            AppConfig { openai_api_key: Some("key-b".into()), ..config.clone() },
            AppConfig { source_lang: "en".into(), ..config.clone() },
            AppConfig { target_lang: "ja".into(), ..config.clone() },
        ] {
            assert_ne!(compute_cache_key(&changed, "hello"), key);
        }
        assert_ne!(compute_cache_key(&config, "different text"), key);

        let google = AppConfig::default();
        let irrelevant = AppConfig { openai_model: Some("unused".into()), ..google.clone() };
        assert_eq!(compute_cache_key(&google, "hello"), compute_cache_key(&irrelevant, "hello"));
        let left = AppConfig { source_lang: "en:zh".into(), target_lang: "x".into(), ..google.clone() };
        let right = AppConfig { source_lang: "en".into(), target_lang: "zh:x".into(), ..google };
        assert_ne!(compute_cache_key(&left, "hello"), compute_cache_key(&right, "hello"));
    }

    #[test]
    fn test_cache_key_provider_credentials() {
        let baidu = AppConfig {
            provider: "baidu".into(),
            baidu_app_id: Some("app-a".into()),
            baidu_secret_key: Some("secret-a".into()),
            ..AppConfig::default()
        };
        let changed = AppConfig { baidu_secret_key: Some("secret-b".into()), ..baidu.clone() };
        assert_ne!(compute_cache_key(&baidu, "hello"), compute_cache_key(&changed, "hello"));
        let changed = AppConfig { baidu_app_id: Some("app-b".into()), ..baidu.clone() };
        assert_ne!(compute_cache_key(&baidu, "hello"), compute_cache_key(&changed, "hello"));
        let deepl = AppConfig {
            provider: "deepl".into(),
            deepl_api_key: Some("key:fx".into()),
            ..AppConfig::default()
        };
        let changed = AppConfig { deepl_api_key: Some("pro-key".into()), ..deepl.clone() };
        assert_ne!(compute_cache_key(&deepl, "hello"), compute_cache_key(&changed, "hello"));
    }

    #[test]
    fn test_single_flight_coalescing_and_cancellation() {
        let key = "test-single-flight-completion";
        let InFlightStatus::Leader(leader) = register_or_join_in_flight(key) else {
            panic!("首次注册应成为 Leader");
        };
        let InFlightStatus::Wait(mut waiter) = register_or_join_in_flight(key) else {
            panic!("相同请求应等待 Leader");
        };
        leader.finish(Ok("translated".into()));
        assert_eq!(waiter.try_recv().unwrap().unwrap(), "translated");

        let InFlightStatus::Leader(leader) = register_or_join_in_flight(key) else {
            panic!("完成后应释放在途记录");
        };
        let InFlightStatus::Wait(mut waiter) = register_or_join_in_flight(key) else {
            panic!("相同请求应等待 Leader");
        };
        drop(leader);
        assert!(matches!(waiter.try_recv().unwrap(), Err(SnipLingoError::NetworkError(_))));
        assert!(matches!(register_or_join_in_flight(key), InFlightStatus::Leader(_)));
    }
}

