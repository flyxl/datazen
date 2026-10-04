//! CM-70 的端到端重放用例。
//!
//! `connection-management.md:1294-1297` 的原文：
//!
//! ```text
//! CM-70 过期幂等键与记录删除（H/W1）
//! - 前置：签名令牌、fake clock，写入已接受且响应丢失。
//! - 步骤：有效期内重发同/不同输入；过期重发；超过记录保留期删除记录，再重放令牌；
//!         伪造 issuedAt/keyVersion。
//! - 断言：同 receipt、不同输入冲突；过期和删除后均不再执行；伪造拒绝；
//!         客户端不能用新键自动重试未知写入；open/context 的 owner 重启后旧令牌
//!         SessionLost，运行时 receipt/token 不落盘。
//! ```
//!
//! 这条不是「零基础」：A1/A2（同 receipt、不同输入冲突）在 `gateway_contract/`
//! 里已经有断言了。这里的职责是把**缺的那几行**补齐，并且全部经由
//! `ExecutionGateway::with_submission_tokens` 的**公开 API** 走一遍——
//! 单元测试能碰 `pub(crate)`，集成测试碰不到，所以它能独立证明「对外真的能用」。
//!
//! 用例按断言分文件，单文件都远低于 800 行上限：
//! - `expiry.rs` A3 有效期内重发 / 过期重发
//! - `retention.rs` A4 超保留期删记录后重放（含协调方点名的反面用例）
//! - `forgery.rs` A5 伪造 issuedAt / keyVersion
//! - `owner_restart.rs` A7 owner 重启后旧令牌 SessionLost
//!
//! 时间一律走 `FixedClock::advance`，不 sleep。

mod gateway_fixtures;

#[path = "cm70/expiry.rs"]
mod expiry;
#[path = "cm70/forgery.rs"]
mod forgery;
#[path = "cm70/owner_restart.rs"]
mod owner_restart;
#[path = "cm70/retention.rs"]
mod retention;

use datazen_runtime::gateway::{ExecutionRequest, GatewayError};
use gateway_fixtures as fx;

/// 把 `Ok` 变成一条带上下文的失败断言。
pub(crate) fn err<T>(result: Result<T, GatewayError>) -> GatewayError {
    match result {
        Ok(_) => panic!("期望被拒，实际受理成功"),
        Err(error) => error,
    }
}

/// 取令牌层给出的机器可读拒绝理由；不是它就直接炸。
pub(crate) fn token_reason(error: &GatewayError) -> &'static str {
    match error {
        GatewayError::SubmissionTokenRejected { reason } => reason,
        other => panic!("期望令牌层拒绝，实际是 {other:?}"),
    }
}

/// 受理一次并返回回执里的 `executionId`（只入队，不下发）。
pub(crate) async fn accept(h: &fx::TokenHarness, req: ExecutionRequest) -> String {
    fx::accept(&h.harness, req).await.as_str().to_owned()
}

/// 受理并**真的下发**一次，返回 `executionId`。
///
/// CM-70 的每条「不再执行」都拿它当前值当基线：先让一次写入真的落下去，
/// 之后任何重放都只许把计数**按在这些数字上**，不许把它们顶高。
pub(crate) async fn write_once(h: &fx::TokenHarness, req: ExecutionRequest) -> String {
    fx::accept_and_dispatch(&h.harness, req)
        .await
        .as_str()
        .to_owned()
}
