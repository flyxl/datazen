//! `PolicyService` 迁移垫片（D3，2026-10-26）。
//!
//! 授权端口（`PolicyService` / `AuthorizationSubject` / `AuthorizationAction` /
//! `AuthorizationDecision` / `DelegationGrant` / `PolicyVersionChange` /
//! `PolicyChangeStream`）的唯一定义自 D3 起在
//! `datazen-modules::execution::application::policy`（§5.1：授权接口归实际
//! 消费者的 application）。本模块只再导出，旧 import 路径不变；D8 退役本
//! crate 时一并删除。

pub use datazen_modules::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
    PolicyChangeStream, PolicyService, PolicyVersionChange,
};
