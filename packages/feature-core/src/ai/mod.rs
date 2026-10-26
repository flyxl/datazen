//! AI 模块树迁移垫片总入口（D6 item 4 切片三 + 收尾，2026-10-26）。
//!
//! budget / ctx_yaml / prompt_resolver / response / safety（切片三）与
//! cancel / chat_request / context / filter / ownership / schema_doc /
//! schema_pipeline / single_turn / sql_generation / tool_loop / tools
//! （收尾切片）的唯一定义均在 `datazen-modules::ai`（domain 与
//! application）。各子模块文件只再导出，旧路径
//! `datazen_feature_core::ai::*` 不变（src-tauri 消费者零改动）；
//! feature-core 退役时（D8）随垫片删除。

pub mod budget;
pub mod cancel;
pub mod chat_request;
pub mod ctx_yaml;
pub mod filter;
pub mod prompt_resolver;
pub mod safety;
pub use datazen_modules::ai::{AiDataEgressLevel, AiSafetyGateConfig};
pub mod context;
pub mod ownership;
pub mod response;
pub mod schema_doc;
pub mod schema_pipeline;
pub mod single_turn;
pub mod sql_generation;
pub mod tool_loop;
pub mod tools;
