//! 请求上下文迁移垫片（D2，2026-10-26）。
//!
//! `RequestContext` / `OwnerRef` / `DelegationRef` 的唯一定义自 D2 起在
//! `datazen-modules::shared::context`（DDD 迁移 §5.1：可信身份、组织/主体
//! 标识与请求关联信息归 shared）。本模块只再导出，旧 import 路径不变；
//! D8 退役本 crate 时一并删除。

pub use datazen_modules::shared::context::{DelegationRef, OwnerRef, RequestContext};
