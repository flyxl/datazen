//! Profile 仓储词汇迁移垫片（D2，2026-10-26）。
//!
//! `ProfileScope` / `ProfileRecord` / `ProfileDraft` / `ProfilePatch` /
//! `ProfileRepository` 的唯一定义自 D2 起在
//! `datazen-modules::connection::domain::profile`（§5.1：Profile 模型与仓储
//! 契约归 connection::domain）。本模块只再导出，旧 import 路径
//! `datazen_platform_api::ports::profile::*` 保持不变；D8 退役本 crate 时
//! 一并删除。回显视图 `ProfileView` 与投影入口 `ProfileView::from_record`
//! 在 `datazen-modules::connection::application`（经 `dto::profile` 再导出）。

pub use datazen_modules::connection::domain::{
    ProfileDraft, ProfileDraftPatch, ProfilePatch, ProfileRecord, ProfileRepository, ProfileScope,
};
