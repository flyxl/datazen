//! Shared feature logic. Hosts bind I/O ports; no native application types occur here.
pub mod ai;
pub mod backup;
pub mod backup_gateway;
pub mod connection_import;
pub mod dashboard;
// D4-d（2026-10-26）：写链路迁入 datazen-modules::table_editing，这里按旧模块
// 名再导出（§5.5），`datazen_feature_core::data_browsing::*` 既有路径不变。
pub use datazen_modules::table_editing as data_browsing;
pub mod error;
pub mod export;
pub mod mcp;
pub mod services;
pub mod workflow;
pub mod workspace_apps;

pub mod file_job;

pub mod restore_job;

pub mod restore_gateway;
