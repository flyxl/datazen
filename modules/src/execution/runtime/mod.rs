//! Execution 私有机制：预算账本、驱动登记表等（§5.3，D3）。
//!
//! 原 `datazen-kernel` 的机制内核（budget / driver_registry）与后续自
//! `datazen` 迁入的会话 actor、lease、队列、等待、回收驻留于此。
//!
//! **过渡期可见性**：本模块暂为 `pub`——旧 `datazen-kernel` /
//! `datazen` 需要再导出这些类型以保持既有路径（§5.5 单向委托）。
//! 用例编排（`RuntimeConnectionUseCases`）已按 §5.3 拆出，住在
//! `execution::application::orchestration`。
//! 按目标设计（§4.3），最终只经模块装配入口暴露服务与关停句柄，机制
//! 类型在 D8 收敛为非公开。

pub mod artifact;
pub mod budget;
pub mod connection;
pub mod directory;
pub mod driver_mapping;
pub mod driver_registry;
pub mod gateway;
/// 门禁开销的测量口径（分位数算法 + 逐请求求和）。
pub mod latency;
pub mod registry;
pub mod resource;
pub mod tunnel;
