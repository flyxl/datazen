//! WR-D6 运维面（配额 / 撤销传播 / 结果 TTL / 配置 CAS / 任务中心）。
//!
//! 本目录是**服务装配层**（host application services）：把 modules 的端口
//! 与 `crate::storage` 的管理库适配器组合成「bootstrap 可直接接线、handler
//! 可直接调用」的最小服务面。纪律：
//!
//! - **不碰路由**：HTTP 绑定（path → handler）归 `http::router`（主线接线），
//!   本目录只提供参数含组织身份（`RequestContext`）的 handler 级函数。
//! - **错误词汇唯一**：跨边界的失败一律 [`datazen::shared::error::ApiError`]；
//!   端口事实（`PortError`）与存储事实（`StorageError`）经本模块的两个映射
//!   函数落地，不在本目录自造错误类型进 wire。
//! - **事务边界**：涉及管理库写的事务边界在本目录内显式可见（`revocation`
//!   的同事务组合是 WR-D6 的验收核心）。
//!
//! 对外最小面（`pub use`）：每个子件只导出「bootstrap 构造所需的服务类型 +
//! handler 级入口 + 失败类型」，内部 helper 一律私有。

pub mod config_cas;
pub mod quota;
pub mod revocation;
pub mod task_center;
pub mod ttl;

pub use config_cas::{ConfigCasFailure, ProfileConfigCas, ProfileConfigSnapshot};
pub use quota::{QuotaAdmission, QuotaGate, QuotaRejection, QuotaRejectionReason};
pub use revocation::{
    EventSinkBridge, PermissionCheck, RevocationCommit, RevocationEventSink, RevocationOutcome,
    RevocationService,
};
pub use task_center::{CancelOutcome, CancelRegistry, TaskCenter, TaskListPage, TaskListQuery};
pub use ttl::{TtlSweepConfig, TtlSweepReport, TtlSweeper};

use datazen::shared::error::{ApiError, ApiErrorCode, PortError};

/// 端口事实 → wire 语义的统一映射（task_center / ttl / quota 共用）。
///
/// 依据 `http::response::status_for` 的既有映射裁决：
/// - `NotFound` / `ArtifactExpired` → `notFound`（404；防 ID 枚举同口径）；
/// - `QuotaExceeded` → `quotaExceeded`（429）；
/// - `StaleClaim` / `IdempotencyConflict` / `PlanAlreadyConsumed` →
///   `contextConflict`（409 状态冲突类）；
/// - 其余（`BackendUnavailable` / `ProviderTimeout` / `TokenInvalid`）→
///   `serviceUnavailable`（503）：基础设施事实不伪装成业务拒绝。
pub fn port_error_to_api_error(source: &PortError) -> ApiError {
    let message = source.to_string();
    match source {
        PortError::NotFound(_) | PortError::ArtifactExpired => {
            ApiError::new(ApiErrorCode::NotFound, message)
        }
        PortError::QuotaExceeded(_) => ApiError::new(ApiErrorCode::QuotaExceeded, message),
        PortError::StaleClaim
        | PortError::IdempotencyConflict
        | PortError::PlanAlreadyConsumed(_) => ApiError::new(ApiErrorCode::ContextConflict, message),
        // BackendUnavailable / ProviderTimeout / TokenInvalid / CasConflict
        // —— CasConflict 由调用方按实体语义细分（`config_cas` 自行裁决为
        // `configRevisionMismatch`），通用路径保守映射 503。
        _ => ApiError::new(ApiErrorCode::ServiceUnavailable, message),
    }
}

/// 管理库事实 → wire 语义的统一映射：存储失败一律 `serviceUnavailable`
/// （503；`StorageError` 是进程边界/基础设施事实，不携带业务判定）。
pub fn storage_error_to_api_error(source: &crate::storage::StorageError) -> ApiError {
    ApiError::new(ApiErrorCode::ServiceUnavailable, source.to_string())
}

/// 审计/日志用的定宽 RFC3339 UTC（毫秒）时刻（与 `shared::clock` 单一形状
/// 同口径；与 `policy::now_string` 同款实现——本目录不依赖 policy 私有项）。
pub(crate) fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_error_mapping_follows_the_status_table() {
        let not_found = port_error_to_api_error(&PortError::NotFound("j".into()));
        assert_eq!(not_found.code, ApiErrorCode::NotFound);
        let quota = port_error_to_api_error(&PortError::QuotaExceeded("full".into()));
        assert_eq!(quota.code, ApiErrorCode::QuotaExceeded);
        let backend = port_error_to_api_error(&PortError::BackendUnavailable("db".into()));
        assert_eq!(backend.code, ApiErrorCode::ServiceUnavailable);
    }

    #[test]
    fn storage_error_maps_to_service_unavailable() {
        let error = storage_error_to_api_error(&crate::storage::StorageError::Query("x".into()));
        assert_eq!(error.code, ApiErrorCode::ServiceUnavailable);
    }
}
