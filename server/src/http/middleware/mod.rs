//! 请求中间件链（team-server §3.1 的进入顺序）。
//!
//! 完整目标链（外 → 内）与各层的落地切片：
//!
//! | 层 | 职责 | 状态 |
//! | --- | --- | --- |
//! | L0 [`request_id`] | 分配/透传 `requestId`，注入扩展 | **已上线（D1）** |
//! | L1a [`body_limit`] | `Content-Length` 显式长度闸（413 + `ApiError` 体） | **已上线（D1）** |
//! | L1b 请求体流式闸 | 无显式长度的流式体（tower-http） | **已上线（D1）** |
//! | L2 认证 | 登录会话 cookie / native 凭据 | WR-D3 |
//! | L3 会话参数 | 只提取 SessionHandle + runtimeEpoch | WR-D3 |
//! | L4 CSRF | 会话绑定令牌校验 | WR-D3 |
//! | L5 体量与速率 | 令牌桶（`ratelimit.rs`） | WR-D3 |
//! | L6 授权 | RBAC / 资源 ACL（`policy/`） | WR-D3 |
//! | 统一错误映射 | handler 错误 → `ApiError` | 状态码表已上线（`http::response`），错误中间件随 D3 |
//! | 审计 | `Audit` 仓储落账 | WR-D2 |
//!
//! L2–L6 落地时在 [`router::build_router`](crate::http::router::build_router)
//! 的装配点按上表顺序插入，本模块文档同步更新「状态」列——链顺序是
//! §3.1 的裁决，不是实现细节。
//!
//! panic 安全（CatchPanicLayer）与 L1b 由 tower-http 提供，装配在
//! `build_router` 里；本目录只放自研层。

pub mod body_limit;
pub mod request_id;
