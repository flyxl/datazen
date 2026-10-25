//! DataZen AI provider contracts and implementations.
//!
//! This crate owns the **whole** AI provider stack (merged from `datazen-ai-api`
//! on 2026-10-26; the merge into `datazen-modules::ai` follows in D1.5):
//!
//! - the provider contracts in [`traits`] / [`types`] / [`factory`] — the
//!   `AiProvider` trait, request/response models, and link-time factory
//!   registration (mirrors `packages/driver-api`);
//! - one [`AiProvider`] implementation per vendor ([`openai`], [`anthropic`],
//!   [`deepseek`], [`ollama`], [`custom`]);
//! - the shared wire protocols in [`protocol`], which every provider delegates to;
//! - the [`registry`] that resolves an `AiProviderType` to a concrete provider
//!   and restores a persisted provider config.
//!
//! It has no dependency on the Tauri host: persistence stays with the caller,
//! which loads the config and passes it to [`AiProviderRegistry::restore_config`].

pub const AI_PROTOCOL_VERSION: u32 = 1;

/// Minimum AI protocol version the host still supports.
///
/// Plugins with version < MIN will be rejected; those between MIN and current
/// will run in degraded mode (missing capabilities default to `false`).
pub const MIN_AI_PROTOCOL_VERSION: u32 = 1;

mod factory;
mod traits;
mod types;

pub use factory::*;
pub use traits::*;
pub use types::*;

pub mod anthropic;
pub mod custom;
pub mod deepseek;
pub mod http;
pub mod metadata;
pub mod ollama;
pub mod openai;
pub mod protocol;
pub mod registry;

pub use metadata::provider_defaults;
pub use registry::{init_ai_providers, register_ai_providers, AiProviderRegistry};

pub use async_trait::async_trait;
pub use inventory;
