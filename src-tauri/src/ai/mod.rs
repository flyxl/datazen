//! AI provider abstraction layer.
//!
//! Types and traits are re-exported from `datazen_ai_api` so the rest of the
//! crate can use `crate::ai::*`.

pub use datazen_ai_api::*;

pub mod anthropic;
pub mod budget;
pub mod context;
pub mod custom;
pub mod deepseek;
pub mod ollama;
pub mod openai;
pub mod prompt_resolver;
pub mod protocol;
pub mod registry;
pub mod schema_pipeline;

pub use context::{prompt_db_type, SchemaContextBuilder};
pub use prompt_resolver::PromptResolver;
pub use registry::{init_ai_providers, register_ai_providers, AiProviderRegistry};
pub use schema_pipeline::{compose_schema_system_suffix, PromptSeed, SchemaContextPipeline};
