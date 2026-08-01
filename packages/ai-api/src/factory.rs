//! Factory trait and inventory registration for AI providers.
//!
//! Mirrors `packages/driver-api/src/factory.rs`:
//! - `AiProviderFactory` replaces `DatabaseDriverFactory`
//! - `register_ai_provider!` replaces `register_driver!`
//! - `iter_ai_provider_factories` replaces `iter_driver_factories`

use crate::AiProvider;
use crate::AI_PROTOCOL_VERSION;
use std::sync::Arc;

/// Factory that creates AI provider instances.
/// Plugin crates implement this and register via `register_ai_provider!`.
pub trait AiProviderFactory: Send + Sync + 'static {
    fn create(&self) -> Arc<dyn AiProvider>;
    fn provider_id(&self) -> &'static str;

    fn protocol_version(&self) -> u32 {
        AI_PROTOCOL_VERSION
    }
}

inventory::collect!(&'static dyn AiProviderFactory);

/// Register an AI provider factory at link time.
///
/// ```ignore
/// struct MyProviderFactory;
/// impl AiProviderFactory for MyProviderFactory { ... }
/// datazen_ai_api::register_ai_provider!(&MyProviderFactory);
/// ```
#[macro_export]
macro_rules! register_ai_provider {
    ($factory:expr) => {
        $crate::inventory::submit!($factory as &'static dyn $crate::AiProviderFactory);
    };
}

/// Iterate over all registered AI provider factories (populated at link time).
pub fn iter_ai_provider_factories() -> inventory::iter<&'static dyn AiProviderFactory> {
    inventory::iter::<&'static dyn AiProviderFactory>
}
