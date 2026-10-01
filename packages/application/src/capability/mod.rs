//! 运行时能力降低机制（P2 计划 `platform-development-plan.md:93`）。
//!
//! 计划第 93 行要求「namespace / session / transaction / snapshot / data / backup
//! 可选能力注册与运行时能力降低机制」。driver-api 侧只做了**声明**
//! （`DatabaseDriverFactory::resource_capabilities()` 默认全未知，
//! 见 `factory.rs:57-66`），**降低**这一半原本没有实现：声明缺失之后，
//! 调用方要么自己猜、要么把失败吞掉。本模块就是那缺失的一半。
//!
//! ## 唯一的失败形状
//!
//! 能力缺失 → [`ApiErrorCode::CapabilityUnsupported`]。没有第二种表达，
//! 也没有「先返回成功、后面再看」的入口。计划第 99 行「新增能力缺失不会 no-op
//! 成功」由 [`CapabilityRegistry::resolve`] 的一个分支保证：
//! [`CapabilityState::Unavailable`] **无条件**返回错误，与
//! [`DegradePolicy`] 无关——策略里没有、也不能有「未声明即忽略」的选项。
//!
//! ## 三层分工
//!
//! | 层 | 位置 | 职责 |
//! |---|---|---|
//! | 词汇 | [`domain`] | 六类能力、三态、授权与完成态的形状 |
//! | 降级 | [`registry`] | 声明 / 降低 / 重新确认 / 取用；失败关闭的状态机 |
//! | 证据 | [`snapshot`] | 可记录、可比较的能力快照与降级时间线 |
//!
//! ## 不做什么
//!
//! * 不碰任何 `platform-api` 端口的**实现体**——那是 P3 的执行面门槛
//!   （计划第 120 行的 H 层 CM-02..07），本轨写了等于替 P3 过门槛。
//! * 不改 `error.rs` 的 [`ApiErrorCode`] 枚举（31 个变体是既有断言），
//!   只**引用** [`ApiErrorCode::CapabilityUnsupported`]。
//! * 不解析 driver-api 的 `CapabilitySet`：声明来源是注入的
//!   [`Declarations`] 闭包，由组装层桥接（见 [`registry`] 模块头）。

mod domain;
mod registry;
mod snapshot;

pub use domain::{
    CapabilityCompletion, CapabilityDomain, CapabilityGrant, CapabilityKey,
    CapabilityRegistrationError, CapabilityState, DegradationCause,
};
pub use registry::{completion_of, CapabilityRegistry, Declarations, DegradePolicy};
pub use snapshot::{CapabilityChange, CapabilitySnapshot, DegradationRecord};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
