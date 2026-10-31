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
//!   response text handling (slice 3, 2026-10-26); chat orchestration
//!   (chat_request / sql_generation / single_turn / filter / schema_doc /
//!   schema_pipeline / context), the tool loop (tools / tool_loop / db_tools
//!   + the DatabaseToolHost port), the cancellation registry, session
//!   ownership grants and the runtime-backed schema context source
//!   (item 4 closeout, 2026-10-26);
//!
//! Hosts provide persistence and directories; this module has no dependency
//! on Tauri or any host.

pub mod application;
pub mod domain;
pub mod providers;

// Compatibility-shaped flat exports for hosts that previously consumed the
// modules facade. The definitions remain owned by their DDD layer above.
pub use application::{
    cancel, chat_request, context, db_tools, filter, mcp_tools, ownership, schema_context,
    schema_doc, schema_pipeline, single_turn, sql_generation, tool_loop, tools,
};
pub use domain::{budget, ctx_yaml, safety};

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
