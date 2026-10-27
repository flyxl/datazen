//! Workflow 领域层：模型词汇、错误信封、定义分派、命令步骤与运行记录词汇、
//! 计划预校验、目标解析链与定义仓储端口。
//!
//! 词源：`datazen::workflow`（切片 a：model/error/executable；
//! 切片 b：command/history/plan_guard/target_plan + registry 端口半）。全部是
//! 纯词汇与纯规则——判据只依赖入参，宿主与未来其他形态用同一份答案。

pub mod command;
pub mod error;
pub mod executable;
pub mod history;
pub mod model;
pub mod plan_guard;
pub mod repository;
pub mod target_plan;

pub use command::WorkflowCommandStep;
pub use error::WorkflowError;
pub use executable::{
    ExecutableBlock, ExecutableStep, ExecutableWorkflow, TargetDeclaration, WorkflowVersion,
    HIGHEST_SUPPORTED_VERSION,
};
pub use model::{
    BlockKind, BlockRetry, ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, MergeSource,
    StepExecutionResult, StepStatus, TransformColumn, WorkflowBlock, WorkflowDefinition,
    WorkflowExecutionResult, WorkflowListItem, WorkflowOutput, WorkflowSchedule, WorkflowStep,
    WorkflowVariable, WorkflowVisibility,
};
