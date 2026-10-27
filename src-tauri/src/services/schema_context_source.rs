//! Desktop wiring of the shared AI metadata source.
//!
//! The implementation lives in `datazen::services::schema_context`.
//! The desktop supplies only the two cross-form pieces it owns: the Runtime
//! facade (the session-to-connection identity) and the execution host (the
//! profile, the driver and command execution). Everything else — the metadata
//! commands, their parsing and the refusals — is shared.
use crate::platform::PlatformEntry;
use crate::services::runtime_command_source::RuntimeCommandSource;
use crate::store::DesktopRepository;
use datazen::ai::application::schema_context::{
    RuntimeSchemaContextSource, UnavailableSchemaContextSource,
};
use datazen::execution::application::RuntimeConnectionUseCases;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::services::ports::{LogicalSchemaContextSource, SessionConnectionLookup};
use datazen::shared::id::DbSessionId;
use std::sync::Arc;

/// 共享层要的「会话 → 连接」身份，由桌面用 Runtime 门面对上。
///
/// 这层薄壳是**依赖倒置的落点**：端口的 trait 定义在 `modules`（消费者），
/// 实现放在持有 Runtime 的宿主里，于是共享层再也不需要命名
/// `datazen::execution::application::RuntimeConnectionUseCases`。
struct RuntimeSessionLookup(Arc<RuntimeConnectionUseCases>);

impl SessionConnectionLookup for RuntimeSessionLookup {
    fn connection_id_for_session(&self, session_id: &str) -> Option<String> {
        self.0
            .connection_id_for_session(&DbSessionId::new(session_id))
            .map(|connection| connection.as_str().to_owned())
    }
}

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
            Arc::new(RuntimeSessionLookup(runtime)),
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
