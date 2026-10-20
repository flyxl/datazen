//! DataZen AI provider implementations and wire-protocol adapters.
//!
//! This crate owns the transport-facing half of the AI stack:
//!
//! - one [`AiProvider`] implementation per vendor ([`openai`], [`anthropic`],
//!   [`deepseek`], [`ollama`], [`custom`]);
//! - the shared wire protocols in [`protocol`], which every provider delegates to;
//! - the [`registry`] that resolves an `AiProviderType` to a concrete provider
//!   and restores a persisted provider config.
//!
//! The contracts themselves (`AiProvider`, request/response models, factory registry)
//! live in `datazen-ai-api`; this crate only implements them. It has no dependency on
//! the Tauri host: persistence stays with the caller, which loads the config and passes
//! it to [`AiProviderRegistry::restore_config`].

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
