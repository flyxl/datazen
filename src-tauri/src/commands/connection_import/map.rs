use crate::db::{ConnectionConfig, SslMode, SshTunnelConfig};
use uuid::Uuid;

/// Map foreign DB type ids to DataZen `database_type` strings.
/// Returns None when unsupported.
pub fn map_database_type(raw: &str) -> Option<&'static str> {
    let t = raw.trim().to_ascii_lowercase().replace('-', "_");
    Some(match t.as_str() {
        "postgresql" | "postgres" | "pg" => "postgresql",
        "mysql" => "mysql",
        "mariadb" => "mariadb",
        "sqlite" => "sqlite",
        "redis" => "redis",
        "mongodb" | "mongo" => "mongodb",
        "sqlserver" | "mssql" | "microsoft_sql_server" => "sqlserver",
        "clickhouse" => "clickhouse",
        "duckdb" => "duckdb",
        "elasticsearch" | "es" => "elasticsearch",
        "rqlite" => "rqlite",
        "turso" | "libsql" => "turso",
        "influxdb" | "influx" => "influxdb",
        "victoriametrics" => "victoriametrics",
        "hbase" => "hbase",
        "vector" | "qdrant" | "milvus" | "weaviate" | "chromadb" => "vector",
        "doris" => "doris",
        "starrocks" => "starrocks",
        "manticore" | "manticoresearch" => "manticore",
        "questdb" => "questdb",
        "cloudberry" => "cloudberry",
        "ob_oracle" | "oceanbase-oracle" | "oceanbase_oracle" => "ob_oracle",
        "kiwi" => "kiwi",
        "presto" | "prestosql" | "trino" | "olap" => "presto",
        "superset" => "superset",
        _ => return None,
    })
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn base_connection(
    id: Option<String>,
    name: String,
    database_type: &str,
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    username: Option<String>,
    password: Option<String>,
    color_tag: Option<String>,
    group: Option<String>,
    ssh_tunnel: Option<SshTunnelConfig>,
) -> ConnectionConfig {
    ConnectionConfig {
        id: id.filter(|s| !s.is_empty()).unwrap_or_else(new_id),
        name,
        database_type: database_type.to_string(),
        host,
        port,
        database,
        schema: None,
        username,
        password,
        ssl_mode: SslMode::default(),
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel,
        color_tag,
        group,
        last_connected_at: None,
        server_version: None,
        options: None,
    }
}
