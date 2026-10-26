//! Workflow 登记服务（播种/列表/保存）；端口半（record/trait/规则）在 domain::repository，一并再导出保持旧路径迁移垫片（D6 item 3 切片 b，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::workflow::application::registry`。
//! 本模块只再导出，旧路径不变；feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::workflow::application::registry::*;
pub use datazen_modules::workflow::domain::repository::{
    decode_definition, encode_definition, list_item_for, validate_workflow_id,
    WorkflowDefinitionRecord, WorkflowDefinitionRepository,
};
