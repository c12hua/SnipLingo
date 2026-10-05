use crate::core::error::SnipLingoError;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};


#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Option<Vec<ChatChoice>>,
}

fn get_lang_display_name(code: &str) -> &'static str {
    match code {
        "zh-CN" => "Simplified Chinese",
        "zh-TW" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "ru" => "Russian",
        "it" => "Italian",
        _ => "Simplified Chinese",
    }
}

pub(super) fn endpoint(base_url: Option<&str>) -> String {
    let raw_base = base_url.unwrap_or("https://api.openai.com/v1").trim().trim_end_matches('/');
    if raw_base.ends_with("/chat/completions") {
        raw_base.to_string()
    } else {
        format!("{}/chat/completions", raw_base)
    }
}

pub(super) fn model_name(model: Option<&str>) -> &str {
    model.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("gpt-4o-mini")
}

pub async fn translate_openai(
    api_key: &str,
    base_url: Option<&str>,
    model: Option<&str>,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String, SnipLingoError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(SnipLingoError::MissingApiKey(
            "尚未配置翻译 API Key，请点击前往配置".to_string(),
        ));
    }

    let endpoint = endpoint(base_url);
    let model_name = model_name(model);

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", key))
            .map_err(|e| SnipLingoError::MissingApiKey(format!("API Key 格式异常: {}", e)))?,
    );

    let client = super::client::get_http_client();

    let target_name = get_lang_display_name(target_lang);
    let system_prompt = if source_lang.is_empty() || source_lang == "auto" {
        format!(
            "You are an expert translator. Translate the user input into clear, natural, and accurate {}. Output ONLY the translated text without explanations, prefixes, or notes.",
            target_name
        )
    } else {
        let source_name = get_lang_display_name(source_lang);
        format!(
            "You are an expert translator. Translate the user input from {} into clear, natural, and accurate {}. Output ONLY the translated text without explanations, prefixes, or notes.",
            source_name, target_name
        )
    };

    let req_body = ChatCompletionRequest {
        model: model_name,
        messages: vec![
            ChatMessage {
                role: "system",
                content: &system_prompt,
            },
            ChatMessage {
                role: "user",
                content: text,
            },
        ],
        temperature: 0.3,
    };

    let resp = client
        .post(&endpoint)
        .headers(headers)
        .json(&req_body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                SnipLingoError::NetworkError("网络请求超时 (12秒)，请检查网络连接或 API 服务状态".to_string())
            } else {
                SnipLingoError::NetworkError(format!("网络请求失败: {}", e))
            }
        })?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(SnipLingoError::MissingApiKey(
            "API Key 无效或未授权，请检查配置".to_string(),
        ));
    }

    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(SnipLingoError::NetworkError(format!(
            "翻译服务响应错误 (HTTP {}): {}",
            status, err_text
        )));
    }

    let parsed = resp
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|e| SnipLingoError::NetworkError(format!("解析翻译响应失败: {}", e)))?;

    if let Some(choices) = parsed.choices {
        if let Some(first) = choices.into_iter().next() {
            if let Some(content) = first.message.content {
                return Ok(content.trim().to_string());
            }
        }
    }

    Err(SnipLingoError::NetworkError("翻译服务返回了空内容".to_string()))
}
