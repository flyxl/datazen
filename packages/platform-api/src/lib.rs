//! # datazen-platform-api
//!
//! 平台契约层：**传输无关的领域词汇与端口**。这里只有类型与 trait，没有 Tauri、
//! 没有 HTTP、没有 SQL 连接、没有具体传输实现。
//!
//! ## 本包负责什么
//!
//! * ID newtype 与版本号（`id`）：`connectionId`（持久化）与 `dbSessionId`（运行时）
//!   是**两个互不兼容的类型**，编译期就不可互换；凭据/策略/网络类 id 的 `Debug` 脱敏。
//! * 请求身份（`context`）：`RequestContext` 只能由 adapter 构造，没有 `Default`。
//! * 目标词汇（`target`）：提交目标 → 规范化目标的形状与层次要求。
//! * 错误词汇（`error`）：`PortError`（端口层事实）与 `ApiError` / `ApiErrorCode`（跨边界
//!   业务拒绝）。它们同住一个模块：端口报事实、用例层把事实判定成业务拒绝，而 `runtime`
//!   要实现用例端口就必须看得见 `ApiError`（R2，见 `error` 模块头）。
//! * 契约 DTO（`dto`）：跨边界传输的形状（camelCase 序列化）。
//! * 端口（`ports`）：注入方是 `Arc<dyn X>`；只读窄端口见 `ports::mod` 的模块头与架构文档
//!   §4.10；时基端口 `Clock` 另带系统时钟 `SystemClock` 与固定时钟 `FixedClock`
//!   （见 `ports::clock` 的模块说明）。端口全集见 `ports::inventory::PORT_TRAITS`——
//!   **唯一的人工清单**，条数由单测核对。
//!
//! ## 本包**不**负责什么
//!
//! * 用例**编排**（何时读、何时判、按什么顺序）、目标计算顺序、身份策略顺序 →
//!   `datazen-application`。注意区分：本包承载用例端口的**签名**（`use_cases`），
//!   不承载编排实现。
//! * 物理会话、租约、预算的实际执行 → `datazen-runtime`。
//! * HTTP/Tauri/DB 的具体形态 → server host / desktop host。
//!
//! ## 依赖边界（硬门禁）
//!
//! * **F-04**：本包**不得**依赖 `application` / `runtime` / 领域包 / `src-tauri`。
//!   因此 ID 类型在本包**定义**（而不是从 runtime re-export），迁移方向是
//!   runtime → `pub use datazen_platform_api::id::*`。
//! * **F-02**：`application` / `runtime` / `platform-api` 的依赖闭包里没有 `tauri*`。
//!   本包的 `Cargo.toml` 里没有任何 `tauri` 依赖，这是「业务可在没有 Tauri 的测试进程中
//!   调用」这一目标（`docs/development/platform-development-plan.md` 的 P1 Track A）的前提。
//! * **F-05**：前端只依赖契约，本包不含 react / `@tauri-apps/api`。
//!
//! 设计依据：`docs/architecture/platform/shared-boundaries-and-ports.md`（§2 文件树、
//! §4.1 约定、§4.3–§4.6 端口签名、§8 CI 门禁）、`connection-management.md`
//! （§3 不变量、§4 DTO、§4.4 可落盘来源）、`system-overview.md`（§6.1 上下文、§6.4 端口表）。

pub mod context;
pub mod dto;
pub mod error;
// `id` 用宏批量生成 ID newtype，宏调用点的 `///` 会被 rustc 判为未使用的文档注释
//（rustdoc 仍会在展开后的类型上生效），故在此局部放行。
#[allow(unused_doc_comments)]
pub mod id;
pub mod migration;
pub mod policy;
pub mod ports;
pub mod target;
pub mod use_cases;

pub use context::{DelegationRef, OwnerRef, RequestContext};
pub use error::{ApiError, ApiErrorCode, PortError, RetryDisposition};
pub use ports::{
    ArtifactRepository, BudgetCoordinator, Clock, ClusterNodeBudgetPort, ControlResourcePort,
    DriverPoolBudgetPort, EventSink, ExecutionSettingsReader, ExecutionTargetResolver, FixedClock,
    IdentityResolver, JobRepository, MasterKeyProvider, MigrationProfileReader, MonitorGrantReader,
    NetworkProvider, OrganizationScope, PolicyService, ProfileReader, ProfileRepository,
    SecretProvider, SessionDirectory, SubmissionTokenIssuer, SystemClock,
};
pub use use_cases::ConnectionUseCases;
