//! AI 用例错误信封（D6 item 4 收尾，2026-10-26）。
//!
//! 形状与 modules 的 `FeatureError` 逐字同构（变体名、消息格式、
//! `From` 边全部一致）：AI 用例自 modules 归位模块后，宿主经
//! `From<AiUseCaseError>` 映射到与旧 `From<FeatureError>` 相同的传输桶，
//! 同一失败在 IPC 上的文案与类别不变。唯一定义在此；WR-A3 P3 反转后
//! `From<AiUseCaseError> for FeatureError` 落在本文件尾部（`feature_error`
//! 门控），feature_error 不再反向拉 ai。

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

// WR-A3 P3 反向依赖反转：映射边自 crates/datazen/src/feature_error.rs 下沉到本域
// 文件尾部（信封不再反向拉 ai）。差异：`Ai` 臂旧投 `FeatureError::Ai`
// （transparent Display），反转后信封无该变体，改投
// `Internal(error.to_string())`——类别 Ai→Internal，消息逐字不变（Display
// 输出即原文案）；其余 7 臂同桶同消息逐字一致。
#[cfg(feature = "feature_error")]
impl From<AiUseCaseError> for crate::feature_error::FeatureError {
    fn from(error: AiUseCaseError) -> Self {
        match error {
            AiUseCaseError::Ai(error) => Self::Internal(error.to_string()),
            AiUseCaseError::Validation(message) => Self::Validation(message),
            AiUseCaseError::NotFound(message) => Self::NotFound(message),
            AiUseCaseError::NotConfigured(message) => Self::NotConfigured(message),
            AiUseCaseError::Internal(message) => Self::Internal(message),
            AiUseCaseError::Driver(error) => Self::Driver(error),
            AiUseCaseError::Io(error) => Self::Io(error),
            AiUseCaseError::Json(error) => Self::Json(error),
        }
    }
}
