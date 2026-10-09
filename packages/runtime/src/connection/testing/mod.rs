//! 传输中立的假资源夹具。
//!
//! 门控：整棵夹具子树只在 `cfg(test)` 或 `feature = "test-harness"` 下存在。
//! 生产路径因此在编译期就看不到它，`src-tauri` 的 `datazen` crate 也不可能引用它。
//!
//! 依赖方向单向向下：
//! - `harness` → `fake_resource` → `commands` → `journal` / `barrier` / `ids` / `fixtures` / `clock`
//! - `journal` 不依赖 `fake_resource`（记录由 provider 写入，断言由测试调用）
//! - `clock` 不依赖任何其他夹具模块
//!
//! **模块表的两处差异**（均为有意为之，理由见 `harness/mod.rs` 模块说明）：
//! 1. 模块表里列了 `bench.rs`（压测入口），但基准**不得**与功能测试
//!    共用二进制入口。该基准已落在独立的 `src/bin/cm60-bench/`（bin target，
//!    不在本子树内），因此 `testing/` 下**永远不建** `bench.rs`。
//! 2. 设计的 DAG 把 `FakeHarness` 放在 `harness` 节点但模块表未列文件名；本实现按
//!    `harness/{mod,cm73,session_cmds,tests}.rs` 目录形态落点，与 `journal/`、`fake_resource/`
//!    的拆法一致。
//!
//! 纪律：夹具内不出现任何 `.env` / `.env.test` 读取路径，也不产生真实外部连接。

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
pub use ids::{FakeIdScope, FakeIds};
pub use journal::{CommandJournal, JournalAssert};

#[cfg(any(test, feature = "test-harness"))]
mod harness;

#[cfg(any(test, feature = "test-harness"))]
pub use harness::{EvictionRaceOutcome, EvictionRaceReport, FakeHarness};
