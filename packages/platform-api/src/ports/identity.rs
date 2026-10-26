//! `IdentityResolver` 迁移垫片（D3，2026-10-26）。
//!
//! 执行身份解析端口与 `ExecutionIdentity` 的唯一定义自 D3 起在
//! `datazen-modules::execution::application::identity`（§5.1：授权、凭据和
//! 网络能力接口归实际消费者的 application）。本模块只再导出，旧 import
//! 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::{ExecutionIdentity, IdentityResolver};
