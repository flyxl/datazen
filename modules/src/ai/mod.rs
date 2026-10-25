//! AI vertical slice: provider contracts (domain), vendor adapters
//! (providers), and the registry that ties them together.
//!
//! Merged from the standalone `datazen-ai-providers` crate on 2026-10-26
//! (D1.5; the crate previously absorbed `datazen-ai-api`). The module owns the
//! **whole** provider stack:
//!
//! - [`domain`]: the `AiProvider` trait, request/response models, and the
//!   link-time factory registration (mirrors `packages/driver-api`);
//! - [`providers`]: one implementation per vendor (openai / anthropic /
//!   deepseek / ollama / custom), the shared wire [`providers::protocol`],
//!   the shared [`providers::http`] client, and the
//!   [`providers::registry::AiProviderRegistry`].
//!
//! The provider-neutral business (chat orchestration, tool loop, safety
//! policy) migrates here in D6. Hosts provide persistence; this module has no
//! dependency on Tauri or any host.

pub mod domain;
pub mod providers;

pub use domain::factory::*;
pub use domain::traits::*;
pub use domain::types::*;

pub use providers::custom;
pub use providers::http;
pub use providers::metadata::provider_defaults;
pub use providers::protocol;
pub use providers::registry::{init_ai_providers, register_ai_providers, AiProviderRegistry};

pub const AI_PROTOCOL_VERSION: u32 = 1;

/// Minimum AI protocol version the host still supports.
///
/// Plugins with version < MIN will be rejected; those between MIN and current
/// will run in degraded mode (missing capabilities default to `false`).
pub const MIN_AI_PROTOCOL_VERSION: u32 = 1;

pub use async_trait::async_trait;
pub use inventory;
