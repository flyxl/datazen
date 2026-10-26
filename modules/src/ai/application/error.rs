//! AI 用例错误信封（D6 item 4 收尾，2026-10-26）。
//!
//! 形状与 feature-core 的 `FeatureError` 逐字同构（变体名、消息格式、
//! `From` 边全部一致）：AI 用例自 feature-core 归位模块后，宿主经
//! `From<AiUseCaseError>` 映射到与旧 `From<FeatureError>` 相同的传输桶，
//! 同一失败在 IPC 上的文案与类别不变。唯一定义在此；feature-core 的
//! `FeatureError` 保留 `From<AiUseCaseError>` 供旧路径汇聚。

use crate::ai::domain::types::AiError;

/// Errors from portable AI use cases. Hosts map these to their transport envelope.
#[derive(Debug, thiserror::Error)]
pub enum AiUseCaseError {
    #[error(transparent)]
    Ai(#[from] AiError),
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

impl From<String> for AiUseCaseError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}
