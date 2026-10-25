//! Vendor adapters and shared transport machinery for the AI module.
//!
//! Each file implements [`crate::ai::domain::AiProvider`] for one vendor and
//! delegates wire formatting to [`protocol`]. The [`registry`] resolves an
//! `AiProviderType` to a concrete provider and restores persisted configs.

pub mod anthropic;
pub mod custom;
pub mod deepseek;
pub mod http;
pub mod metadata;
pub mod ollama;
pub mod openai;
pub mod protocol;
pub mod registry;
