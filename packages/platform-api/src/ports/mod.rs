//! 端口契约：14 个 trait，**只有 trait，没有任何具体传输类型**。
//!
//! 约定（§4.1）：
//!
//! * `#[async_trait] pub trait X: Send + Sync + 'static`，对象安全，以 `Arc<dyn X>` 注入。
//! * 身份参数用 platform-api 的 ID newtype；`connectionId`（持久化）与 `dbSessionId`
//!   （运行时，永不落盘）**永不混用**。
//! * 失败一律 `Result<_, PortError>`；**业务拒绝不在端口层抛出**——端口返回能力/基础设施
//!   事实，由用例层翻译成 `ApiError`。
//! * 端口不做重试、鉴权、缓存。
//! * 每个端口文件自带它的**私有词汇表**（§2.4），私有词汇不出现在 DTO 里。
//! * `PortError::ArtifactExpired` 是端口层取值，HTTP 映射是 server host 的职责。
//!
//! 14 个 trait = [概要 §6.4](system-overview.md#64-repositories-与环境-ports) 的 10 行 +
//! 本文新增的 `IdentityResolver` + P2 新增的三个物理预算端口
//! （`DriverPoolBudgetPort` / `ControlResourcePort` / `ClusterNodeBudgetPort`，见 [`budget`]）。
//! §6.4 的「repositories」是一张表，**不是一个合并 trait**：落库面按聚合拆成
//! `ProfileRepository` 与 `JobRepository` 两个 trait，实现方不必实现与自己无关的方法
//! （§4.2 不合并 trait）；同一原则下，P2 的物理预算也**没有**并进 `BudgetCoordinator`
//! ——见 [`budget`] 模块头对四个 trait 分工的说明。
//!
//! **不存在 `ExecutionRecordRepository`**：执行的落库投影由 runtime / server host 承担，
//! §6.4 没有这一行，本包也不凭空加一个。

pub mod artifact;
pub mod budget;
pub mod event;
pub mod identity;
pub mod job;
pub mod network;
pub mod policy;
pub mod profile;
pub mod secret;
pub mod session_directory;
pub mod token;

pub use artifact::ArtifactStore;
pub use budget::{
    BudgetCoordinator, ClusterNodeBudgetPort, ControlResourcePort, DriverPoolBudgetPort,
};
pub use event::EventSink;
pub use identity::IdentityResolver;
pub use job::JobRepository;
pub use network::NetworkProvider;
pub use policy::PolicyService;
pub use profile::ProfileRepository;
pub use secret::SecretProvider;
pub use session_directory::SessionDirectory;
pub use token::SubmissionTokenIssuer;
