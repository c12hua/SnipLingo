use crate::core::error::SnipLingoError;
use super::client::get_http_client;

/// 谷歌翻译免 Key 公共 Web API 适配器
pub async fn translate_google(
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String, SnipLingoError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(SnipLingoError::NoTextDetected);
    }

    let client = get_http_client();
    let sl = if source_lang.is_empty() { "auto" } else { source_lang };
    let tl = if target_lang.is_empty() { "zh-CN" } else { target_lang };

    let url = "https://translate.googleapis.com/translate_a/single";
    let resp = client
        .get(url)
        .query(&[
            ("client", "gtx"),
            ("sl", sl),
            ("tl", tl),
            ("dt", "t"),
            ("q", trimmed),
        ])
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                SnipLingoError::NetworkError(
                    "谷歌翻译连接超时 (10秒)，国内网络环境建议切换为「百度翻译」或配置代理".to_string(),
                )
            } else {
                SnipLingoError::NetworkError(format!("谷歌翻译请求失败: {}", e))
            }
        })?;

    if !resp.status().is_success() {
        return Err(SnipLingoError::NetworkError(format!(
            "谷歌翻译服务响应异常 (HTTP {})",
            resp.status()
        )));
    }

    let json_val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SnipLingoError::NetworkError(format!("解析谷歌翻译响应失败: {}", e)))?;

    // 解析 Google Translate 响应结构
    if let Some(sentences) = json_val.get(0).and_then(|v| v.as_array()) {
        let mut result = String::new();
        for item in sentences {
            if let Some(trans_segment) = item.get(0).and_then(|v| v.as_str()) {
                result.push_str(trans_segment);
            }
        }
        let final_text = result.trim();
        if !final_text.is_empty() {
            return Ok(final_text.to_string());
        }
    }

    Err(SnipLingoError::NetworkError("谷歌翻译未返回有效结果".to_string()))
}
