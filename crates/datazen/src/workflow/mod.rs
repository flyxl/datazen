//! Workflow：步骤/块/事务边界的领域词汇、用例规则与执行引擎
//! （D6 item 3，2026-10-26；切片 a 词汇 + 切片 b 引擎）。
//!
//! 词源：`datazen::workflow` 全家族逐字迁移。分层：
//! * `domain`——模型词汇（model）、错误信封（error）、定义分派（executable）、
//!   命令步骤词汇（command）、运行记录词汇（history）、计划预校验（plan_guard，
//!   §7 先证明后执行）、执行目标解析链（target_plan，§4 四级链）与定义仓储
//!   端口（repository，trait 归 domain、宿主实现）；
//! * `application`——宿主服务端口（host）、执行上下文与模板解析（context）、
//!   条件求值（conditions）、数据步骤 merge/transform（data_step）、结果组装
//!   （results，家族内部）、命令步骤规则（command_runtime）、资源执行编排
//!   （resource_execution，经 execution::application 端口派发）与登记服务
//!   （registry，播种/列表/保存）；
//! * `engine`——执行引擎（executor：步骤归一为 Driver Command 派发，AI 与
//!   控制流步骤保持通用）与定时调度（scheduler：tick 循环/在途守卫/取消）。
//!
//! 边界（07-stages D6 工作项 3）：保留 step/session/transaction block 的资源
//! 边界（resource_execution 经 execution::application 端口，不持有运行时具体
//! 类型）；调用 execution 与 jobs 的词汇；宿主只实现 host/repository 端口与
//! 持久化（桌面 SQLite / YAML 落盘）。
//!
//! 调度授权（WR-A4 / P6 关闭项）：`domain::grant` 定义定时执行的服务身份与
//! 授权单契约，`application::grant_reader` 是它的只读窄端口；调度器的派发
//! 路径（`engine::scheduler`）在触发时经端口校验授权单后才执行既有路径，
//! 且以 `application::execution_identity` 从放行所依据的授权单派生服务身份
//! 执行上下文（owner 归属、审计主体 kind=Service、无委托）传给宿主。

pub mod application;
pub mod domain;
pub mod engine;

// 旧扁平路径镜像（modules 垫片与 src-tauri 消费者兼容；仅保留有消费者的别名）。
pub use application::command_runtime;
pub use application::grant_reader;
pub use application::host;
pub use application::registry;
pub use application::resource_execution;
pub use application::runtime_reads;
pub use domain::command;
pub use domain::error;
pub use domain::grant;
pub use domain::history;
pub use domain::model;
pub use engine::executor;
pub use engine::scheduler;

pub use domain::command::WorkflowCommandStep;
pub use domain::error::WorkflowError;
pub use domain::model::{
    BlockKind, BlockRetry, ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, MergeSource,
    StepExecutionResult, StepStatus, TransformColumn, WorkflowBlock, WorkflowDefinition,
    WorkflowExecutionResult, WorkflowListItem, WorkflowOutput, WorkflowSchedule, WorkflowStep,
    WorkflowVariable, WorkflowVisibility,
};
