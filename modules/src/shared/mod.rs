//! Cross-module foundation vocabulary: IDs, request context, and error envelopes.
//!
//! Per the DDD refactor plan (§5.1), the IDs that form ownership contracts and
//! the trusted request context live in `shared` with a **single definition**;
//! feature modules re-export the types they own (e.g. `connection::domain`
//! re-exports `ConnectionId`). `shared` never depends on a feature module,
//! never hosts business repositories, and carries no business entities — only
//! identity newtypes, the adapter-built [`RequestContext`](context::RequestContext),
//! and the cross-boundary error envelopes
//! ([`PortError`](error::PortError) / [`ApiError`](error::ApiError)).
//!
//! Moved here in D2 (2026-10-26) from `datazen-modules::platform_api`, which now only
//! re-exports the same types until its retirement in D8.

pub mod clock;
pub mod context;
pub mod error;
/// FNV-1a 64 位指纹：Job 内核与连接运行时共用的纯算法（§5.3）。
pub mod hash;
pub mod migration;
/// 归属校验纯函数（`check_owner`）：共享身份词汇上的运算。
pub mod policy;
pub mod scope;
// `id` 用宏批量生成 ID newtype，宏调用点的 `///` 会被 rustc 判为未使用的文档注释
//（rustdoc 仍会在展开后的类型上生效），故在此局部放行。
#[allow(unused_doc_comments)]
pub mod id;

pub use context::{DelegationRef, OwnerRef, RequestContext};
pub use error::{ApiError, ApiErrorCode, PortError, RetryDisposition};
