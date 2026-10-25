//! 端口契约：**一份只有 trait、没有任何具体传输类型**的契约集；唯一的例外是形态中立、
//! 无 I/O 的进程内原语实现（[`clock`] 的 `SystemClock` / `FixedClock`、
//! [`budget::pool_ledger`] 的参考账本，见 `docs/architecture/platform/shared-boundaries-and-ports.md` §4.9）。
//!
//! 约定（§4.1）：
//!
//! * `#[async_trait] pub trait X: Send + Sync + 'static`，对象安全，以 `Arc<dyn X>` 注入。
//! * 身份参数用 platform-api 的 ID newtype；`connectionId`（持久化）与 `dbSessionId`
//!   （运行时，永不落盘）**永不混用**。
//! * 失败一律 `Result<_, PortError>`；**业务拒绝不在端口层抛出**——端口返回能力/基础设施
//!   事实，由用例层翻译成 `ApiError`。
//! * **能力事实用「值」表达，不塞进 `PortError`**：例如目标不被支持时，端口返回
//!   `execution_target::TargetResolution::Unsupported { reason_code }` 而不是抛错。
//!   `PortError` 的变体集合已被 `error.rs` 的 `variant_set_includes_the_p5_job_contract`
//!   守卫钉死、**不可扩展**，因此新增一类「不被支持」只能靠返回值形状承载。
//! * 端口不做重试、鉴权、缓存。
//! * 每个端口文件自带它的**私有词汇表**（§2.4），私有词汇不出现在 DTO 里。
//! * `PortError::ArtifactExpired` 是端口层取值，HTTP 映射是 server host 的职责。
//!
//! 端口契约的**全集** = [概要 §6.4](system-overview.md#64-repositories-与环境-ports)
//! 表中列出的那些 + 本模块新增的 `IdentityResolver` + P2 新增的物理预算端口
//! （`DriverPoolBudgetPort` / `ControlResourcePort` / `ClusterNodeBudgetPort`，见 [`budget`]）
//! 设置仓储已迁入 `datazen-modules::settings`；本包保留兼容导出。
//! + `NativeToolPort`（受控原生版本探针与清理事实）
//! + 隧道定义仓储 `TunnelRepository`（见 [`tunnel`]；隧道定义是独立聚合，
//!   名单不带密钥，单条取用才给全量）
//! + 时基端口 `Clock`（见 [`clock`]；§6.4 也没有这一行——时刻不是仓库，是一个原语）
//! + SSH known-hosts 仓储 `KnownHostsRepository`（见 [`known_hosts`]；**同步**端口，因为
//!   它跑在 `russh` 的 `check_server_key` 回调里；`load` 按契约不失败、读不到即空集合，
//!   `save` 失败报 `PortError::BackendUnavailable`）
//! + **窄端口**（见 §4.10）：`ProfileReader` / `MonitorGrantReader` /
//!   `ExecutionSettingsReader` / `ExecutionTargetResolver` / `MasterKeyProvider` /
//!   `MigrationProfileReader`。
//!
//! §6.4 的「repositories」是一张表，**不是一个合并 trait**：落库面按聚合拆成
//! `ProfileRepository` 与 `JobRepository`，实现方不必实现与自己无关的方法
//! （§4.2 不合并 trait）；同一原则下，P2 的物理预算也**没有**并进 `BudgetCoordinator`
//! ——见 [`budget`] 模块头对各 trait 分工的说明。
//! **按读写再切一刀同样不合并**：上列窄端口是既有端口的**只读投影 / 读侧拆分**
//! （`ProfileReader` 只暴露 `ProfileRepository` 的读方法；`MigrationProfileReader` 只暴露
//! 迁移 profile 的三个读方法——它原先是 workflow 迁移 step 的读侧拆分，那个能力移除后
//! 端口作为三件套的只读投影边界保留；`ExecutionSettingsReader` 只暴露执行期真正要读的两项
//! 设置）。它们与既有 trait **读同一份事实**——实现方同时实现两者，事实仍然只有一份；
//! 消费方按需注入窄的那个，于是「只读」是**类型事实**而不是约定。
//!
//! **不存在 `ExecutionRecordRepository`**：执行的落库投影由 runtime / server host 承担，
//! §6.4 没有这一行，本包也不凭空加一个。
//!
//! **端口全集是机器可读的事实，不是手写的数字**：确切条目见 [`inventory::PORT_TRAITS`]
//! （**唯一的人工清单**），`datazen-platform-api` 的单测拿它与源码里真实的 `pub trait`
//! 声明、本模块头上面那段构成说明、概要 §6.4 的端口表逐项对账；任何一处漂移都会让
//! CI 变红。本模块头与 `lib.rs` 的模块头**都不复述端口数量**——数量由清单派生。

pub mod artifact;
pub mod budget;
pub mod clock;
pub mod connection_catalog;
pub mod event;
pub mod execution_settings;
pub mod execution_target;
pub mod identity;
pub mod inventory;
pub mod job;
pub mod known_hosts;
pub mod master_key;
pub mod migration_profile;
pub mod monitor_grant;
pub mod native_tool;
pub mod network;
pub mod policy;
pub mod profile;
pub mod profile_reader;
pub mod scope;
pub mod secret;
pub mod session_directory;
pub mod token;
pub mod tunnel;

pub use artifact::ArtifactRepository;
pub use budget::{
    BudgetCoordinator, ClusterNodeBudgetPort, ControlResourcePort, DriverPoolBudgetPort,
};
pub use clock::{Clock, FixedClock, SystemClock};
pub use connection_catalog::ConnectionCatalog;
pub use event::EventSink;
pub use execution_settings::ExecutionSettingsReader;
pub use execution_target::ExecutionTargetResolver;
pub use identity::IdentityResolver;
pub use inventory::PORT_TRAITS;
pub use job::JobRepository;
pub use known_hosts::{KnownHostEntry, KnownHostsRepository};
pub use master_key::MasterKeyProvider;
pub use migration_profile::{MigrationProfileReader, MigrationProfileSummary};
pub use monitor_grant::MonitorGrantReader;
pub use network::NetworkProvider;
pub use policy::PolicyService;
pub use profile::ProfileRepository;
pub use profile_reader::ProfileReader;
pub use scope::OrganizationScope;
pub use secret::SecretProvider;
pub use session_directory::SessionDirectory;
pub use token::SubmissionTokenIssuer;
pub use tunnel::TunnelRepository;

pub mod settings;
pub use settings::SettingsRepository;
