//! P5 JobRuntime 内核（§10.1.1 / data-migration-jobs.md）。
//!
//! 职责划分（冻结口径）：
//!
//! * [`JobRepository`]（platform-api 端口）：持久化接受、幂等 receipt、planId 唯一消费、
//!   claim/renew/fencing（§4.3 目标修订在 P5 落地）。
//! * [`JobHandler`]：`validatePlan / runStage / verifyRecovery` 三态语义，按
//!   kind + handlerVersion 注册；runtime 负责 claim、预算、阶段调度、取消意图、事件与 cleanup。
//! * [`JobRuntime`]：编排层，只做状态推进，不做方言、不做 IPC。
//! * 持久化路径排除运行时句柄；事件载荷只含状态/引用（§2.3）。

pub mod budget;
pub mod error;
pub mod handler;
pub mod plan;
pub mod repository;
pub mod runtime;
pub mod time;

pub use budget::{detect_endpoint_overlap, EndpointRef, EndpointRole, MultiEndpointPermits};
pub use error::JobError;
pub use handler::{
    CancelToken, HandlerRegistry, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};
pub use plan::{project_frozen_plan, FrozenPlan, APPLY_KINDS, SUPPORTED_PLAN_MAJOR};
pub use repository::{CancelPollSnapshot, InMemoryJobRepository};
pub use runtime::{JobResult, JobRuntime, CANCEL_POLL_INTERVAL};
pub use time::{JobClock, SharedClock};
