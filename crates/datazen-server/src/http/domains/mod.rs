//! 域路由（WR-C2 契约面 §6 的逐域 HTTP 薄绑定）。
//!
//! 每个域文件自包含：域状态结构（composition root 注入）+ `pub fn
//! xxx_router(state) -> Router` + handler。handler 只做「解包 → 调
//! application 用例 → 映射 wire」三步（WR-A2 薄壳裁决的 HTTP 半边）；
//! 编排归 modules application，路由文件零业务规则。
//!
//! 复用 `crate::http::router` 的共享助手（`SessionIdentity` 提取器 +
//! 错误映射），错误 wire 与状态码全仓一处定义。

pub mod ai_host;
pub mod connections;
pub mod dashboard;
pub mod driver_gateway;
pub mod execution;
pub mod identity;
pub mod org_memory;
pub mod query_assets;
pub mod schema_diff;
pub mod security;
pub mod sessions;
pub mod settings;
/// Data Sync 域（W8 刀3e）：任务/档案资产、配对分类、迁移运行历史读面
/// 与 sync job 读回/取消（契约 §6.16）。
pub mod sync;
pub mod table_data;
/// tableData 域的活句柄面（W8 刀2）：FK 二/值搜索/origin 四读端点的
/// handler 与借出助手；路由仍由 `table_data.rs` 挂载，本文件只承载实现。
pub mod table_data_live;
/// Workflow 域（W8 刀3c）：定义管理、同步执行与运行历史（契约 §6.14）。
pub mod workflow;
