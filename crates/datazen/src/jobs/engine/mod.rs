//! jobs 引擎（D5）：JobRuntime 编排与进程内参考仓储。
//! 只做状态推进；不认识具体迁移功能，不做方言与 IPC。

pub mod memory_repository;
pub mod memory_runtime_repository;
pub mod runtime;
