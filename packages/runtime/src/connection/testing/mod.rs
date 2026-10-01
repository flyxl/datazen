//! 传输中立的假资源夹具（fake-runtime-fixtures.md §2 模块表）。
//!
//! 门控：整棵夹具子树只在 `cfg(test)` 或 `feature = "test-harness"` 下存在。
//! 生产路径因此在编译期就看不到它，`src-tauri` 的 `datazen` crate 也不可能引用它（§2 隔离规则）。
//!
//! 依赖方向单向向下（§2）：
//! - `harness` → `fake_resource` → `commands` → `journal` / `barrier` / `ids` / `fixtures` / `clock`
//! - `journal` 不依赖 `fake_resource`（记录由 provider 写入，断言由测试调用）
//! - `clock` 不依赖任何其他夹具模块
//!
//! **与 §2 模块表的两处差异**（均为有意为之，理由见 `harness/mod.rs` 模块说明）：
//! 1. §2 表里列了 `bench.rs`（CM-60 压测入口），但 §11.6 明确要求基准**不得**与功能测试
//!    共用二进制入口，且 §12 要求它落在独立的 `src/bin/cm60-bench`。两者都不是 P0 交付物，
//!    在 P0 阶段保留一个空壳 `bench.rs` 只会制造「已实现」的错觉，因此本阶段**不建该文件**。
//! 2. §2 的 DAG 把 `FakeHarness` 放在 `harness` 节点但模块表未列文件名；本实现按
//!    `harness/{mod,cm73,session_cmds,tests}.rs` 目录形态落点，与 `journal/`、`fake_resource/`
//!    的拆法一致。
//!
//! §13 纪律：夹具内不出现任何 `.env` / `.env.test` 读取路径，也不产生真实外部连接。

pub mod barrier;
pub mod clock;
pub mod commands;
pub mod fake_resource;
pub mod fixtures;
pub mod ids;
pub mod journal;

pub use barrier::{Barrier, DrainBarrier};
pub use clock::FakeClock;
pub use commands::session_handle_command_definitions;
pub use fake_resource::{FakeResource, FakeResourceProvider, FakeScript};
pub use fixtures::fixtures;
pub use ids::{FakeIds, FakeIdScope};
pub use journal::{CommandJournal, JournalAssert};

#[cfg(any(test, feature = "test-harness"))]
mod harness;

#[cfg(any(test, feature = "test-harness"))]
pub use harness::{EvictionRaceOutcome, EvictionRaceReport, FakeHarness};
