//! `SecretProvider` 迁移垫片（D3，2026-10-26）。
//!
//! 凭据读取端口（`SecretProvider` / `SecretPurpose` / `ResolvedCredential`）的
//! 唯一定义自 D3 起在 `datazen-modules::execution::application::secret`（§5.1：
//! 凭据接口归实际消费者的 application；`SecretRef` 只认 connection::domain 的
//! `ProfileRecord` 映射）。本模块只再导出，旧 import 路径不变；D8 退役本
//! crate 时一并删除。

pub use datazen_modules::execution::application::secret::{
    ResolvedCredential, SecretProvider, SecretPurpose,
};
