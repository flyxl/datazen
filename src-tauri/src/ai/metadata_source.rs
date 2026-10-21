//! Desktop session identity lookup bound to independent Runtime metadata reads.
use crate::{
    db::DriverRegistry, platform::PlatformEntry,
    services::runtime_command_source::RuntimeCommandSource, store::Store,
};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, TableInfo, TableSchema};
use datazen_feature_core::services::ports::SchemaContextSource;
use std::sync::Arc;
pub struct RuntimeSchemaSource {
    runtime: RuntimeCommandSource,
}
impl RuntimeSchemaSource {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>, platform: PlatformEntry) -> Self {
        Self {
            runtime: RuntimeCommandSource::new(registry, store, platform),
        }
    }
    pub(crate) async fn read(
        &self,
        session_id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.runtime
            .read_session_command(session_id, command, input)
            .await
    }
}
#[async_trait::async_trait]
impl SchemaContextSource for RuntimeSchemaSource {
    async fn get_session(
        &self,
        _id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        Err("The Runtime metadata source does not expose physical session handles".into())
    }
    async fn driver_for_session(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        let (owner, _, _) = self.runtime.profile_for_session(id).await?;
        self.runtime.driver_for_connection(&owner).await
    }
    async fn session_schema(&self, id: &str) -> Option<String> {
        self.runtime
            .profile_for_session(id)
            .await
            .ok()
            .and_then(|(_, config, _)| config.schema)
    }
    async fn tables(
        &self,
        id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String> {
        let output = self
            .read(
                id,
                "list_tables",
                serde_json::json!({ "database": database, "schema": schema }),
            )
            .await?;
        Ok(datazen_driver_api::parse_tables_from_command(&output))
    }
    async fn table_schema(
        &self,
        _cache: &datazen_runtime::schema_cache::SchemaCache,
        id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<TableSchema, String> {
        let output = self
            .read(
                id,
                "get_table_schema",
                serde_json::json!({ "database": database, "schema": schema, "table": table }),
            )
            .await?;
        serde_json::from_value(output["schema"].clone()).map_err(|error| error.to_string())
    }
}
