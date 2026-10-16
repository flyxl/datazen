//! AI provider abstraction layer.
//!
//! Types and traits are re-exported from `datazen_ai_api` so the rest of the
//! crate can use `crate::ai::*`. Provider implementations and their wire
//! protocols moved to `packages/ai-providers`; this module re-exports them
//! so every existing `crate::ai::openai::…` style path keeps working.
//!
//! What stays here is the part that legitimately needs host services: schema context,
//! safety gating, prompt resolution, token budget and cancellation.

pub use datazen_ai_api::*;
pub use datazen_ai_providers::*;

pub mod budget;
pub mod cancel;
pub mod context;
pub mod ctx_yaml;
pub mod prompt_resolver;
pub mod safety;
pub mod schema_pipeline;

pub use cancel::CancellationRegistry;
pub use context::{prompt_db_type, SchemaContextBuilder};
pub use prompt_resolver::PromptResolver;
pub use schema_pipeline::{compose_schema_system_suffix, PromptSeed, SchemaContextPipeline};
