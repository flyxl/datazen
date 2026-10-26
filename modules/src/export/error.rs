//! 导出用例错误信封（D6 item 5 切片 a，2026-10-26）。
//!
//! 形状与 feature-core 的 `FeatureError` 逐字同构（变体名、消息格式、
//! `From` 边全部一致）：导出/文件产物作业族自 feature-core 归位模块后，
//! 宿主经 `From<ExportError>` 映射到与旧 `From<FeatureError>` 相同的
//! 传输桶，同一失败在 IPC 上的文案与类别不变。
//!
//! 本信封为**文件产物族共用**：export（表导出/FileArtifactJob）与
//! backup（备份/SQL 恢复）同族同错误语义，不另立第二信封。

/// Errors from portable export use cases. Hosts map these to their transport envelope.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
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

impl From<String> for ExportError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}
