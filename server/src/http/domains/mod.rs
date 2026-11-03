//! 域路由（WR-C2 契约面 §6 的逐域 HTTP 薄绑定）。
//!
//! 每个域文件自包含：域状态结构（composition root 注入）+ `pub fn
//! xxx_router(state) -> Router` + handler。handler 只做「解包 → 调
//! application 用例 → 映射 wire」三步（WR-A2 薄壳裁决的 HTTP 半边）；
//! 编排归 modules application，路由文件零业务规则。
//!
//! 复用 `crate::http::router` 的共享助手（`SessionIdentity` 提取器 +
//! 错误映射），错误 wire 与状态码全仓一处定义。

pub mod connections;
pub mod identity;
pub mod sessions;
pub mod settings;
