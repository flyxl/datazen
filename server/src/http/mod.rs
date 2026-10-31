//! HTTP 面：路由表、`ApiError` → HTTP 映射、中间件链。
//!
//! - [`router`]：v1 路由表的宿主。域路由按
//!   `docs/architecture/platform/http-api-contract.md`（WR-C2 冻结的契约面）
//!   随 WR-D2/D4/D5 逐域点亮；未点亮路径统一 501。
//! - [`response`]：跨边界 wire 类型 `ApiError`（`datazen::shared::error`，
//!   IPC 与 HTTP 共用同一份定义）的 HTTP 落地：状态码表 + JSON 体。
//! - [`middleware`]：请求中间件链，L0/L1 已上线，L2–L6 的落点已记录。

pub mod middleware;
pub mod response;
pub mod router;
