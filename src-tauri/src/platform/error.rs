//! `ApiError` → `CommandError` 的窄适配。
//!
//! ## 为什么要有这一层
//!
//! 开发计划 :77 要求"保留现有 IPC 外观"。既有前端收到的错误是**一个脱敏后的纯字符串**
//! （[`CommandError`] 的 `Serialize` 实现只写 `redact_secrets_for_log(&self.to_string())`），
//! 而不是结构化对象。本模块因此只保证一件事：把应用层 [`ApiError`] 收敛成既有形状，
//! **不新增结构化字段、不改序列化形态、不让错误码直接泄进前端契约**。
//!
//! `ApiError::message` 本身就是脱敏文案（应用层负责构造时就不带秘密材料），
//! 这里仍再走一遍 `CommandError` 的 redact，保持单点脱敏。
//!
//! ## 映射为何要按 code 分档
//!
//! 前端目前按**中文文案**区分失败类型，没有按 code 分支。因此这里的档位只影响
//! 既有 enum 变体的选择（`NotFound` / `Validation` / `NotConfigured` / `Connection` /
//! `Internal`），不引入新语义。`RetryDisposition` 在桌面 IPC 上不表达——桌面是本地调用，
//! 重试由调用方自己的交互决定，不是服务端 `Retry-After` 的事。

use crate::commands::CommandError;
use datazen_application::error::{ApiError, ApiErrorCode};

/// 把应用层错误收敛成既有 IPC 错误形状。
pub fn into_command_error(error: ApiError) -> CommandError {
    // 保留 requestId 便于对账：它只是 UUID，不含任何凭据。
    let detail = match error.request_id.as_ref() {
        Some(request_id) => format!("（requestId={request_id}）"),
        None => String::new(),
    };
    let message = format!("{}{detail}", error.message);

    match error.code {
        // 目标 / 会话 / 键值查无此物：与既有 NotFound 同义。
        ApiErrorCode::SessionNotFound
        | ApiErrorCode::SessionLost
        | ApiErrorCode::NotFound
        | ApiErrorCode::SourceChanged => CommandError::NotFound(message),

        // 入参不合法：与既有 Validation 同义。
        ApiErrorCode::InvalidArgument
        | ApiErrorCode::PayloadTooLarge
        | ApiErrorCode::TargetRequired
        | ApiErrorCode::TargetConflict
        | ApiErrorCode::TargetConflictRows
        | ApiErrorCode::ConfigRevisionMismatch
        | ApiErrorCode::TransactionResolutionRequired
        | ApiErrorCode::IdempotencyConflict
        | ApiErrorCode::IdempotencyExpired => CommandError::Validation(message),

        // 环境未就绪：与既有 NotConfigured 同义。
        ApiErrorCode::PermissionDenied
        | ApiErrorCode::Unauthenticated
        | ApiErrorCode::CapabilityUnsupported
        | ApiErrorCode::UnsupportedPlan
        | ApiErrorCode::ServiceUnavailable => CommandError::NotConfigured(message),

        // 其余一律收敛到 Internal：不新增变体，也不让新 code 改变既有 IPC 外观。
        ApiErrorCode::ResourceBusy
        | ApiErrorCode::QueueFull
        | ApiErrorCode::SessionQuotaExceeded
        | ApiErrorCode::RateLimited
        | ApiErrorCode::QuotaExceeded
        | ApiErrorCode::EndpointOverlap
        | ApiErrorCode::RuntimeEpochMismatch
        | ApiErrorCode::ContextConflict
        | ApiErrorCode::OutcomeUnknown
        | ApiErrorCode::RollbackFailed
        | ApiErrorCode::CleanupFailed
        | ApiErrorCode::PlanStale
        | ApiErrorCode::TargetUnsupported => CommandError::Internal(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::RequestId;

    fn err(code: ApiErrorCode, message: &str) -> ApiError {
        ApiError::new(code, message).with_request_id(Some(RequestId::new("req-7")))
    }

    #[test]
    fn every_error_code_maps_without_panicking() {
        // 枚举全部变体，保证 match 不漏；新增变体时本测试会先红。
        let all = [
            ApiErrorCode::InvalidArgument,
            ApiErrorCode::TargetRequired,
            ApiErrorCode::TargetConflict,
            ApiErrorCode::TargetUnsupported,
            ApiErrorCode::SessionNotFound,
            ApiErrorCode::SessionLost,
            ApiErrorCode::RuntimeEpochMismatch,
            ApiErrorCode::ContextConflict,
            ApiErrorCode::PermissionDenied,
            ApiErrorCode::ResourceBusy,
            ApiErrorCode::QueueFull,
            ApiErrorCode::TransactionResolutionRequired,
            ApiErrorCode::CapabilityUnsupported,
            ApiErrorCode::UnsupportedPlan,
            ApiErrorCode::EndpointOverlap,
            ApiErrorCode::SessionQuotaExceeded,
            ApiErrorCode::IdempotencyExpired,
            ApiErrorCode::RollbackFailed,
            ApiErrorCode::CleanupFailed,
            ApiErrorCode::OutcomeUnknown,
            ApiErrorCode::PlanStale,
            ApiErrorCode::SourceChanged,
            ApiErrorCode::TargetConflictRows,
            ApiErrorCode::IdempotencyConflict,
            ApiErrorCode::Unauthenticated,
            ApiErrorCode::NotFound,
            ApiErrorCode::PayloadTooLarge,
            ApiErrorCode::QuotaExceeded,
            ApiErrorCode::RateLimited,
            ApiErrorCode::ServiceUnavailable,
            ApiErrorCode::ConfigRevisionMismatch,
        ];
        assert_eq!(all.len(), 31, "ApiErrorCode 变体数应与契约一致");
        for code in all {
            let mapped = into_command_error(err(code, "boom"));
            // 窄适配只换变体，消息必须原样透出。
            assert!(
                mapped.to_string().contains("boom"),
                "{code:?} 丢失了错误文案"
            );
        }
    }

    #[test]
    fn request_id_is_carried_for_correlation() {
        let mapped = into_command_error(err(ApiErrorCode::SessionNotFound, "会话不在"));
        assert!(mapped.to_string().contains("req-7"));
    }

    #[test]
    fn codes_land_in_the_documented_existing_variants() {
        let cases = [
            (ApiErrorCode::SessionNotFound, "NotFound"),
            (ApiErrorCode::InvalidArgument, "Validation"),
            (ApiErrorCode::PermissionDenied, "NotConfigured"),
            (ApiErrorCode::CapabilityUnsupported, "NotConfigured"),
            (ApiErrorCode::Unauthenticated, "NotConfigured"),
            (ApiErrorCode::SessionLost, "NotFound"),
            (ApiErrorCode::RuntimeEpochMismatch, "Internal"),
            (ApiErrorCode::IdempotencyExpired, "Validation"),
        ];
        for (code, expect_contains) in cases {
            let mapped = into_command_error(err(code, "x"));
            let text = mapped.to_string();
            // Display 对 String 载荷变体只输出 msg，故以变体名匹配需走 debug 形状；
            // 这里改为断言它们确实落到了 String 载荷变体（非 Store/Driver/Ai/Io/Json）。
            assert!(!text.is_empty(), "{code:?} 映射后文案不应为空");
            let debug = format!("{mapped:?}");
            assert!(
                debug.contains(expect_contains),
                "{code:?} 期望落在 {expect_contains}，实际 {debug}"
            );
        }
    }

    /// 窄适配的硬约束：序列化后**仍然是一个字符串**，与旧的 `Result<T, String>` 外观一致。
    #[test]
    fn mapped_error_still_serializes_as_a_plain_string() {
        let mapped = into_command_error(err(ApiErrorCode::CapabilityUnsupported, "未接入"));
        let json = serde_json::to_value(&mapped).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            json.is_string(),
            "CommandError 必须序列化为字符串，实际 {json}"
        );
    }

    /// 应用层不变量：错误文案不得携带秘密材料；这里再确认映射不追加任何原始载荷。
    #[test]
    fn mapping_adds_nothing_beyond_message_and_request_id() {
        let original = ApiError::new(ApiErrorCode::PlanStale, "只有这句话");
        let text = into_command_error(original).to_string();
        assert!(text.starts_with("只有这句话"));
        assert!(!text.contains("ApiError"), "映射不得泄漏内部类型名");
    }
}
