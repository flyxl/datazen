use crate::mysql::{decode_mysql_text, decode_mysql_text_opt};
use datazen_driver_api::*;
use serde_json::json;
use sqlx::Row;

pub fn mysql_admin_command_definitions() -> Vec<DriverCommandDefinition> {
    let mut cmds = vec![
        query_command_definition(),
        execute_command_definition(),
        query_stream_command_definition(),
    ];

    cmds.push(DriverCommandDefinition {
        id: "server_status_snapshot".into(),
        name: "Server Status Snapshot".into(),
        description: Some(
            "Read-only snapshot of server uptime, connections, and database size".into(),
        ),
        input_schema: json!({ "type": "object", "properties": {} }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Observe, CommandAccessLevel::Read),
    });

    cmds.push(DriverCommandDefinition {
        id: "estimate_table_rows".into(),
        name: "Estimate Table Rows".into(),
        description: Some("Cheap row estimate from information_schema.tables".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "table": { "type": "string", "description": "Table name" },
                "schema": { "type": "string", "description": "Database name (defaults to current)" }
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
        description: Some("Create a new MySQL database".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Database name" },
                "encoding": { "type": "string", "description": "Character set (e.g. utf8mb4)" }
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
        description: Some("Create a new MySQL user".into()),
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
                "username": { "type": "string", "description": "Target user" },
                "database": { "type": "string", "description": "Target database (empty for all)" },
                "privileges": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Privileges (e.g. SELECT, INSERT, UPDATE, DELETE, CREATE, ALL PRIVILEGES)"
                },
                "grantOption": { "type": "boolean", "description": "WITH GRANT OPTION" }
            },
            "required": ["username", "privileges"],
            "x-datazen": {
                "privilegeGroups": [
                    {
                        "label": "Privileges",
                        "privileges": [
                            "SELECT",
                            "INSERT",
                            "UPDATE",
                            "DELETE",
                            "CREATE",
                            "DROP",
                            "ALTER",
                            "INDEX"
                        ]
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
        description: Some("Drop a MySQL user".into()),
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
                "username": { "type": "string", "description": "Target user" },
                "database": { "type": "string", "description": "Target database (empty for all)" },
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
        id: "drop_database".into(),
        name: "Drop Database".into(),
        description: Some("Drop a MySQL database".into()),
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
        description: Some("Kill a connection or query by process ID".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pid": { "type": "integer", "description": "Process ID" },
                "force": { "type": "boolean", "description": "Use KILL CONNECTION instead of KILL QUERY" }
            },
            "required": ["pid"]
        }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Admin, CommandAccessLevel::HighRisk),
    });

    cmds.extend(schema_catalog_command_definitions());
    cmds.extend(schema_object_command_definitions());

    cmds
}

pub fn build_admin_sql(command: &str, input: &serde_json::Value) -> Result<String, DriverError> {
    match command {
        "drop_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            Ok(format!("DROP DATABASE `{}`", name.replace('`', "``")))
        }
        "create_database" => {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let mut sql = format!("CREATE DATABASE `{}`", name.replace('`', "``"));
            if let Some(encoding) = input["encoding"].as_str() {
                let safe = encoding
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>();
                if !safe.is_empty() {
                    sql.push_str(&format!(" CHARACTER SET {}", safe));
                }
            }
            Ok(sql)
        }
        "create_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            let password = input["password"].as_str().unwrap_or("");
            Ok(format!(
                "CREATE USER '{}'@'%' IDENTIFIED BY '{}'",
                username.replace('\'', "''"),
                password.replace('\'', "''")
            ))
        }
        "drop_user" => {
            let username = input["username"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
            Ok(format!("DROP USER '{}'@'%'", username.replace('\'', "''")))
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
            let priv_str = privileges.join(", ");
            let database = input["database"].as_str().unwrap_or("");
            let target = if database.is_empty() {
                "*.*".to_string()
            } else {
                format!("`{}`.* ", database.replace('`', "``"))
            };
            Ok(format!(
                "REVOKE {} ON {} FROM '{}'@'%'",
                priv_str,
                target,
                username.replace('\'', "''")
            ))
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
            let priv_str = privileges.join(", ");
            let database = input["database"].as_str().unwrap_or("");
            let target = if database.is_empty() {
                "*.*".to_string()
            } else {
                format!("`{}`.* ", database.replace('`', "``"))
            };
            let mut sql = format!(
                "GRANT {} ON {} TO '{}'@'%'",
                priv_str,
                target,
                username.replace('\'', "''")
            );
            if input["grantOption"].as_bool().unwrap_or(false) {
                sql.push_str(" WITH GRANT OPTION");
            }
            Ok(sql)
        }
        _ => Err(DriverError::Unsupported(format!(
            "unsupported admin command: {command}"
        ))),
    }
}

pub async fn execute_mysql_admin_command(
    pool: &sqlx::MySqlPool,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match command {
        "server_status_snapshot" => fetch_mysql_server_status(pool).await,
        "estimate_table_rows" => estimate_mysql_table_rows(pool, &input).await,
        "list_processes" => fetch_mysql_process_list(pool).await,
        "kill_process" => kill_mysql_process(pool, &input).await,
        _ => {
            let sql = build_admin_sql(command, &input)?;
            sqlx::query(&sql)
                .execute(pool)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            Ok(CommandResult {
                data: json!({ "ok": true }),
            })
        }
    }
}

async fn fetch_mysql_server_status(pool: &sqlx::MySqlPool) -> Result<CommandResult, DriverError> {
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let database: Option<String> = sqlx::query_scalar("SELECT DATABASE()")
        .fetch_one(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let status_rows = sqlx::query(
        "SHOW GLOBAL STATUS WHERE Variable_name IN ('Uptime', 'Threads_connected', 'Threads_running', 'Questions', 'Threads_created', 'Slow_queries', 'Bytes_received', 'Bytes_sent', 'Com_select', 'Com_insert', 'Com_update', 'Com_delete')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut uptime_seconds: i64 = 0;
    let mut connections: i64 = 0;
    let mut active_queries: i64 = 0;
    let mut questions: i64 = 0;
    let mut new_sessions: i64 = 0;
    let mut slow_queries: i64 = 0;
    let mut bytes_received: i64 = 0;
    let mut bytes_sent: i64 = 0;
    let mut com_select: i64 = 0;
    let mut com_insert: i64 = 0;
    let mut com_update: i64 = 0;
    let mut com_delete: i64 = 0;
    for row in status_rows {
        let name = decode_mysql_text(&row, "Variable_name");
        let value = decode_mysql_text(&row, "Value");
        let parsed = value.parse::<i64>().unwrap_or(0);
        match name.as_str() {
            "Uptime" => uptime_seconds = parsed,
            "Threads_connected" => connections = parsed,
            "Threads_running" => active_queries = parsed,
            "Questions" => questions = parsed,
            "Threads_created" => new_sessions = parsed,
            "Slow_queries" => slow_queries = parsed,
            "Bytes_received" => bytes_received = parsed,
            "Bytes_sent" => bytes_sent = parsed,
            "Com_select" => com_select = parsed,
            "Com_insert" => com_insert = parsed,
            "Com_update" => com_update = parsed,
            "Com_delete" => com_delete = parsed,
            _ => {}
        }
    }
    let qps = if uptime_seconds > 0 {
        format!("{:.2}", questions as f64 / uptime_seconds as f64)
    } else {
        "0.00".to_string()
    };

    let max_connections: i64 = sqlx::query_scalar("SELECT @@GLOBAL.max_connections")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let database_size_mb: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(ROUND(SUM(data_length + index_length) / 1024 / 1024, 2) AS DOUBLE) FROM information_schema.tables WHERE table_schema = DATABASE()",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let database_size = database_size_mb
        .map(|mb| format!("{mb} MB"))
        .unwrap_or_else(|| "0 MB".into());

    // 状态变量列表（MySQL 专属，供「状态变量」明细面板渲染）。
    // 由驱动决定提供哪些变量；Host 仅渲染返回的数组，不感知方言差异。
    // 返回完整列表（与其它 DB 管理软件一致，通常 ~500 项）；仅跳过 Rsa_public_key
    // 这一个超长二进制块，避免把仪表盘明细撑爆。
    let all_status = sqlx::query("SHOW GLOBAL STATUS")
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let mut status_variables: Vec<serde_json::Value> = Vec::new();
    for row in all_status {
        let name = decode_mysql_text(&row, "Variable_name");
        if name.eq_ignore_ascii_case("Rsa_public_key") {
            continue;
        }
        let value = decode_mysql_text(&row, "Value");
        status_variables.push(json!({ "name": name, "value": value }));
    }

    let statement_counters = json!({
        "select": com_select,
        "insert": com_insert,
        "update": com_update,
        "delete": com_delete,
    });

    // InnoDB 缓存命中率（百分比）：read_requests 命中 / 总读取请求。
    // 从 SHOW GLOBAL STATUS 中已读取的变量里取（缺省任一统计时隐藏该卡片/指标）。
    let innodb_read_requests: Option<i64> = sqlx::query_scalar::<_, String>(
        "SELECT VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'Innodb_buffer_pool_read_requests'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|s| s.parse::<i64>().ok());
    let innodb_reads: Option<i64> = sqlx::query_scalar::<_, String>(
        "SELECT VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'Innodb_buffer_pool_reads'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|s| s.parse::<i64>().ok());
    let mut innodb_hit_ratio: Option<String> = None;
    if let (Some(req), Some(read)) = (innodb_read_requests, innodb_reads) {
        let hit = req - read;
        if req > 0 {
            innodb_hit_ratio = Some(format!("{:.2}%", (hit as f64 / req as f64) * 100.0));
        }
    }

    let mut data = json!({
        "version": version,
        "database": database.unwrap_or_default(),
        "uptimeSeconds": uptime_seconds,
        "connections": connections,
        "maxConnections": max_connections,
        "activeQueries": active_queries,
        "databaseSize": database_size,
        "qps": qps,
        "slowQueries": slow_queries,
        "networkIn": bytes_received,
        "networkOut": bytes_sent,
        // 累积计数器：Host 跨刷新做差分得到每秒速率。
        "questionsCounter": questions,
        "newSessionsCounter": new_sessions,
        "bytesInCounter": bytes_received,
        "bytesOutCounter": bytes_sent,
        "statementCounters": statement_counters,
        "statusVariables": status_variables,
    });
    if let Some(hit) = innodb_hit_ratio {
        data["innodbHitRatio"] = json!(hit);
    }

    Ok(CommandResult { data })
}

async fn estimate_mysql_table_rows(
    pool: &sqlx::MySqlPool,
    input: &serde_json::Value,
) -> Result<CommandResult, DriverError> {
    let table = input["table"]
        .as_str()
        .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
    let schema = input["schema"].as_str();

    let estimated_rows: Option<i64> = if let Some(db) = schema {
        sqlx::query_scalar(
            "SELECT TABLE_ROWS FROM information_schema.tables WHERE table_schema = ? AND table_name = ?",
        )
        .bind(db)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?
    } else {
        sqlx::query_scalar(
            "SELECT TABLE_ROWS FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = ?",
        )
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?
    };

    Ok(CommandResult {
        data: json!({ "estimatedRows": estimated_rows.unwrap_or(0) }),
    })
}

async fn fetch_mysql_process_list(pool: &sqlx::MySqlPool) -> Result<CommandResult, DriverError> {
    let rows = sqlx::query(
        r#"
        SELECT
            CAST(ID AS SIGNED) AS pid,
            USER AS user,
            DB AS `database`,
            COMMAND AS state,
            LEFT(INFO, 500) AS query,
            CAST(TIME AS SIGNED) * 1000 AS durationMs,
            SUBSTRING_INDEX(HOST, ':', 1) AS client
        FROM information_schema.PROCESSLIST
        ORDER BY TIME DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let processes: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "pid": row.get::<i64, _>("pid"),
                "user": decode_mysql_text_opt(row, "user"),
                "database": decode_mysql_text_opt(row, "database"),
                "state": decode_mysql_text_opt(row, "state"),
                "query": decode_mysql_text_opt(row, "query"),
                "durationMs": row.try_get::<i64, _>("durationMs").unwrap_or(0),
                "client": decode_mysql_text_opt(row, "client"),
            })
        })
        .collect();

    Ok(CommandResult {
        data: json!({ "processes": processes }),
    })
}

async fn kill_mysql_process(
    pool: &sqlx::MySqlPool,
    input: &serde_json::Value,
) -> Result<CommandResult, DriverError> {
    let pid = input["pid"]
        .as_i64()
        .ok_or_else(|| DriverError::InvalidConfig("pid is required".into()))?;
    let force = input["force"].as_bool().unwrap_or(false);
    let sql = if force {
        format!("KILL {pid}")
    } else {
        format!("KILL QUERY {pid}")
    };
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    Ok(CommandResult {
        data: json!({ "ok": true }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definitions_include_admin_commands() {
        let defs = mysql_admin_command_definitions();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"query"));
        assert!(ids.contains(&"execute"));
        assert!(ids.contains(&"query_stream"));
        assert!(ids.contains(&"server_status_snapshot"));
        assert!(ids.contains(&"estimate_table_rows"));
        assert!(ids.contains(&"create_database"));
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
        assert_eq!(sql, "CREATE DATABASE `testdb`");
    }

    #[test]
    fn create_database_with_encoding() {
        let input = json!({ "name": "mydb", "encoding": "utf8mb4" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE `mydb` CHARACTER SET utf8mb4");
    }

    #[test]
    fn create_database_escapes_backticks() {
        let input = json!({ "name": "test`db" });
        let sql = build_admin_sql("create_database", &input).unwrap();
        assert_eq!(sql, "CREATE DATABASE `test``db`");
    }

    #[test]
    fn create_user_basic() {
        let input = json!({ "username": "reader" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, "CREATE USER 'reader'@'%' IDENTIFIED BY ''");
    }

    #[test]
    fn create_user_with_password() {
        let input = json!({ "username": "admin", "password": "s3cret" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, "CREATE USER 'admin'@'%' IDENTIFIED BY 's3cret'");
    }

    #[test]
    fn create_user_escapes_quotes() {
        let input = json!({ "username": "o'brien", "password": "it's" });
        let sql = build_admin_sql("create_user", &input).unwrap();
        assert_eq!(sql, "CREATE USER 'o''brien'@'%' IDENTIFIED BY 'it''s'");
    }

    #[test]
    fn missing_name_returns_error() {
        let input = json!({});
        assert!(build_admin_sql("create_database", &input).is_err());
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
        assert_eq!(sql, "DROP DATABASE `testdb`");
    }

    #[test]
    fn unknown_command_returns_error() {
        let input = json!({ "name": "x" });
        assert!(build_admin_sql("some_unknown_cmd", &input).is_err());
    }

    /// 临场联调测试：连真实本地 MySQL（root 空密码），验证进程列表非空 + 状态变量完整。
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn live_process_list_and_status_variables_on_local_mysql() {
        let host = std::env::var("MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port = std::env::var("MYSQL_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3306);
        let user = std::env::var("MYSQL_USER").unwrap_or_else(|_| "root".into());
        let pass = std::env::var("MYSQL_PASSWORD").unwrap_or_default();
        let db = std::env::var("MYSQL_DB").unwrap_or_else(|_| "mysql".into());
        let opts = sqlx::mysql::MySqlConnectOptions::new()
            .host(&host)
            .port(port)
            .username(&user)
            .password(&pass)
            .database(&db);
        let pool = match sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skip live mysql: {e}");
                return;
            }
        };

        let pl = fetch_mysql_process_list(&pool).await;
        eprintln!("MYSQL list_processes -> {pl:?}");
        let pl_data = pl.unwrap().data;
        let procs = pl_data["processes"].as_array().cloned().unwrap_or_default();
        assert!(
            !procs.is_empty(),
            "MYSQL process list is EMPTY — bug! got {pl_data}"
        );

        let ss = fetch_mysql_server_status(&pool).await;
        let ss_data = ss.unwrap().data;
        let vars = ss_data["statusVariables"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        eprintln!("MYSQL statusVariables count = {}", vars.len());
        assert!(
            vars.len() > 200,
            "expected full status vars, got {}",
            vars.len()
        );
        // 扩展契约：卡片/图表面板依赖的字段应被返回。
        for key in [
            "questionsCounter",
            "newSessionsCounter",
            "bytesInCounter",
            "bytesOutCounter",
            "slowQueries",
            "statementCounters",
        ] {
            assert!(
                ss_data.get(key).is_some(),
                "MySQL snapshot must expose {key} (extended dashboard contract), got {ss_data}"
            );
        }
        pool.close().await;
    }
}
