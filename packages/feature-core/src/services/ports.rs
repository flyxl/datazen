use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver};
use datazen_platform_api::settings::ExecutionSettings;
use std::sync::Arc;
#[async_trait::async_trait]
pub trait DatabaseToolHost: Send + Sync {
    async fn get_session_config(&self, session_id: &str) -> Result<ConnectionConfig, String>;
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        self.get_session_config(id).await
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String>;
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let _ = (id, command, input);
        Err("DatabaseToolHost does not support target execution".into())
    }
    async fn execution_settings(&self) -> ExecutionSettings;
}
#[async_trait::async_trait]
pub trait ConnectionCatalog: Send + Sync {
    async fn get_connections(&self) -> Vec<ConnectionConfig>;
}

/// Why a live session could not be bound for a foreign-key navigation probe.
///
/// This is not a navigation rule failure: the `grid.fk.*` codes cover the
/// rules, while a missing session is an ordinary host condition.
#[derive(Debug, Clone)]
pub enum FkSessionError {
    /// The session (or the connection profile behind it) is gone.
    NotFound(String),
    /// The host could not lend a driver/handle for the session.
    Unavailable(String),
}

impl std::fmt::Display for FkSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message) | Self::Unavailable(message) => f.write_str(message),
        }
    }
}

/// One live database session bound for a foreign-key navigation probe.
///
/// The host lends the driver and the handle for the duration of the probe and
/// keeps its own session lease alive through `guard`; the shared layer treats
/// the guard as an opaque token and never inspects it. Dropping the binding
/// releases the lease, so the binding must outlive every driver call made
/// with it.
pub struct FkSessionBinding {
    /// The driver of the bound session.
    pub driver: Arc<dyn DatabaseDriver>,
    /// The connection handle of the bound session.
    pub handle: ConnectionHandle,
    /// Database the connection profile is bound to; the fallback when the
    /// request names none.
    pub database: Option<String>,
    /// Schema the connection profile is bound to; the fallback when the
    /// request names none.
    pub schema: Option<String>,
    /// Opaque host guard that keeps the session lease alive.
    pub guard: Box<dyn std::any::Any + Send + Sync>,
}

/// Host capability for resolving a foreign-key navigation target.
///
/// Foreign-key navigation needs one live session for three reads — the source
/// table's foreign keys, the target table's metadata, and one existence probe
/// against the target table — so the port hands out a single binding instead of
/// separate driver lookups that could straddle two sessions.
#[async_trait::async_trait]
pub trait ForeignKeyNavigationSource: Send + Sync {
    /// Bind the live session behind `db_session_id`.
    async fn bind_session(&self, db_session_id: &str) -> Result<FkSessionBinding, FkSessionError>;
}
#[async_trait::async_trait]
pub trait SchemaContextSource: Send + Sync {
    async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String>;
    async fn session_schema(&self, session_id: &str) -> Option<String>;
    async fn driver_for_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn DatabaseDriver>, String> {
        Ok(self.get_session(session_id).await?.0)
    }
    async fn tables(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<datazen_driver_api::TableInfo>, String> {
        let (driver, handle) = self.get_session(session_id).await?;
        driver
            .get_tables(&handle, database, schema)
            .await
            .map_err(|error| error.to_string())
    }
    async fn table_schema(
        &self,
        cache: &datazen_runtime::schema_cache::SchemaCache,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<datazen_driver_api::TableSchema, String> {
        let (driver, handle) = self.get_session(session_id).await?;
        cache
            .get_table_schema(
                session_id,
                database,
                schema,
                table,
                &datazen_driver_api::LegacyCatalogReader::new(driver.as_ref()),
                &handle,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[async_trait::async_trait]
impl<T: DatabaseToolHost + ?Sized> DatabaseToolHost for Arc<T> {
    async fn get_session_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        (**self).get_session_config(id).await
    }
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        (**self).profile_config(id).await
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        (**self).driver_for_connection(id).await
    }
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        (**self).execute_target(id, command, input).await
    }
    async fn execution_settings(&self) -> ExecutionSettings {
        (**self).execution_settings().await
    }
}
#[async_trait::async_trait]
impl<T: ConnectionCatalog + ?Sized> ConnectionCatalog for Arc<T> {
    async fn get_connections(&self) -> Vec<ConnectionConfig> {
        (**self).get_connections().await
    }
}
