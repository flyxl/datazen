//! 响应解析与提示词文本处理迁移垫片（D6 item 4 切片三，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::ai::application::response`。本模块只再导出，
//! 旧路径 `datazen_feature_core::ai::*` 不变（src-tauri 消费者零改动）；
//! feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::ai::application::response::*;
