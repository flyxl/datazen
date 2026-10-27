//! Workflow 应用层：宿主服务端口、执行上下文、步骤规则与登记服务。
//!
//! host / resource_execution 的端口由宿主实现（桌面 AppState / execution
//! gateway 装配）；规则部分是纯函数，每个宿主拿到同一份判定。

pub mod command_runtime;
pub mod conditions;
pub mod context;
pub mod data_step;
pub mod host;
pub mod registry;
pub mod resource_execution;
// 家族内部结果组装（executor 与 data_step 消费），不进公开面。
pub(in crate::workflow) mod results;
