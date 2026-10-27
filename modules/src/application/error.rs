//! 用例层的失败类型 [`ApiError`]（以及 [`ApiErrorCode`] / [`RetryDisposition`]）的**再导出**：
//! 定义已搬到 `platform-api`。
//!
//! 依据：[概要 §6.3](../../../docs/architecture/platform/system-overview.md)「`ApiError` 包含
//! `code`、脱敏 `message`、`requestId`、`retryDisposition`」与
//! [连接 §13](../../../docs/architecture/platform/connection-management.md) 的 code 表。
//!
//! ## 为什么整个 ApiError 都不在本 crate 定义
//!
//! P0 断环（[ddd-refactor-plan.md](../../../docs/development/ddd-refactor-plan.md)）：用例端口
//! `ConnectionUseCases` 的方法签名返回 `ApiError`，而**实现**该端口的是 `datazen`；
//! runtime 不得依赖 application（R2：两者是兄弟，互不依赖），端口签名里的每个类型就只能住在
//! 更内层的 `platform-api`。同理 `ApiErrorCode`（31 个 code）、`RetryDisposition`、请求 DTO
//! 与端口 trait 本体也都在 platform-api：它们同属「两端都必须看见的词汇」。
//!
//! 这不是行为搬迁——本 crate 内逐字照旧可用（`ApiError::new`、`code.retry_disposition()`、
//! `ApiErrorCode::ALL`、`{code}` 格式化、serde 形态全部不变），只是 `impl` 必须与被 impl 的
//! 类型同 crate（orphan 规则 E0117）。宿主 179 处引用、`runtime` 侧也都有对称的再导出测试，
//! 互相指同一个第三方类型，因此不可能各自漂移。
//!
//! ## 两条必须保持的边界
//!
//! 1. **本类型只表示「请求被拒绝」**。派发后的执行终态失败由
//!    `ExecutionState = failed` + `ExecutionErrorCode` 表达（连接 §13 末段），**不是** `ApiError`。
//!    因此 `ApiErrorCode` 里没有 `hostRejected`——它是 `ExecutionErrorCode` 的取值，
//!    由 platform-api 定义并在其内部以 `execution_error_codes_do_not_leak_into_this_enum` 钉住。
//! 2. **`ApiError` 与 `ExecutionErrorCode` 是两个命名空间**，不得互相塞入对方取值。
//! 3. **本模块不做 `PortError` → HTTP 状态码的映射**：端口失败到 HTTP 的映射是 server host
//!    的职责（同 §4.7「`ArtifactExpired` 映射成 404 而不是 410 由 host 决定」）。端口失败到
//!    **用例** 失败的业务判定由各用例自己做，因为 `CasConflict { entity }` 是否等于
//!    `ConfigRevisionMismatch` 取决于 entity，见 [`ApiError::config_revision_mismatch`] 的说明。
//!
//! ## `message` 必须是脱敏文本
//!
//! `message` 是给调用者看的脱敏说明：driver 原始错误、SQL 片段、绝对路径、凭据**不得**进入
//! 这里（连接 §13 末段：driver 内部错误由网关脱敏映射）。构造时只写入本模块或用例自己拼装的
//! 固定文案；把 `&dyn Display` 形式的外部错误直接塞进 `message` 是本模块明确禁止的用法，
//! 需要归因时用 `tracing` 记录，错误体只给 code + 脱敏文案。

pub use crate::shared::error::{ApiError, ApiErrorCode, RetryDisposition};

#[cfg(test)]
mod tests {
    use super::*;

    /// 编译期证明（不是自报）：`application::ApiError` 与 `application::ApiErrorCode`
    /// **就是** platform-api 里那两个类型，而不是本 crate 另有一份同名副本。
    ///
    /// 双向赋值只有在两侧同类型时才编译得过——若本 crate 保留了本地定义，
    /// 下面第二行的类型标注就会立刻 `mismatched types` 编译失败。
    #[test]
    fn the_api_error_is_the_platform_api_type_not_a_local_copy() {
        let from_reexport: ApiError = ApiError::new(ApiErrorCode::NotFound, "gone");
        let from_home: crate::shared::error::ApiError = from_reexport;
        let round_tripped: ApiError = from_home;
        assert_eq!(round_tripped.code, ApiErrorCode::NotFound);
        // 能力也必须随之可见：`ALL` 与 `retry_disposition()` 是固有成员，经再导出调用即可用。
        assert_eq!(ApiErrorCode::ALL.len(), 31);
        assert_eq!(
            ApiErrorCode::NotFound.retry_disposition(),
            RetryDisposition::SafeRead
        );
    }
}
