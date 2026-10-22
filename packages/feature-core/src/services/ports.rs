use datazen_driver_api::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, TableInfo, TableSchema,
};
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
/// The metadata a form-neutral consumer needs to describe a session's database.
///
/// Every method here is answered from the session's *identity* — which profile
/// owns it, and which metadata commands its connection answers — so a source
/// never has to own a live [`ConnectionHandle`]. The Runtime path addresses
/// sessions logically and implements this trait in full; it is the port every
/// metadata consumer depends on.
///
/// The trait carries **no** physical-shaped default implementation. A default
/// built on a connection handle could only ever be inherited by a source that
/// owns one, and the only source in the tree owns none — so the defaults were
/// unusable everywhere and had to be overridden one by one. The physical half
/// lives on [`PhysicalSchemaContextSource`], where it belongs.
#[async_trait::async_trait]
pub trait LogicalSchemaContextSource: Send + Sync {
    /// The driver that serves the session's connection.
    async fn driver_for_session(&self, session_id: &str)
        -> Result<Arc<dyn DatabaseDriver>, String>;
    /// The schema configured on the session's connection, if any.
    async fn session_schema(&self, session_id: &str) -> Option<String>;
    /// The tables of one `(database, schema)` scope.
    async fn tables(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String>;
    /// The full schema of one table.
    async fn table_schema(
        &self,
        session_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<TableSchema, String>;
}

/// The metadata source of a form that owns live connection handles.
///
/// This is the *physical* half of the metadata port: the source answers with
/// the driver's own handle, so metadata can be read straight against the
/// connection. The `ConnectionHandle`-based reads are not default methods of
/// this trait — they need the [`SchemaCache`](datazen_runtime::schema_cache::SchemaCache)
/// that memoizes them, and the cache belongs to the implementation that owns
/// the session, not to the caller of the port. They live in
/// [`PhysicalSchemaContextAdapter`](crate::services::physical_schema_context::PhysicalSchemaContextAdapter),
/// which presents a physical source through [`LogicalSchemaContextSource`].
///
/// A source that addresses sessions logically — the Runtime path — has no
/// handle to hand out, so it answers this contract with the refusal that
/// rejects physical-handle exposure. Its own reads live on
/// [`LogicalSchemaContextSource`], which is the port every consumer uses.
#[async_trait::async_trait]
pub trait PhysicalSchemaContextSource: Send + Sync {
    /// The live driver and physical handle of a session.
    async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String>;
    /// The schema configured on the session's connection, if any.
    ///
    /// The same fact the logical port calls `session_schema`, under a name of
    /// its own: a source that answers both contracts would otherwise carry two
    /// identically named methods, and every call on it would be ambiguous.
    async fn configured_schema(&self, session_id: &str) -> Option<String>;
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
