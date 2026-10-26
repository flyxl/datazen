//! AI vertical slice: provider contracts (domain), vendor adapters
//! (providers), the registry that ties them together, and the
//! provider-neutral business (application).
//!
//! Merged from the standalone `datazen-ai-providers` crate on 2026-10-26
//! (D1.5; the crate previously absorbed `datazen-ai-api`). The module owns the
//! **whole** provider stack:
//!
//! - [`domain`]: the `AiProvider` trait, request/response models, the
//!   link-time factory registration (mirrors `packages/driver-api`), plus the
//!   D6 business vocabulary (budget / ctx_yaml / safety gate rules);
//! - [`providers`]: one implementation per vendor (openai / anthropic /
//!   deepseek / ollama / custom), the shared wire [`providers::protocol`],
//!   the shared [`providers::http`] client, and the
//!   [`providers::registry::AiProviderRegistry`];
//! - [`application`]: the provider-neutral business — prompt resolution and
//!   response text handling (slice 3, 2026-10-26); chat orchestration, tool
//!   loop and session ownership migrate in the following slices.
//!
//! Hosts provide persistence and directories; this module has no dependency
//! on Tauri or any host.

pub mod application;
pub mod domain;
pub mod providers;

pub use application::prompt_resolver;
pub use application::response;
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
