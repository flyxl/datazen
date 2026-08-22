use datazen_driver_api::*;
use serde_json::json;
use sqlx::Row;

pub fn pg_admin_command_definitions() -> Vec<DriverCommandDefinition> {
    let mut cmds = vec![
        query_command_definition(),
        execute_command_definition(),
        query_stream_command_definition(),
    ];

    cmds.push(DriverCommandDefinition {
        id: "server_status_snapshot".into(),
        name: "Server Status Snapshot".into(),
        description: Some(
            "Read-only snapshot of server status, trend counters, and database size".into(),
        ),
        input_schema: json!({ "type": "object", "properties": {} }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Observe, CommandAccessLevel::Read),
    });

    cmds.push(DriverCommandDefinition {
        id: "estimate_table_rows".into(),
        name: "Estimate Table Rows".into(),
        description: Some("Cheap row estimate from pg_class statistics".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "table": { "type": "string", "description": "Table name" },
                "schema": { "type": "string", "description": "Schema name (defaults to public)" }
            },
            "required": ["table"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Observe, CommandAccessLevel::Read),
    });

    cmds.push(DriverCommandDefinition {
        id: "create_database".into(),
        name: "Create Database".into(),
        description: Some("Create a new PostgreSQL database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Database name" },
                "encoding": {
                    "type": "string",
                    "description": "Character encoding (e.g. UTF8)",
                    "examples": ["UTF8"]
                },
                "owner": {
                    "type": "string",
                    "description": "Database owner role",
                    "examples": ["postgres"]
                }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "create_schema".into(),
        name: "Create Schema".into(),
        description: Some("Create a new schema in the current database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Schema name" },
                "owner": {
                    "type": "string",
                    "description": "Schema owner role",
                    "examples": ["postgres"]
                }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "create_user".into(),
        name: "Create User".into(),
        description: Some("Create a new PostgreSQL user (role with LOGIN)".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Username" },
                "password": { "type": "string", "description": "Password" }
            },
            "required": ["username"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "grant_privileges".into(),
        name: "Grant Privileges".into(),
        description: Some("Grant privileges to a user on a database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Target user/role" },
                "database": { "type": "string", "description": "Target database (empty for all)" },
                "privileges": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Privileges to grant (e.g. SELECT, INSERT, UPDATE, DELETE, CREATE, ALL PRIVILEGES)"
                },
                "grantOption": { "type": "boolean", "description": "WITH GRANT OPTION" }
            },
            "required": ["username", "privileges"],
            "x-datazen": {
                "privilegeGroups": [
                    {
                        "label": "Table",
                        "privileges": [
                            "SELECT",
                            "INSERT",
                            "UPDATE",
                            "DELETE",
                            "TRUNCATE",
                            "REFERENCES",
                            "TRIGGER"
                        ]
                    },
                    {
                        "label": "Database",
                        "privileges": ["CONNECT", "CREATE", "TEMPORARY"]
                    }
                ]
            }
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "drop_user".into(),
        name: "Drop User".into(),
        description: Some("Drop a PostgreSQL user (role)".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Username to drop" }
            },
            "required": ["username"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "revoke_privileges".into(),
        name: "Revoke Privileges".into(),
        description: Some("Revoke privileges from a user".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "username": { "type": "string", "description": "Target user/role" },
                "database": { "type": "string", "description": "Target database (empty for current)" },
                "privileges": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Privileges to revoke"
                }
            },
            "required": ["username", "privileges"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "drop_schema".into(),
        name: "Drop Schema".into(),
        description: Some("Drop a schema from the current database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Schema name to drop" },
                "cascade": { "type": "boolean", "description": "CASCADE (drop dependent objects)" }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "drop_database".into(),
        name: "Drop Database".into(),
        description: Some("Drop a PostgreSQL database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Database name to drop" }
            },
            "required": ["name"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.push(DriverCommandDefinition {
        id: "list_processes".into(),
        name: "List Processes".into(),
        description: Some("List active server processes / sessions".into()),
        input_schema: json!({
            "type": "object",
            "properties": {},
            "x-datazen": {
                "columns": [
                    { "id": "pid", "name": "PID" },
                    { "id": "user", "name": "User" },
                    { "id": "database", "name": "Database" },
                    { "id": "client", "name": "Client" },
                    { "id": "state", "name": "State" },
                    { "id": "query", "name": "Query" },
                    { "id": "durationMs", "name": "Duration (ms)" }
                ]
            }
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Observe, CommandAccessLevel::Read),
    });

    cmds.push(DriverCommandDefinition {
        id: "kill_process".into(),
        name: "Kill Process".into(),
        description: Some("Cancel or terminate a backend process by PID".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pid": { "type": "integer", "description": "Backend PID" },
                "force": { "type": "boolean", "description": "Use pg_terminate_backend instead of pg_cancel_backend" }
            },
            "required": ["pid"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.extend(schema_object_command_definitions());

    cmds
}

const PG_DATABASE_PRIVILEGES: &[&str] = &["CONNECT", "CREATE", "TEMPORARY", "TEMP"];

fn is_database_privilege(p: &str) -> bool {
    PG_DATABASE_PRIVILEGES.contains(&p.to_uppercase().as_str())
}

pub fn build_admin_sql(command: &str, input: &serde_json::Value) -> Result<String, DriverError> {
    match command {
        "drop_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            Ok(format!("DROP DATABASE {}", quote_ident(name)))
        }
        "drop_schema" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let cascade = input["cascade"].as_bool().unwrap_or(false);
            let mut sql = format!("DROP SCHEMA {}", quote_ident(name));
            if cascade {
                sql.push_str(" CASCADE");
            }
            Ok(sql)
        }
        "create_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let mut sql = format!("CREATE DATABASE {}", quote_ident(name));
            if let Some(encoding) = input["encoding"].as_str() {
                sql.push_str(&format!(" ENCODING '{}'", encoding.replace('\'', "''")));
            }
            if let Some(owner) = input["owner"].as_str() {
                sql.push_str(&format!(" OWNER {}", quote_ident(owner)));
            }
            Ok(sql)
        }
        "create_schema" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let mut sql = format!("CREATE SCHEMA {}", quote_ident(name));
            if let Some(owner) = input["owner"].as_str() {
                sql.push_str(&format!(" AUTHORIZATION {}", quote_ident(owner)));
            }
            Ok(sql)
        }
        "create_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let mut sql = format!("CREATE ROLE {} WITH LOGIN", quote_ident(username));
            if let Some(password) = input["password"].as_str() {
                sql.push_str(&format!(" PASSWORD '{}'", password.replace('\'', "''")));
            }
            Ok(sql)
        }
        "drop_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let q = quote_ident(username);
            Ok(format!(
                "REASSIGN OWNED BY {q} TO CURRENT_USER; DROP OWNED BY {q}; DROP ROLE {q}"
            ))
        }
        "revoke_privileges" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let privileges = input["privileges"]
                .as_array()
                .ok_or_else(|| DriverError::InvalidConfig("privileges array is required".into()))?
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>();
            if privileges.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "at least one privilege is required".into(),
                ));
            }

            let database = input["database"].as_str().unwrap_or("");
            let db_privs: Vec<&str> = privileges
                .iter()
                .copied()
                .filter(|p| is_database_privilege(p))
                .collect();
            let tbl_privs: Vec<&str> = privileges
                .iter()
                .copied()
                .filter(|p| !is_database_privilege(p))
                .collect();

            let mut stmts = Vec::new();

            if !db_privs.is_empty() && !database.is_empty() {
                stmts.push(format!(
                    "REVOKE {} ON DATABASE {} FROM {}",
                    db_privs.join(", "),
                    quote_ident(database),
                    quote_ident(username),
                ));
            }

            if !tbl_privs.is_empty() {
                stmts.push(format!(
                    "REVOKE {} ON ALL TABLES IN SCHEMA public FROM {}",
                    tbl_privs.join(", "),
                    quote_ident(username),
                ));
            }

            if stmts.is_empty() {
                stmts.push(format!(
                    "REVOKE {} ON ALL TABLES IN SCHEMA public FROM {}",
                    privileges.join(", "),
                    quote_ident(username),
                ));
            }

            Ok(stmts.join("; "))
        }
        "grant_privileges" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let privileges = input["privileges"]
                .as_array()
                .ok_or_else(|| DriverError::InvalidConfig("privileges array is required".into()))?
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>();
            if privileges.is_empty() {
                return Err(DriverError::InvalidConfig(
                    "at least one privilege is required".into(),
                ));
            }

            let database = input["database"].as_str().unwrap_or("");
            let grant_option = input["grantOption"].as_bool().unwrap_or(false);
            let grant_suffix = if grant_option {
                " WITH GRANT OPTION"
            } else {
                ""
            };

            let db_privs: Vec<&str> = privileges
                .iter()
                .copied()
                .filter(|p| is_database_privilege(p))
                .collect();
            let tbl_privs: Vec<&str> = privileges
                .iter()
                .copied()
                .filter(|p| !is_database_privilege(p))
                .collect();

            let mut stmts = Vec::new();

            if !db_privs.is_empty() && !database.is_empty() {
                stmts.push(format!(
                    "GRANT {} ON DATABASE {} TO {}{}",
                    db_privs.join(", "),
                    quote_ident(database),
                    quote_ident(username),
                    grant_suffix,
                ));
            }

            if !tbl_privs.is_empty() {
                stmts.push(format!(
                    "GRANT {} ON ALL TABLES IN SCHEMA public TO {}{}",
                    tbl_privs.join(", "),
                    quote_ident(username),
                    grant_suffix,
                ));
            }

            if stmts.is_empty() {
                if !db_privs.is_empty() {
                    stmts.push(format!(
                        "GRANT {} ON ALL TABLES IN SCHEMA public TO {}{}",
                        db_privs.join(", "),
                        quote_ident(username),
                        grant_suffix,
                    ));
                } else {
                    return Err(DriverError::InvalidConfig(
                        "no applicable privileges".into(),
                    ));
                }
            }

            Ok(stmts.join("; "))
        }
        _ => Err(DriverError::Unsupported(format!(
            "unsupported admin command: {command}"
        ))),
    }
}

pub async fn execute_pg_admin_command(
    pool: &sqlx::PgPool,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match command {
        "server_status_snapshot" => fetch_pg_server_status(pool).await,
        "estimate_table_rows" => estimate_pg_table_rows(pool, &input).await,
        "list_processes" => fetch_pg_process_list(pool).await,
        "kill_process" => kill_pg_process(pool, &input).await,
        _ => execute_pg_admin_sql(pool, command, input).await,
    }
}

async fn fetch_pg_process_list(pool: &sqlx::PgPool) -> Result<CommandResult, DriverError> {
    let rows = sqlx::query(
        r#"
        SELECT
            pid,
            usename AS "user",
            datname AS database,
            state,
            LEFT(query, 500) AS query,
            COALESCE(EXTRACT(EPOCH FROM (now() - query_start)) * 1000, 0)::bigint AS "durationMs",
            CASE
                WHEN client_addr IS NOT NULL THEN client_addr::text
                WHEN btrim(application_name) IS NOT NULL AND btrim(application_name) <> '' THEN application_name
                ELSE '[local]'
            END AS client
        FROM pg_stat_activity
        WHERE pid <> pg_backend_pid()
        ORDER BY query_start NULLS LAST
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let processes: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "pid": row.get::<i32, _>("pid"),
                "user": row.try_get::<String, _>("user").ok(),
                "database": row.try_get::<String, _>("database").ok(),
                "state": row.try_get::<String, _>("state").ok(),
                "query": row.try_get::<String, _>("query").ok(),
                "durationMs": row.try_get::<i64, _>("durationMs").unwrap_or(0),
                "client": row.try_get::<String, _>("client").ok(),
            })
        })
        .collect();

    Ok(CommandResult {
        data: json!({ "processes": processes }),
    })
}

async fn kill_pg_process(
    pool: &sqlx::PgPool,
    input: &serde_json::Value,
) -> Result<CommandResult, DriverError> {
    use sqlx::Executor;
    let pid = input["pid"]
        .as_i64()
        .ok_or_else(|| DriverError::InvalidConfig("pid is required".into()))?;
    let force = input["force"].as_bool().unwrap_or(false);
    let fn_name = if force {
        "pg_terminate_backend"
    } else {
        "pg_cancel_backend"
    };
    let sql = format!("SELECT {fn_name}({pid})");
    pool.execute(sql.as_str())
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    Ok(CommandResult {
        data: json!({ "ok": true }),
    })
}

async fn fetch_pg_server_status(pool: &sqlx::PgPool) -> Result<CommandResult, DriverError> {
    let max_connections: i64 = sqlx::query_scalar("SELECT current_setting('max_connections')::int")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let row = sqlx::query(
        r#"
        SELECT
            version() AS version,
            current_database() AS database,
            EXTRACT(EPOCH FROM (now() - pg_postmaster_start_time()))::bigint AS uptime_seconds,
            (SELECT count(*)::bigint FROM pg_stat_activity) AS connections,
            (SELECT count(*)::bigint FROM pg_stat_activity WHERE state = 'active' AND pid <> pg_backend_pid()) AS active_queries,
            pg_size_pretty(pg_database_size(current_database())) AS database_size
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    // 累积计数器与瞬时指标：供 Host 差分/播种折线图并渲染数据卡片（数据驱动，无则隐藏）。
    let counters = sqlx::query(
        r#"
        SELECT
            xact_commit::bigint AS xact_commit,
            xact_rollback::bigint AS xact_rollback,
            (xact_commit + xact_rollback)::bigint AS questions,
            tup_fetched::bigint AS tup_fetched,
            tup_returned::bigint AS tup_returned,
            tup_inserted::bigint AS tup_inserted,
            tup_updated::bigint AS tup_updated,
            tup_deleted::bigint AS tup_deleted,
            blks_read::bigint AS blks_read,
            blks_hit::bigint AS blks_hit,
            deadlocks::bigint AS deadlocks,
            temp_files::bigint AS temp_files,
            temp_bytes::bigint AS temp_bytes,
            (blks_read * 8192)::bigint AS bytes_in,
            (blks_hit * 8192)::bigint AS bytes_out
        FROM pg_stat_database
        WHERE datname = current_database()
        "#,
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let xact_commit: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("xact_commit"))
        .unwrap_or(0);
    let xact_rollback: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("xact_rollback"))
        .unwrap_or(0);
    let questions: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("questions"))
        .unwrap_or(0);
    let tup_fetched: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("tup_fetched"))
        .unwrap_or(0);
    let tup_returned: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("tup_returned"))
        .unwrap_or(0);
    let tup_inserted: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("tup_inserted"))
        .unwrap_or(0);
    let tup_updated: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("tup_updated"))
        .unwrap_or(0);
    let tup_deleted: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("tup_deleted"))
        .unwrap_or(0);
    let blks_read: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("blks_read"))
        .unwrap_or(0);
    let blks_hit: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("blks_hit"))
        .unwrap_or(0);
    let deadlocks: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("deadlocks"))
        .unwrap_or(0);
    let temp_files: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("temp_files"))
        .unwrap_or(0);
    let temp_bytes: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("temp_bytes"))
        .unwrap_or(0);
    let bytes_in: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("bytes_in"))
        .unwrap_or(0);
    let bytes_out: i64 = counters
        .as_ref()
        .map(|r| r.get::<i64, _>("bytes_out"))
        .unwrap_or(0);

    // 缓存命中率（百分比）：blks_hit / (blks_hit + blks_read)。
    let cache_hit: Option<f64> = {
        let total = blks_hit + blks_read;
        if total > 0 {
            Some((blks_hit as f64 / total as f64) * 100.0)
        } else {
            None
        }
    };
    let cache_hit_ratio: Option<String> = cache_hit.map(|v| format!("{v:.2}%"));

    // 当前 WAL 字节数（累积）：pg_current_wal_lsn − '0/0'。无该函数（旧 PG < 10）时为 None。
    let wal_bytes: Option<i64> =
        sqlx::query_scalar("SELECT pg_wal_lsn_diff(pg_current_wal_lsn(), '0/0')::bigint")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    // 会话状态计数（瞬时 gauge，供「服务器会话」图：总/活跃/空闲）。
    let session_stats = sqlx::query(
        r#"
        SELECT
            count(*)::int AS total,
            count(*) FILTER (WHERE state = 'active')::int AS active,
            count(*) FILTER (WHERE state = 'idle')::int AS idle
        FROM pg_stat_activity
        "#,
    )
    .fetch_one(pool)
    .await
    .ok();
    let (session_total, session_active, session_idle) = match session_stats {
        Some(r) => (
            r.get::<i32, _>("total"),
            r.get::<i32, _>("active"),
            r.get::<i32, _>("idle"),
        ),
        None => (-1, -1, -1),
    };

    // PG14+ `sessions` 为累积会话数；旧版本无该列时省略字段（对应趋势卡自动隐藏）。
    let new_sessions: Option<i64> = sqlx::query_scalar(
        "SELECT sessions::bigint FROM pg_stat_database WHERE datname = current_database()",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let uptime_seconds = row.get::<i64, _>("uptime_seconds");
    let qps = if uptime_seconds > 0 {
        format!("{:.2}", questions as f64 / uptime_seconds as f64)
    } else {
        "0.00".to_string()
    };
    let txs_per_sec = if uptime_seconds > 0 {
        format!("{:.2}", questions as f64 / uptime_seconds as f64)
    } else {
        "0.00".to_string()
    };

    // PG 状态变量：由驱动决定提供哪些设置，Host 仅渲染返回数组（数据驱动分叉）。
    let pg_settings = sqlx::query(
        "SELECT name, setting FROM pg_settings WHERE unit IS NOT NULL OR source = 'configuration file' ORDER BY name LIMIT 150",
    )
    .fetch_all(pool)
    .await;
    let status_variables: Vec<serde_json::Value> = match pg_settings {
        Ok(rows) => rows
            .iter()
            .map(|r| {
                json!({
                    "name": r.get::<String, _>("name"),
                    "value": r.try_get::<String, _>("setting").ok(),
                })
            })
            .collect(),
        Err(_) => vec![],
    };

    let statement_counters = json!({
        "select": tup_fetched,
        "insert": tup_inserted,
        "update": tup_updated,
        "delete": tup_deleted,
    });

    let mut data = json!({
        "version": row.get::<String, _>("version"),
        "database": row.get::<String, _>("database"),
        "uptimeSeconds": uptime_seconds,
        "connections": row.get::<i64, _>("connections"),
        "maxConnections": max_connections,
        "activeQueries": row.get::<i64, _>("active_queries"),
        "databaseSize": row.get::<String, _>("database_size"),
        "qps": qps,
        "txsPerSec": txs_per_sec,
        "networkIn": bytes_in,
        "networkOut": bytes_out,
        "deadlocks": deadlocks,
        "tempFiles": temp_files,
        "tempBytes": temp_bytes,
        "sessionTotal": session_total,
        "sessionActive": session_active,
        "sessionIdle": session_idle,
        "questionsCounter": questions,
        "xactCommitCounter": xact_commit,
        "xactRollbackCounter": xact_rollback,
        "blksReadCounter": blks_read,
        "blksHitCounter": blks_hit,
        "tupFetchedCounter": tup_fetched,
        "tupReturnedCounter": tup_returned,
        "tupInsertedCounter": tup_inserted,
        "tupUpdatedCounter": tup_updated,
        "tupDeletedCounter": tup_deleted,
        "tempBytesCounter": temp_bytes,
        "bytesInCounter": bytes_in,
        "bytesOutCounter": bytes_out,
        "statementCounters": statement_counters,
        "statusVariables": status_variables,
    });
    if let Some(hit) = cache_hit_ratio {
        data["cacheHitRatio"] = json!(hit);
    }
    if let Some(wal) = wal_bytes {
        data["walBytes"] = json!(wal);
    }
    if let Some(sessions) = new_sessions {
        data["newSessionsCounter"] = json!(sessions);
    }

    Ok(CommandResult { data })
}

async fn estimate_pg_table_rows(
    pool: &sqlx::PgPool,
    input: &serde_json::Value,
) -> Result<CommandResult, DriverError> {
    let table = input["table"]
        .as_str()
        .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
    let schema = input["schema"].as_str().unwrap_or("public");

    let row = sqlx::query(
        r#"
        SELECT COALESCE(c.reltuples, 0)::bigint AS estimated_rows
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'r'
          AND c.relname = $1
          AND n.nspname = $2
        "#,
    )
    .bind(table)
    .bind(schema)
    .fetch_optional(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let estimated_rows = row.map(|r| r.get::<i64, _>("estimated_rows")).unwrap_or(0);

    Ok(CommandResult {
        data: json!({ "estimatedRows": estimated_rows }),
    })
}

async fn execute_pg_admin_sql(
    pool: &sqlx::PgPool,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    use sqlx::Executor;
    let sql = build_admin_sql(command, &input)?;
    for stmt in sql.split("; ") {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        pool.execute(s)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    }
    Ok(CommandResult {
        data: json!({ "ok": true }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definitions_include_admin_commands() {
        let defs = pg_admin_command_definitions();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"query"));
        assert!(ids.contains(&"execute"));
        assert!(ids.contains(&"query_stream"));
        assert!(ids.contains(&"server_status_snapshot"));
        assert!(ids.contains(&"estimate_table_rows"));
        assert!(ids.contains(&"create_database"));
        assert!(ids.contains(&"create_schema"));
        assert!(ids.contains(&"create_user"));
        assert!(ids.contains(&"list_objects"));
        assert!(ids.contains(&"get_object_ddl"));
        assert!(ids.contains(&"list_privileges"));
        assert!(ids.contains(&"list_processes"));
        assert!(ids.contains(&"kill_process"));
    }

    #[test]
    fn create_database_basic() {
        let input = json!({ "name": "testdb" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, r#"CREATE DATABASE "testdb""#);
    }

    #[test]
    fn create_database_with_encoding_and_owner() {
        let input = json!({ "name": "mydb", "encoding": "UTF8", "owner": "admin" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(
            sql,
            r#"CREATE DATABASE "mydb" ENCODING 'UTF8' OWNER "admin""#
        );
    }

    #[test]
    fn create_database_escapes_quotes() {
        let input = json!({ "name": "test\"db" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, r#"CREATE DATABASE "test""db""#);
    }

    #[test]
    fn create_schema_basic() {
        let input = json!({ "name": "analytics" });
        let sql = build_admin_sql("create_schema", &input).unwrap();
        assert_eq!(sql, r#"CREATE SCHEMA "analytics""#);
    }

    #[test]
    fn create_schema_with_owner() {
        let input = json!({ "name": "hr", "owner": "hr_admin" });
        let sql = build_admin_sql("create_schema", &input).unwrap();
        assert_eq!(sql, r#"CREATE SCHEMA "hr" AUTHORIZATION "hr_admin""#);
    }

    #[test]
    fn create_user_basic() {
        let input = json!({ "username": "reader" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, r#"CREATE ROLE "reader" WITH LOGIN"#);
    }

    #[test]
    fn create_user_with_password() {
        let input = json!({ "username": "writer", "password": "s3cret" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, r#"CREATE ROLE "writer" WITH LOGIN PASSWORD 's3cret'"#);
    }

    #[test]
    fn create_user_escapes_password_quotes() {
        let input = json!({ "username": "u", "password": "it's" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, r#"CREATE ROLE "u" WITH LOGIN PASSWORD 'it''s'"#);
    }

    #[test]
    fn missing_name_returns_error() {
        let input = json!({});
        assert!(build_admin_sql("create_database", &input).is_err());
        assert!(build_admin_sql("create_schema", &input).is_err());
    }

    #[test]
    fn missing_username_returns_error() {
        let input = json!({});
        assert!(build_admin_sql("create_user", &input).is_err());
    }

    #[test]
    fn drop_database_basic() {
        let input = json!({ "name": "testdb" });
        let sql = build_admin_sql("drop_database", &input).unwrap();
        assert_eq!(sql, r#"DROP DATABASE "testdb""#);
    }

    #[test]
    fn drop_schema_basic() {
        let input = json!({ "name": "analytics" });
        let sql = build_admin_sql("drop_schema", &input).unwrap();
        assert_eq!(sql, r#"DROP SCHEMA "analytics""#);
    }

    #[test]
    fn drop_schema_cascade() {
        let input = json!({ "name": "old_data", "cascade": true });
        let sql = build_admin_sql("drop_schema", &input).unwrap();
        assert_eq!(sql, r#"DROP SCHEMA "old_data" CASCADE"#);
    }

    #[test]
    fn grant_table_privileges_on_all_tables() {
        let input = json!({
            "username": "reader",
            "privileges": ["SELECT", "INSERT"],
        });
        let sql = build_admin_sql("grant_privileges", &input).unwrap();
        assert_eq!(
            sql,
            r#"GRANT SELECT, INSERT ON ALL TABLES IN SCHEMA public TO "reader""#
        );
    }

    #[test]
    fn grant_db_privileges_on_database() {
        let input = json!({
            "username": "admin",
            "database": "mydb",
            "privileges": ["CONNECT", "CREATE"],
        });
        let sql = build_admin_sql("grant_privileges", &input).unwrap();
        assert_eq!(
            sql,
            r#"GRANT CONNECT, CREATE ON DATABASE "mydb" TO "admin""#
        );
    }

    #[test]
    fn grant_mixed_privileges_splits_statements() {
        let input = json!({
            "username": "dev",
            "database": "mydb",
            "privileges": ["CONNECT", "SELECT", "INSERT"],
        });
        let sql = build_admin_sql("grant_privileges", &input).unwrap();
        assert!(sql.contains(r#"GRANT CONNECT ON DATABASE "mydb" TO "dev""#));
        assert!(sql.contains(r#"GRANT SELECT, INSERT ON ALL TABLES IN SCHEMA public TO "dev""#));
    }

    #[test]
    fn grant_with_grant_option() {
        let input = json!({
            "username": "admin",
            "privileges": ["SELECT"],
            "grantOption": true,
        });
        let sql = build_admin_sql("grant_privileges", &input).unwrap();
        assert!(sql.contains("WITH GRANT OPTION"));
    }

    #[test]
    fn unknown_command_returns_error() {
        let input = json!({ "name": "x" });
        assert!(build_admin_sql("some_unknown_cmd", &input).is_err());
    }

    /// 临场联调测试：连真实本地 PG（postgres/postgres），验证进程列表非空 + 状态变量返回。
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn live_process_list_and_status_on_local_pg() {
        let host = std::env::var("PGHOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port = std::env::var("PGPORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5432);
        let user = std::env::var("PGUSER").unwrap_or_else(|_| "postgres".into());
        let pass = std::env::var("PGPASSWORD").unwrap_or_else(|_| "postgres".into());
        let db = std::env::var("PGDATABASE").unwrap_or_else(|_| "postgres".into());
        let opts = sqlx::postgres::PgConnectOptions::new()
            .host(&host)
            .port(port)
            .username(&user)
            .password(&pass)
            .database(&db);
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skip live pg: {e}");
                return;
            }
        };

        let pl = fetch_pg_process_list(&pool).await;
        eprintln!("PG list_processes -> {pl:?}");
        let procs = pl.unwrap().data["processes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(!procs.is_empty(), "PG process list EMPTY — bug!");

        let ss = fetch_pg_server_status(&pool).await;
        let ss_data = ss.unwrap().data;
        let vars = ss_data["statusVariables"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        eprintln!("PG statusVariables count = {}", vars.len());
        assert!(
            ss_data.get("questionsCounter").is_some(),
            "PG snapshot must expose questionsCounter for trend charts, got {ss_data}"
        );
        assert!(
            ss_data.get("qps").is_some(),
            "PG snapshot must expose qps for dashboard cards, got {ss_data}"
        );
        // 扩展契约：卡片/图表面板依赖的字段应被返回（值依环境而定，仅校验存在）。
        for key in [
            "xactCommitCounter",
            "xactRollbackCounter",
            "blksReadCounter",
            "blksHitCounter",
            "tupFetchedCounter",
            "tupReturnedCounter",
            "tupInsertedCounter",
            "tupUpdatedCounter",
            "tupDeletedCounter",
            "deadlocks",
            "tempFiles",
        ] {
            assert!(
                ss_data.get(key).is_some(),
                "PG snapshot must expose {key} (extended dashboard contract), got {ss_data}"
            );
        }
        pool.close().await;
    }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
