//! WR-F2：MCP Server web 形态（streamable HTTP 传输）。
//!
//! 分层与桌面形态（`apps/desktop/src-tauri/src/mcp`）一一对应：
//! * [`contract`]——对外契约面（工具清单/输入属性键/错误帮助文案），与桌面
//!   `tool_help.rs` 逐字对齐；golden 契约
//!   `apps/desktop/src-tauri/src/mcp/fixtures/mcp_external_contract.json`
//!   由 `tests/mcp_contract_parity.rs` 直接 include 断言两形态逐字节一致。
//! * [`wire`]——JSON-RPC 2.0 / MCP 线路词汇：错误码映射（rmcp `ErrorData`
//!   语义：invalid_params = -32602、internal_error = -32603）、工具错误帮助块。
//! * [`http`]——streamable HTTP 传输绑定（axum）：`POST /mcp` 收 JSON-RPC，
//!   initialize 颁发 `Mcp-Session-Id`，会话校验、DELETE 终止、GET 405。
//!   服务端身份 = L2 认证链的登录会话（组织隔离沿 ctx），经
//!   [`http::SessionAuthenticator`] 端口注入——接线归主线。
//! * [`runtime`]——modules application 面的装配（`StatefulSessionHost` 的
//!   server 侧实现 + 工具分发）。依赖 modules `mcp` feature，由
//!   `#[cfg(feature = "mcp")]` 门控：主线在 `server/Cargo.toml` 增加
//!   `mcp = ["datazen/mcp"]` feature 后点亮（见报告接线清单）。
//!
//! 本模块只提供 handler 级函数与 Router 片段（`http::mcp_router`），不接
//! `http::router`、不做 bootstrap 装配——接线归主线（WR-F2 报告附清单）。

pub mod contract;
pub mod http;
pub mod wire;

#[cfg(feature = "mcp")]
pub mod runtime;
