use crate::core::error::SnipLingoError;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};


#[derive(Serialize)]
struct DeepLRequest<'a> {
    text: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_lang: Option<&'a str>,
    target_lang: &'a str,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    text: String,
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Option<Vec<DeepLTranslation>>,
}

fn to_deepl_source_lang(code: &str) -> Option<&'static str> {
    match code {
        "auto" | "" => None,
        "zh-CN" | "zh-TW" => Some("ZH"),
        "en" => Some("EN"),
        "ja" => Some("JA"),
        "ko" => Some("KO"),
        "fr" => Some("FR"),
        "de" => Some("DE"),
        "es" => Some("ES"),
        "ru" => Some("RU"),
        "it" => Some("IT"),
        other => {
            let up = other.to_uppercase();
            if up.starts_with("ZH") {
                Some("ZH")
            } else if up.starts_with("EN") {
                Some("EN")
            } else {
                None
            }
        }
    }
}

fn to_deepl_target_lang(code: &str) -> &'static str {
    match code {
        "zh-CN" | "zh-TW" => "ZH",
        "en" => "EN-US",
        "ja" => "JA",
        "ko" => "KO",
        "fr" => "FR",
        "de" => "DE",
        "es" => "ES",
        "ru" => "RU",
        "it" => "IT",
        other => {
            let up = other.to_uppercase();
            if up.starts_with("ZH") {
                "ZH"
            } else if up.starts_with("EN") {
                "EN-US"
            } else {
                "ZH"
            }
        }
    }
}

pub async fn translate_deepl(
    api_key: &str,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String, SnipLingoError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(SnipLingoError::MissingApiKey(
            "尚未配置 DeepL API Key，请点击前往配置".to_string(),
        ));
    }

    // DeepL 根据 Key 后缀判断是 Free 还是 Pro
    let endpoint = if key.ends_with(":fx") {
        "https://api-free.deepl.com/v2/translate"
    } else {
        "https://api.deepl.com/v2/translate"
    };

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("DeepL-Auth-Key {}", key))
            .map_err(|e| SnipLingoError::MissingApiKey(format!("DeepL Key 格式异常: {}", e)))?,
    );

    let client = super::client::get_http_client();

    let source = to_deepl_source_lang(source_lang);
    let target = to_deepl_target_lang(target_lang);

    let req_body = DeepLRequest {
        text: vec![text],
        source_lang: source,
        target_lang: target,
    };

    let resp = client
        .post(endpoint)
        .headers(headers)
        .json(&req_body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                SnipLingoError::NetworkError("DeepL 网络请求超时 (12秒)，请检查网络连接".to_string())
            } else {
                SnipLingoError::NetworkError(format!("网络请求失败: {}", e))
            }
        })?;

    let status = resp.status();
    if status.as_u16() == 403 {
        return Err(SnipLingoError::MissingApiKey(
            "DeepL API Key 无效或未授权，请检查设置".to_string(),
        ));
    } else if status.as_u16() == 456 {
        return Err(SnipLingoError::NetworkError(
            "DeepL 翻译额度已耗尽 (Quota Exceeded)".to_string(),
        ));
    }

    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(SnipLingoError::NetworkError(format!(
            "DeepL 服务响应错误 (HTTP {}): {}",
            status, err_text
        )));
    }

    let parsed = resp
        .json::<DeepLResponse>()
        .await
        .map_err(|e| SnipLingoError::NetworkError(format!("解析 DeepL 响应失败: {}", e)))?;

    if let Some(mut trans) = parsed.translations {
        if let Some(first) = trans.pop() {
            return Ok(first.text.trim().to_string());
        }
    }

    Err(SnipLingoError::NetworkError("DeepL 返回了空内容".to_string()))
}
