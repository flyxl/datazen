//! ID 词汇迁移垫片（D2，2026-10-26）。
//!
//! 全部 ID newtype、版本号与 `Counter` 的唯一定义自 D2 起在
//! `datazen-modules::shared::id`（DDD 迁移 §5.1：形成归属契约的最小 ID/引用值
//! 放 shared，领域再导出同一类型）。本模块只再导出，旧 import 路径
//! `datazen_platform_api::id::*` 保持不变；D8 退役本 crate 时一并删除。
//!
//! `docs/architecture/naming.md` 的 ID 术语规范继续由同一份定义落地：
//! `connectionId` 与 `dbSessionId` 仍是互不兼容的 newtype，跨类型 `From`
//! 依旧不存在。

pub use datazen_modules::shared::id::*;
