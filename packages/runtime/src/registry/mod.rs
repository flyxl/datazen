//! 会话登记表（§6.3 / §6.5 / §7 / §9.4 / §12）。
//!
//! 进程内、按会话串行 + 跨会话并行的注册与执行仲裁器。
//! 本模块**拥有** [`SessionPort`] 的实现（`SessionRegistry` 即是那个实现），
//! 对外只投影 [`SessionView`]——物理连接句柄永不上浮到会话面。
//!
//! ## 子模块分工
//!
//! | 文件 | 职责 | 关键约束 |
//! | --- | --- | --- |
//! | [`port`] | 冻结契约面 [`SessionPort`] | 除 D-01 外一字不改 |
//! | [`receipt`] | §7.6 [`CancelReceipt`] | `disposition` 是**控制请求处置**，不是异常 |
//! | [`actor`] | 每会话 actor + 双邮箱 | 串行靠队列，不靠全局锁；控制路径旁路执行队列 |
//! | [`registry`] | 登记表、配额账、审计账 | 表锁只用来定位/插入，取出 `Arc` 后立即释放 |
//! | [`backend`] | 物理资源接缝 | 五个方法的调用顺序由 actor 的释放例程保证 |
//! | [`handles`] | §6.5 句柄登记 + CM-24 取消绑定 | 登记只在内存；伪造绑定零副作用拒绝 |
//! | [`epoch`] | 世代与 D-02 出口折叠 | 折叠是纯函数且全覆盖 |
//! | [`audit`] | CM-72 审计条目 | 只带非敏感能力版本；处置与执行终态正交 |
//!
//! ## 硬约束落在哪
//!
//! - **锁不跨 `.await`**：见 [`registry`]（表锁只定位/插入）与 [`actor`]（in-flight 执行
//!   以 `JoinHandle` 携带状态按值离开 actor 循环）。
//! - **同会话串行 / 跨会话并行**：见 [`actor`] 的两条邮箱——执行队列 FIFO 串行，
//!   取消/作废走控制邮箱，不排队等 in-flight 结束。
//! - **不靠配置找一个替代会话**（CM-71 / §6.5）：见 [`registry`] 的 `locate`——
//!   只有 `dbSessionId` + `runtimeEpoch` **精确匹配**才算命中，配置侧字段不参与查找。
//! - **端口层返回 `RuntimeError` 而非 `PortError`**（D-05 刻意的分层）：见 [`port`]。
//!
//! ## 本模块不导出
//!
//! - `dbSessionId` 的生成与归属校验（§12.1：归 `SessionDirectory`，registry 只消费）；
//! - actor 邮箱、调度队列、登记表的读写锁（§6.3 的内部实现）；
//! - 资源层句柄（`ResourceHandle`、Lease、预算许可）；
//! - 任何 Job 实现（CM-58 只出「剩余配额」查询口，Job 归 gateway 轨道）。
//!
//! ## 已知冻结面偏差
//!
//! - **D-01 已补齐**：[`SessionPort::cancel_execution`] 现返回 [`CancelReceipt`]。
//! - **D-02 折叠**：[`epoch::fold_exit`] 对 `ProviderError::RuntimeEpochMismatch` 返回
//!   `SessionNotFound`，刻意不同于 `ProviderError::api_code()`。
//! - `connection::port` 里另有一个**同名但 2 字段**的 `CancelReceipt`（缺 `state`）。
//!   本模块**不**把它转出到 `registry` 命名空间；§7.6 形状以 [`receipt::CancelReceipt`] 为准。

pub mod actor;
pub mod audit;
pub mod backend;
pub mod epoch;
pub mod handles;
pub mod port;
pub mod receipt;
pub mod registry;

pub use actor::{spawn_actor, AuditOutbox, ControlCommand, ExecCommand, OpenRequest, SessionActor};

// 端口侧的取值一律**转出**而不是重定义：
// `CancelDisposition` 已在 `connection::port` 定型（三态 + 线上字面量），
// 在这里重定义一份，只会让同一个概念有两套 Rust 类型需要互相 cast。
#[doc(inline)]
pub use crate::connection::port::CancelDisposition;

// `SessionView` / `SessionHandle` 是会话面的**只读投影**，
// registry 转出它们是为了让 gateway 一个 import 就能拿到全套，
// 而不是让 gateway 再写一遍 `use crate::connection::{...}` 的清单。
#[doc(inline)]
pub use crate::connection::{SessionHandle, SessionView};

pub use audit::{AuditKind, AuditLog, CapabilityVersions, Outcome, RegistryAuditEntry};
pub use backend::SessionBackend;
pub use epoch::{fold_exit, ExitFact, ExitProjection, RuntimeEpoch};
pub use handles::{ExecutionBinding, HandleRegistry};
pub use port::SessionPort;
pub use receipt::CancelReceipt;
pub use registry::SessionRegistry;
