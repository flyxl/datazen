//! 传输中立的假资源夹具（fake-runtime-fixtures.md §2 模块表）。
//!
//! 门控：整棵夹具子树只在 `cfg(test)` 或 `feature = "test-harness"` 下存在。
//! 生产路径因此在编译期就看不到它，`src-tauri` 的 `datazen` crate 也不可能引用它（§2 隔离规则）。
//!
//! 依赖方向单向向下（§2）：
//! - `bench` → `fake_resource` → `commands` → `journal` / `barrier` / `ids` / `fixtures` / `clock`
//! - `journal` 不依赖 `fake_resource`（记录由 provider 写入，断言由测试调用）
//! - `clock` 不依赖任何其他夹具模块
//!
//! §13 纪律：夹具内不出现任何 `.env` / `.env.test` 读取路径，也不产生真实外部连接。

pub mod barrier;
pub mod bench;
pub mod clock;
pub mod commands;
pub mod fake_resource;
pub mod fixtures;
pub mod ids;
pub mod journal;

pub use barrier::{Barrier, DrainBarrier};
pub use bench::{cm60_harness, cm60_stress};
pub use clock::FakeClock;
pub use commands::session_handle_command_definitions;
pub use fake_resource::{FakeResource, FakeResourceProvider, FakeScript};
pub use fixtures::fixtures;
pub use ids::{FakeIds, FakeIdScope};
pub use journal::{CommandJournal, JournalAssert};

#[cfg(any(test, feature = "test-harness"))]
pub use harness::FakeHarness;
#[cfg(any(test, feature = "test-harness"))]
mod harness;