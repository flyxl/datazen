//! CommandJournal 与变化点断言（fake-runtime-fixtures.md §5）。
//!
//! 依赖方向：本模块**不依赖** `fake_resource` —— journal 记录由 provider 写入，断言由测试调用。
//! 依赖向下到 `clock`（时间戳）与非夹具的 `connection::{port, session, types, execution}`。
//!
//! §13 日志脱敏：journal 不记录凭据、附件令牌与幂等令牌 nonce；故障注入脚本的**脚本 id** 可记录，
//! 字面量不可记录。本模块因此没有任何可以接收 `Secret` 的入口。
//!
//! 按职责拆为四个子模块（单文件均远低于 800 行上限）：
//!
//! | 子模块 | 职责 |
//! | --- | --- |
//! | `entry.rs` | 条目类型（`ResourceEvent` / `HandleAction` / `JournalEntry` / `PermitEvent` / `HandleRecord`）与纯函数 |
//! | `core.rs` | 状态容器 `JournalState` 与全部 `record_*` 写入、读取器 |
//! | `asserts.rs` | §5.3 变化点规则与 §4.3 I1–I8 泄漏不变式 |
//! | `tests.rs` | 上述规则的自测 |

pub(crate) mod asserts;
pub(crate) mod core;
mod entry;
#[cfg(test)]
mod tests;

pub use asserts::JournalAssert;
pub use core::CommandJournal;
pub use entry::{HandleAction, HandleRecord, JournalEntry, PermitEvent, ResourceEvent};

// 测试沿用「`use super::*` 即可拿到上下文类型」的写法，这里集中再导出一次，
// 避免子模块各自重复一长串 use。
pub use super::clock::FakeClock;
pub use crate::connection::execution::{EffectOutcome, ExecutionErrorCode, TruncationRecord};
pub use crate::connection::port::{BudgetClass, PermitId, PermitReason};
pub use crate::connection::session::{HandleKind, SessionHandleRef};
pub use crate::connection::types::{
    ConfigRevision, ConnectionId, Counter, DbSessionId, ExecutionId, HandleId, LeaseId, OwnerRef,
    PoolKeyFingerprint, PoolKeyInputs, ResourceId, StreamId,
};
