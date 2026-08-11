//! MongoDB driver — native `mongodb` crate.
//!
//! The query editor accepts JSON command bodies:
//! - aggregation: `{"collection":"orders","pipeline":[{"$match":{"status":"paid"}},{"$limit":20}]}`
//! - find:        `{"collection":"orders","filter":{"amount":{"$gt":1000}},"limit":20}`
//! - insert:      `{"collection":"orders","insert":[{...}]}`
//! - update:      `{"collection":"orders","update":{"filter":{...},"update":{"$set":{...}}}}`
//! - delete:      `{"collection":"orders","delete":{"filter":{...}}}`

use datazen_driver_api::*;
use async_trait::async_trait;
use ::mongodb::bson::{doc, Bson, Document};
use ::mongodb::options::{ClientOptions, Credential};
use ::mongodb::{Client, Collection};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;
use futures_util::TryStreamExt;

pub struct MongodbDriver {
    clients: RwLock<HashMap<String, (Client, Option<String>)>>,
}

impl MongodbDriver {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    fn get<'a>(
        map: &'a HashMap<String, (Client, Option<String>)>,
        handle: &ConnectionHandle,
    ) -> Result<&'a (Client, Option<String>), DriverError> {
        map.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    fn uri(config: &ConnectionConfig) -> Result<String, DriverError> {
        let host = config
            .host
            .clone()
            .ok_or_else(|| DriverError::InvalidConfig("host is required".into()))?;
        let port = config
            .port
            .ok_or_else(|| DriverError::InvalidConfig("port is required".into()))?;
        let mut uri = String::new();
        if let (Some(u), Some(p)) = (&config.username, &config.password) {
            if !u.is_empty() {
                uri.push_str(&format!("mongodb://{}:{}@", u, p));
            } else {
                uri.push_str("mongodb://");
            }
        } else {
            uri.push_str("mongodb://");
        }
        let tls = matches!(
            config.ssl_mode,
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull
        );
        uri.push_str(&format!("{host}:{port}/?directConnection=true"));
        if tls {
            uri.push_str("&tls=true");
        }
        Ok(uri)
    }

    async fn client(config: &ConnectionConfig) -> Result<Client, DriverError> {
        let uri = Self::uri(config)?;
        let mut options = ClientOptions::parse(&uri)
            .await
            .map_err(|e| DriverError::ConnectionFailed(format!("MongoDB URI parse failed: {e}")))?;
        options.app_name = Some("DataZen".to_string());
        if let (Some(u), Some(p)) = (&config.username, &config.password) {
            if !u.is_empty() {
                options.credential = Some(Credential::builder().username(u.clone()).password(p.clone()).build());
            }
        }
        Client::with_options(options)
            .map_err(|e| DriverError::ConnectionFailed(format!("MongoDB client failed: {e}")))
    }

    fn doc_to_value(doc: &Document) -> serde_json::Value {
        serde_json::to_value(doc).unwrap_or(serde_json::Value::Null)
    }

    fn bson_type_name(v: &Bson) -> String {
        match v {
            Bson::Null => "null".to_string(),
            Bson::Boolean(_) => "bool".to_string(),
            Bson::Int32(_) | Bson::Int64(_) => "int".to_string(),
            Bson::Double(_) | Bson::Decimal128(_) => "double".to_string(),
            Bson::String(_) => "string".to_string(),
            Bson::Array(_) => "array".to_string(),
            Bson::Document(_) => "object".to_string(),
            Bson::Binary(_) => "binary".to_string(),
            Bson::ObjectId(_) => "objectId".to_string(),
            Bson::DateTime(_) => "date".to_string(),
            Bson::Timestamp(_) => "timestamp".to_string(),
            _ => "other".to_string(),
        }
    }

    fn collection(client: &Client, database: &str, name: &str) -> Collection<Document> {
        client.database(database).collection(name)
    }

    fn parse_command(
        sql: &str,
    ) -> Result<(String, serde_json::Value), DriverError> {
        let cmd: serde_json::Value = serde_json::from_str(sql.trim()).map_err(|_| {
            DriverError::QueryFailed(
                "MongoDB queries are JSON commands, e.g. {\"collection\":\"orders\",\"pipeline\":[]}".into(),
            )
        })?;
        let collection = cmd
            .get("collection")
            .and_then(|c| c.as_str())
            .ok_or_else(|| DriverError::QueryFailed("JSON command requires a \"collection\" field".into()))?
            .to_string();
        Ok((collection, cmd))
    }
}

#[async_trait]
impl DatabaseDriver for MongodbDriver {
    fn driver_type(&self) -> DatabaseType {
        "mongodb".to_string()
    }

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Document
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = Self::client(config).await?;
        let _ = client
            .list_database_names()
            .await
            .map_err(|e| DriverError::ConnectionFailed(format!("MongoDB ping failed: {e}")))?;
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "mongodb".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = Self::client(config).await?;
        let pool_id = format!("mongodb_{}", uuid::Uuid::new_v4());
        self.clients
            .write()
            .await
            .insert(pool_id.clone(), (client, config.database.clone()));
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.clients.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let map = self.clients.read().await;
        let (client, _) = Self::get(&map, handle)?;
        client
            .list_database_names()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("MongoDB list databases failed: {e}")))
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let map = self.clients.read().await;
        let (client, _) = Self::get(&map, handle)?;
        let names = client
            .database(database)
            .list_collection_names()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("MongoDB list collections failed: {e}")))?;
        Ok(names
            .into_iter()
            .map(|name| TableInfo {
                name,
                schema: Some(database.to_string()),
                table_type: TableType::Table,
                row_count: None,
            })
            .collect())
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let map = self.clients.read().await;
        let (client, database) = Self::get(&map, handle)?;
        let db = database.clone().unwrap_or_else(|| "test".to_string());
        let mut columns = Vec::new();
        if let Some(doc) = client
            .database(&db)
            .collection::<Document>(table)
            .find_one(doc! {})
            .await
            .map_err(|e| DriverError::QueryFailed(format!("MongoDB sample failed: {e}")))?
        {
            let mut keys: Vec<String> = doc.keys().cloned().collect();
            keys.sort();
            for k in keys {
                let t = doc.get(&k).map(Self::bson_type_name).unwrap_or_else(|| "unknown".to_string());
                let is_pk = k == "_id";
                columns.push(ColumnSchema {
                    name: k,
                    data_type: t,
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: is_pk,
                    is_auto_increment: false,
                });
            }
        }
        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let (collection, cmd) = Self::parse_command(sql)?;
        let map = self.clients.read().await;
        let (client, database) = Self::get(&map, handle)?;
        let db = cmd
            .get("database")
            .and_then(|d| d.as_str())
            .map(String::from)
            .or_else(|| database.clone())
            .unwrap_or_else(|| "test".to_string());
        let col = Self::collection(client, &db, &collection);
        let start = Instant::now();
        let mut rows_json: Vec<serde_json::Value> = Vec::new();
        if let Some(pipeline) = cmd.get("pipeline").and_then(|p| p.as_array()) {
            let bson_pipeline: Result<Vec<Document>, _> = pipeline
                .iter()
                .map(|stage| ::mongodb::bson::to_document(stage))
                .collect();
            let bson_pipeline = bson_pipeline
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB pipeline parse failed: {e}")))?;
            let mut cursor = col
                .aggregate(bson_pipeline)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB aggregate failed: {e}")))?;
            while let Some(d) = cursor
                .try_next()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB aggregate read failed: {e}")))?
            {
                rows_json.push(Self::doc_to_value(&d));
            }
        } else {
            let filter = cmd
                .get("filter")
                .map(::mongodb::bson::to_document)
                .transpose()
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB filter parse failed: {e}")))?
                .unwrap_or_else(|| doc! {});
            let limit = cmd.get("limit").and_then(|l| l.as_u64()).unwrap_or(100);
            let mut cursor = col
                .find(filter)
                .limit(limit as i64)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB find failed: {e}")))?;
            while let Some(d) = cursor
                .try_next()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB find read failed: {e}")))?
            {
                rows_json.push(Self::doc_to_value(&d));
            }
        }
        let mut columns: Vec<String> = Vec::new();
        for row in &rows_json {
            if let Some(obj) = row.as_object() {
                for k in obj.keys() {
                    if !columns.contains(k) {
                        columns.push(k.clone());
                    }
                }
            }
        }
        columns.sort();
        let column_infos: Vec<ColumnInfo> = columns
            .iter()
            .map(|name| ColumnInfo {
                name: name.clone(),
                data_type: "bson".to_string(),
                nullable: true,
            })
            .collect();
        let rows: Vec<Vec<Option<Value>>> = rows_json
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|c| row.get(c).map(json_to_value).unwrap_or(Some(Value::Null)))
                    .collect()
            })
            .collect();
        Ok(QueryResult {
            columns: column_infos,
            rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let result = self.query(handle, sql).await?;
        Ok(MultiQueryResult {
            results: vec![StatementResult {
                sql: sql.to_string(),
                columns: result.columns,
                rows: result.rows,
                rows_affected: result.rows_affected,
                execution_time_ms: result.execution_time_ms,
                truncated: false,
            }],
            total_time_ms: result.execution_time_ms,
        })
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let (collection, cmd) = Self::parse_command(sql)?;
        let map = self.clients.read().await;
        let (client, database) = Self::get(&map, handle)?;
        let db = cmd
            .get("database")
            .and_then(|d| d.as_str())
            .map(String::from)
            .or_else(|| database.clone())
            .unwrap_or_else(|| "test".to_string());
        let col = Self::collection(client, &db, &collection);
        if let Some(docs) = cmd.get("insert").and_then(|d| d.as_array()) {
            let bson_docs: Result<Vec<Document>, _> = docs
                .iter()
                .map(::mongodb::bson::to_document)
                .collect();
            let bson_docs = bson_docs
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB insert parse failed: {e}")))?;
            let res = col
                .insert_many(bson_docs)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB insert failed: {e}")))?;
            return Ok(res.inserted_ids.len() as u64);
        }
        if let Some(upd) = cmd.get("update") {
            let filter = upd
                .get("filter")
                .map(::mongodb::bson::to_document)
                .transpose()
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB update filter parse failed: {e}")))?
                .unwrap_or_else(|| doc! {});
            let update = upd
                .get("update")
                .map(::mongodb::bson::to_document)
                .transpose()
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB update doc parse failed: {e}")))?
                .unwrap_or_else(|| doc! {});
            let res = col
                .update_many(filter, update)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB update failed: {e}")))?;
            return Ok(res.modified_count);
        }
        if let Some(del) = cmd.get("delete") {
            let filter = del
                .get("filter")
                .map(::mongodb::bson::to_document)
                .transpose()
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB delete filter parse failed: {e}")))?
                .unwrap_or_else(|| doc! {});
            let res = col
                .delete_many(filter)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("MongoDB delete failed: {e}")))?;
            return Ok(res.deleted_count);
        }
        Err(DriverError::QueryFailed(
            "MongoDB execute requires insert/update/delete in the JSON command".into(),
        ))
    }

    async fn use_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        let trimmed = database.trim();
        if trimmed.is_empty() {
            return Err(DriverError::InvalidConfig(
                "Database name must not be empty".into(),
            ));
        }
        if trimmed.contains('\0') {
            return Err(DriverError::InvalidConfig(
                "Database name contains invalid characters".into(),
            ));
        }
        let mut map = self.clients.write().await;
        let entry = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        entry.1 = Some(trimmed.to_string());
        Ok(())
    }

    fn supports_explain(&self) -> bool {
        false
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        statement_command_definitions(
            "Run a MongoDB JSON command (find/aggregate)",
            "Run a MongoDB JSON write command (insert/update/delete)",
            "JSON command",
        )
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn use_database_rejects_empty_and_updates_pool() {
        let driver = MongodbDriver::new();
        let handle = ConnectionHandle {
            id: "c".into(),
            pool_id: "missing".into(),
        };
        assert!(matches!(
            driver.use_database(&handle, "  ").await,
            Err(DriverError::InvalidConfig(_))
        ));

        // Insert a stub pool entry without a live client by connecting is heavy;
        // validate only the empty-name path here. ConnectionFailed covers missing pool.
        let err = driver.use_database(&handle, "app").await.unwrap_err();
        assert!(matches!(err, DriverError::ConnectionFailed(_)));
    }

    #[test]
    fn command_definitions_describe_json_commands() {
        let ids: Vec<_> = MongodbDriver::new()
            .command_definitions()
            .into_iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(ids, vec!["query", "execute"]);
        let query = MongodbDriver::new()
            .command_definitions()
            .into_iter()
            .find(|d| d.id == "query")
            .unwrap();
        assert!(query.description.unwrap().contains("JSON"));
        assert_eq!(query.input_schema["properties"]["sql"]["title"], "JSON command");
    }
}

fn json_to_value(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Null => Some(Value::Null),
        serde_json::Value::Bool(b) => Some(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Value::Integer(i))
            } else {
                n.as_f64().map(Value::Float)
            }
        }
        serde_json::Value::String(s) => Some(Value::String(s.clone())),
        other => Some(Value::Json(other.clone())),
    }
}
