//! jobs 领域层（D5）：聚合词汇、规则与仓储端口。
//! Repository trait 在本层声明（§4.1 第 5 条），实现在宿主（桌面 app_db / 未来 Web）。

pub mod budget;
pub mod error;
pub mod job;
pub mod native_tool;
pub mod owner;
pub mod plan;
pub mod recovery;
pub mod repository;
pub mod runtime_repository;
pub mod time;
