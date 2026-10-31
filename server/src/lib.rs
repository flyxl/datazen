//! `datazen-server` —— 团队 Web 服务宿主（P7 / web-ready WR-D）。
//!
//! 一个二进制、三个子命令（serve | migrate | check-schema，team-server
//! §2.4）。`bootstrap::serve` 是唯一的 composition root：任何模块都不得在其
//! 之外隐式构造全局单例（§2.3），失败策略 fail-closed。
//!
//! # 与桌面的关系：同一内核、两个宿主
//!
//! 桌面（`apps/desktop/src-tauri`，crate `datazen_desktop`）与本 crate 是
//! 平级的两个宿主：编排逻辑只存在于 `datazen`（modules）的 application 层，
//! 宿主只做「取 repository → 组 request → 调用例」的薄绑定。桌面绑 Tauri
//! IPC，本 crate 绑 HTTP；HTTP 面镜像的是 backend-client 域客户端面
//! （`docs/architecture/platform/http-api-contract.md`），不镜像 IPC 命令面。
//!
//! # 依赖闭包（§2.2，CI 强制）
//!
//! 允许：`datazen`（modules）、driver-api / ai-api、HTTP 框架、tokio、序列化。
//! 禁止：任何 `tauri*`、`wry` / `tao`、`datazen_desktop`、前端 UI runtime。
//! `scripts/check-server-deps.mjs` 在 CI 里跑 `cargo tree` 的闭包并拒绝上述
//! crate 出现。
//!
//! # WR-D1 骨架的边界（后续切片的落点）
//!
//! - 中间件链（§3.1）：L0 requestId 与 L1 请求体字节闸已上线；L2 认证、
//!   L3 会话参数、L4 CSRF、L5 速率、L6 授权随 WR-D3 落地，`http/middleware`
//!   的模块文档记录了完整顺序。
//! - `/api/v1` 域路由：按契约表随 WR-D2/D4/D5 逐域点亮；D1 骨架对未点亮
//!   路径统一回答 501 `capabilityUnsupported`（契约 §7 的映射裁决）。
//! - 迁移：`migrate` / `check-schema` 已随 WR-D2 第一切片落地（真实 expand
//!   迁移执行器 + 只读版本报告；落后退出码 3、超前退出码 4）。
//! - TLS 与 `:8443` 生产监听、UI origin 校验：随 WR-D3 的同源 HTTPS 落地；
//!   D1 默认只在回环地址监听明文。

pub mod bootstrap;
pub mod config;
pub mod error;
pub mod http;
pub mod logging;
pub mod storage;
