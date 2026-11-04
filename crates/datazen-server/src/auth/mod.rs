//! WR-D3 余项：OIDC 登录流（RP 侧，授权码 + PKCE S256 + JWKS）。
//!
//! - [`oidc`]：发现文档缓存（TTL + issuer 精确匹配）、待定授权事务（内存，
//!   §4.6 先例口径：单进程 v1、TTL + 一次性消费、重启即失效）、code +
//!   verifier 到 token 端点交换、JWKS 按 `kid` 缓存轮换（未知 kid 触发
//!   一次刷新）、以及 `login` / `callback` 两个 axum handler——挂载点为
//!   契约 §5 的 `GET /api/v1/auth/login` 与 `GET /api/v1/auth/callback`
//!   （主线统一接线，见模块报告）。所有时长来自 [`oidc::OidcConfig`]
//!   构造注入，非协议常量（§4.1）。
//! - [`claims`]：id_token 的 nonce/azp/sub/iat 手工校验（§4.2 校验表的
//!   crate 之外收尾段）与 claims → membership 身份映射（不在登录时隐式
//!   开户）。
//!
//! 本模块骨架预置曾仅为并行开发的 mod 声明占位（主线统一接线），本刀
//! 填充完成。

pub mod claims;
pub mod oidc;

#[cfg(test)]
mod oidc_flow_tests;
