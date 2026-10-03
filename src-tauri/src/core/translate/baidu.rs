use crate::core::error::SnipLingoError;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
struct BaiduTransResult {
    dst: String,
}

#[derive(Deserialize)]
struct BaiduResponse {
    error_code: Option<String>,
    error_msg: Option<String>,
    trans_result: Option<Vec<BaiduTransResult>>,
}

fn to_baidu_source_lang(code: &str) -> &'static str {
    match code {
        "auto" | "" => "auto",
        "zh-CN" => "zh",
        "zh-TW" => "cht",
        "en" => "en",
        "ja" => "jp",
        "ko" => "kor",
        "fr" => "fra",
        "es" => "spa",
        "ru" => "ru",
        "de" => "de",
        "it" => "it",
        other => {
            if other.starts_with("zh") {
                "zh"
            } else {
                "auto"
            }
        }
    }
}

fn to_baidu_target_lang(code: &str) -> &'static str {
    match code {
        "zh-CN" => "zh",
        "zh-TW" => "cht",
        "en" => "en",
        "ja" => "jp",
        "ko" => "kor",
        "fr" => "fra",
        "es" => "spa",
        "ru" => "ru",
        "de" => "de",
        "it" => "it",
        _ => "zh",
    }
}

/// 百度翻译开放平台适配器
pub async fn translate_baidu(
    app_id: &str,
    secret_key: Option<&str>,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String, SnipLingoError> {
    let aid = app_id.trim();
    let sk = secret_key.unwrap_or("").trim();

    if aid.is_empty() || sk.is_empty() {
        return Err(SnipLingoError::MissingApiKey(
            "请先在设置中配置百度翻译的 App ID 与 密钥 (Secret Key)".to_string(),
        ));
    }

    let salt = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "1435660288".to_string());

    // 百度翻译开放平台签名算法: md5(appid + q + salt + key)
    let sign_raw = format!("{}{}{}{}", aid, text, salt, sk);
    let sign = format!("{:x}", md5::compute(sign_raw.as_bytes()));

    let from = to_baidu_source_lang(source_lang);
    let to = to_baidu_target_lang(target_lang);

    let client = super::client::get_http_client();

    let url = "https://fanyi-api.baidu.com/api/trans/vip/translate";
    let resp = client
        .post(url)
        .form(&[
            ("q", text),
            ("from", from),
            ("to", to),
            ("appid", aid),
            ("salt", &salt),
            ("sign", &sign),
        ])
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                SnipLingoError::NetworkError("百度翻译连接超时 (10秒)，请检查网络连接".to_string())
            } else {
                SnipLingoError::NetworkError(format!("百度翻译请求失败: {}", e))
            }
        })?;

    if !resp.status().is_success() {
        return Err(SnipLingoError::NetworkError(format!(
            "百度翻译服务响应异常 (HTTP {})",
            resp.status()
        )));
    }

    let parsed = resp
        .json::<BaiduResponse>()
        .await
        .map_err(|e| SnipLingoError::NetworkError(format!("解析百度翻译响应失败: {}", e)))?;

    if let Some(err_code) = parsed.error_code {
        if err_code != "52000" {
            let err_msg = parsed.error_msg.unwrap_or_default();
            return match err_code.as_str() {
                "52003" => Err(SnipLingoError::MissingApiKey(
                    "百度翻译 App ID 未授权或无效，请检查配置".to_string(),
                )),
                "54001" => Err(SnipLingoError::MissingApiKey(
                    "百度翻译签名错误，请检查密钥 (Secret Key) 是否填写正确".to_string(),
                )),
                "54004" => Err(SnipLingoError::NetworkError(
                    "百度翻译账户额度不足，请在开放平台开通免费额度或充值".to_string(),
                )),
                _ => Err(SnipLingoError::NetworkError(format!(
                    "百度翻译错误 ({}): {}",
                    err_code, err_msg
                ))),
            };
        }
    }

    if let Some(trans_results) = parsed.trans_result {
        let lines: Vec<String> = trans_results.into_iter().map(|item| item.dst).collect();
        let final_text = lines.join("\n");
        if !final_text.trim().is_empty() {
            return Ok(final_text);
        }
    }

    Err(SnipLingoError::NetworkError("百度翻译未返回有效结果".to_string()))
}
