//! Workflow：步骤/块/事务边界的领域词汇（D6 item 3 第一切片 a）。
//!
//! 本切片只迁**领域层**（model / error / executable：词汇、错误信封与定义
//! 分派）——它是 dashboard（D6 item 2）的前置依赖（组件绑定/归档引用
//! WorkflowDefinition）。执行器、注册表、调度器等机制随后续切片归位
//! `application` / `runtime`，调用 execution 与 jobs（07-stages D6 工作项 3）。

pub mod domain;

pub use domain::error::WorkflowError;
pub use domain::executable::{
    ExecutableBlock, ExecutableStep, ExecutableWorkflow, TargetDeclaration, WorkflowVersion,
    HIGHEST_SUPPORTED_VERSION,
};
pub use domain::model::{
    BlockKind, BlockRetry, ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, MergeSource,
    StepExecutionResult, StepStatus, TransformColumn, WorkflowBlock, WorkflowDefinition,
    WorkflowExecutionResult, WorkflowListItem, WorkflowOutput, WorkflowSchedule, WorkflowStep,
    WorkflowVariable, WorkflowVisibility,
};
