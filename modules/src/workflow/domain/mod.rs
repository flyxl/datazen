//! Workflow 领域层：模型词汇、错误信封与定义分派（D6 第一切片 a）。
//!
//! 词源：`datazen-feature-core::workflow` 的 model / error / executable 三文件
//! 逐字迁移（serde 形状与公开 API 不变）。executable 是纯函数层（版本分派 +
//! 静态校验 + 可执行形态，自述「不碰资源、连接与 Job」），随词汇一起归位
//! domain；执行器/注册表/调度器/资源执行仍留 feature-core（后续切片）。

pub mod error;
pub mod executable;
pub mod model;

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
