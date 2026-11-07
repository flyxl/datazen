//! Schema 服务的应用错误（D4-b，随服务自 modules 迁入）。
//!
//! 变体与 `Display` 字符串与旧 `FeatureError` 对应变体**逐字一致**
//! （§5.1：错误兼容由测试决定，不顺便改文案）；宿主错误映射按同一口径
//! 换算（桌面 `From<SchemaError> for CommandError` 与旧
//! `From<FeatureError>` 的对应臂相同）。

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    NotConfigured(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Driver(#[from] datazen_driver_api::DriverError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<String> for SchemaError {
    fn from(message: String) -> Self {
        Self::Internal(message)
    }
}
