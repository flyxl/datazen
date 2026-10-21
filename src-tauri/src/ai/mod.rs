//! AI provider abstraction layer.
//!
//! Types and traits are re-exported from `datazen_ai_api` so the rest of the
//! crate can use `crate::ai::*`. Provider implementations and their wire
//! protocols moved to `packages/ai-providers`; this module re-exports them
//! so every existing `crate::ai::openai::…` style path keeps working.
//!
//! What stays here is the part that legitimately needs host services: schema context,
//! safety gating, prompt resolution, token budget and cancellation.
//!
//! Those portable rules now live in `packages/feature-core`
//! (`datazen_feature_core::ai::*`); the host keeps only the surface it names
//! through `crate::ai::*` plus the schema-metadata adapter below.

pub use datazen_ai_api::*;
pub use datazen_ai_providers::*;

pub use datazen_feature_core::ai::cancel::CancellationRegistry;
pub use datazen_feature_core::ai::context::{prompt_db_type, SchemaContextBuilder};
pub use datazen_feature_core::ai::prompt_resolver::PromptResolver;
pub use datazen_feature_core::ai::schema_pipeline::{
    compose_schema_system_suffix, PromptSeed, SchemaContextPipeline,
};

pub(crate) mod metadata_source;
