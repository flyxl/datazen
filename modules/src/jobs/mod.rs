//! D5：Job 运行时（07-stages.md D5 工作项 1）。
//!
//! 原 `datazen-kernel::job` 全家（机制内核）与 platform-api 的 Job 端口/DTO 家族
//! （`jobs::domain::repository::JobRepository`、`jobs::domain::job::*`、`ports::clock`、`ports::native_tool`
//! 词汇）整体归位本模块，三分如下（01-target-architecture.md §4.1）：
//!
//! * `domain`——Job 聚合词汇与规则：plan（FrozenPlan 投影）、owner（StageOwnerPermit）、
//!   time（JobClock）、budget（多端预算）、error、job（落盘 DTO 家族）、
//!   repository（JobRepository 端口，§4.1 第 5 条：仓储 trait 在 domain，
//!   实现由宿主提供）、runtime_repository（运行时侧扩展端口）、native_tool
//!   （nativeToolProbe 作业的冻结选择词汇）。
//! * `application`——handler（JobHandler 三态协议 + 注册表）、host（JobHost 装配契约）。
//! * `engine`——runtime（JobRuntime 编排：受理/幂等/claim fencing/调度/取消）、
//!   memory_repository（进程内参考实现，镜像 DDL 不变量）。
//!
//! 引擎不 import 具体迁移功能；Schema Diff / Data Sync / Data Transfer 的 Handler
//! 由各功能实现，宿主注册（§4.2）。

pub mod application;
pub mod domain;
pub mod engine;

// 旧深路径兼容（`datazen_kernel::job::runtime::…` 等经 kernel 垫片穿透；
// 仅保留仍有消费者的别名，其余深路径直接经 `jobs::domain::…`/`jobs::engine::…`）：
pub use domain::{job, repository, runtime_repository};

// 扁平名（镜像原 kernel::job::* 的既有形态，消费者零改动）：
pub use application::handler::{
    CancelToken, HandlerRegistry, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};
pub use application::host::JobHost;
pub use domain::budget::{
    detect_endpoint_overlap, EndpointRef, EndpointRole, MultiEndpointPermits,
};
pub use domain::error::JobError;
pub use domain::owner::StageOwnerPermit;
pub use domain::plan::{project_frozen_plan, FrozenPlan, APPLY_KINDS, SUPPORTED_PLAN_MAJOR};
pub use domain::recovery::JobRecoveryVerifier;
pub use domain::repository::JobRepository;
pub use domain::runtime_repository::JobRuntimeRepository;
pub use domain::time::{JobClock, PortJobClock, SharedClock, SystemJobClock};
pub use engine::memory_repository::{CancelPollSnapshot, InMemoryJobRepository};
pub use engine::runtime::{JobResult, JobRuntime, CANCEL_POLL_INTERVAL};
