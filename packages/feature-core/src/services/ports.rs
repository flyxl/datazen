use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver};
use datazen_platform_api::settings::ExecutionSettings;
use std::sync::Arc;
#[async_trait::async_trait]
pub trait DatabaseToolHost: Send + Sync {
    async fn resolve_session_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), String>;
    async fn get_session_config(&self, session_id: &str) -> Result<ConnectionConfig, String>;
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        let (session, _, _) = self.resolve_session_for_connection(id).await?;
        self.get_session_config(&session).await
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        Ok(self.resolve_session_for_connection(id).await?.1)
    }
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let (_, driver, handle) = self.resolve_session_for_connection(id).await?;
        driver
            .execute_command(&handle, command, input)
            .await
            .map_err(|error| error.to_string())
    }
    async fn execution_settings(&self) -> ExecutionSettings;
}
#[async_trait::async_trait]
pub trait ConnectionCatalog: Send + Sync {
    async fn get_connections(&self) -> Vec<ConnectionConfig>;
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
            .get_table_schema(session_id, database, schema, table, &driver, &handle)
            .await
            .map_err(|error| error.to_string())
    }
}

#[async_trait::async_trait]
impl<T: DatabaseToolHost + ?Sized> DatabaseToolHost for Arc<T> {
    async fn resolve_session_for_connection(
        &self,
        id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        (**self).resolve_session_for_connection(id).await
    }
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
