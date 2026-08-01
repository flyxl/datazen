//! AI provider abstraction layer.
//!
//! Types and traits are re-exported from `datazen_ai_api` so the rest of the
//! crate can use `crate::ai::*`.

pub use datazen_ai_api::*;

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod registry;

pub use registry::{init_ai_providers, AiProviderRegistry};
