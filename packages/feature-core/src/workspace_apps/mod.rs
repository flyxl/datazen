//! Workspace App 形态中立内核迁移垫片（D6 item 5 切片 c，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::wapps`（manifest 模式与校验、
//! ZIP/目录安装、按 wapp 隔离的键值存储、注册表、URI 语法与路由判定）。
//! 本模块只再导出（平铺符号 + 子模块深路径），旧路径
//! `datazen_feature_core::workspace_apps::*` 与
//! `…::workspace_apps::{install,manager,manifest,protocol,storage}::*` 不变
//! （src-tauri 与集成测试消费者零改动）；feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::wapps::*;
pub use datazen_modules::wapps::{install, manager, manifest, protocol, storage};
