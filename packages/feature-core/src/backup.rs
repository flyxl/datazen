//! 备份与 SQL 恢复用例迁移垫片（D6 item 5 切片 a，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::backup`（BackupSource/
//! BackupEvents 端口、dump/restore 编排、.sql/.sql.gz 流式恢复）。本模块
//! 只再导出，旧路径 `datazen_feature_core::backup::*` 不变（src-tauri 与
//! backup_gateway 消费者零改动）；feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::backup::*;
