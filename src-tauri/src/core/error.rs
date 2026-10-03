use serde::Serialize;

/// SnipLingo 标准化错误分类枚举
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", content = "message")]
pub enum SnipLingoError {
    /// 截图捕获失败 (屏幕锁定、显卡驱动等原因)
    CaptureFailed(String),
    /// 未在选区中检测到可识别的文字
    NoTextDetected,
    /// OCR 识别过程出错
    OcrFailed(String),
    /// 尚未配置翻译 API Key
    MissingApiKey(String),
    /// 网络连接失败 (超时、DNS、离线等)
    NetworkError(String),
}

impl std::fmt::Display for SnipLingoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnipLingoError::CaptureFailed(msg) => write!(f, "截图失败: {}", msg),
            SnipLingoError::NoTextDetected => write!(f, "未在选区中检测到有效文字"),
            SnipLingoError::OcrFailed(msg) => write!(f, "OCR 识别失败: {}", msg),
            SnipLingoError::MissingApiKey(msg) => write!(f, "缺少 API Key: {}", msg),
            SnipLingoError::NetworkError(msg) => write!(f, "网络请求失败: {}", msg),
        }
    }
}
