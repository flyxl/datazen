//! Desktop wiring of the shared AI metadata source.
//!
//! The implementation lives in `datazen_feature_core::services::schema_context`.
//! The desktop supplies only the two cross-form pieces it owns: the Runtime
//! facade (the session-to-connection identity) and the execution host (the
//! profile, the driver and command execution). Everything else — the metadata
//! commands, their parsing and the refusals — is shared.
use crate::platform::PlatformEntry;
use crate::services::runtime_command_source::RuntimeCommandSource;
use crate::store::DesktopRepository;
use datazen_feature_core::services::ports::LogicalSchemaContextSource;
use datazen_feature_core::services::schema_context::{
    RuntimeSchemaContextSource, UnavailableSchemaContextSource,
};
use datazen_runtime::driver_registry::DriverRegistry;
use std::sync::Arc;

/// Build the shared metadata source for the desktop.
///
/// A platform that failed to assemble has no Runtime facade, and therefore no
/// metadata to read. The source then reports that reason instead of pretending
/// the database is empty.
pub(crate) fn desktop_schema_context_source(
    registry: Arc<DriverRegistry>,
    store: Arc<DesktopRepository>,
    platform: PlatformEntry,
) -> Arc<dyn LogicalSchemaContextSource> {
    let runtime = platform.require().and_then(|adapter| adapter.runtime());
    match runtime {
        Ok(runtime) => Arc::new(RuntimeSchemaContextSource::new(
            runtime,
            Arc::new(RuntimeCommandSource::new(registry, store, platform)),
        )),
        Err(error) => {
            tracing::error!(
                code = ?error.code,
                "runtime unavailable for AI schema metadata"
            );
            Arc::new(UnavailableSchemaContextSource::new(error.to_string()))
        }
    }
}
