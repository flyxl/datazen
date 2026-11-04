//! MySQL connection options and pooled session helpers.

use datazen_driver_api::*;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlSslMode};
use sqlx::Row;
use std::time::Duration;

pub(crate) fn non_empty_secret(value: Option<&str>) -> Option<&str> {
    value.filter(|s| !s.trim().is_empty())
}

/// The socket MySQL dials when the config names no host. One constant feeds
/// both [`build_mysql_options`] and `DatabaseDriver::default_host`, so the host
/// can never compare endpoints against a value the driver does not dial.
pub(crate) const DEFAULT_HOST: &str = "localhost";

/// The port MySQL dials when the config names none. See [`DEFAULT_HOST`].
pub(crate) const DEFAULT_PORT: u16 = 3306;

pub(crate) fn build_mysql_options(
    config: &ConnectionConfig,
) -> Result<MySqlConnectOptions, DriverError> {
    let host = config.host.as_deref().unwrap_or(DEFAULT_HOST);
    let port = config.port.unwrap_or(DEFAULT_PORT);
    let mut opts = MySqlConnectOptions::new()
        .host(host)
        .port(port)
        .username(config.username.as_deref().unwrap_or("root"));
    if let Some(password) = non_empty_secret(config.password.as_deref()) {
        opts = opts.password(password);
    }
    if let Some(database) = config
        .database
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        opts = opts.database(database);
    }

    let mysql_ssl = match config.ssl_mode {
        SslMode::Disable => MySqlSslMode::Disabled,
        SslMode::Prefer => MySqlSslMode::Preferred,
        SslMode::Require => MySqlSslMode::Required,
        SslMode::VerifyCa => MySqlSslMode::VerifyCa,
        SslMode::VerifyFull => MySqlSslMode::VerifyIdentity,
    };
    opts = opts.ssl_mode(mysql_ssl);

    Ok(opts)
}

impl super::MysqlDriver {
    /// Build `USE \`db\`` with identifier quoting. Rejects empty / whitespace-only names.
    pub(crate) fn build_use_database_sql(database: &str) -> Result<String, DriverError> {
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
        Ok(format!("USE {}", Self::quote_identifier(trimmed)))
    }

    pub(crate) async fn current_database_on_conn(
        conn: &mut sqlx::pool::PoolConnection<sqlx::MySql>,
    ) -> Result<String, DriverError> {
        use sqlx::Executor;
        // Text protocol: prepared `SELECT DATABASE()` can return the schema from
        // PREPARE time, not the current default after a later USE.
        let row = (&mut **conn)
            .fetch_one("SELECT DATABASE()")
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        Ok(row.try_get::<String, _>(0).unwrap_or_default())
    }

    /// Execute SQL via the MySQL text protocol (COM_QUERY).
    ///
    /// `sqlx::query` always PREPARE's. MySQL rejects `BEGIN`/`COMMIT`/`ROLLBACK`,
    /// `USE`, `SET`, some DDL, and `CREATE PROCEDURE` on the prepared protocol
    /// with **1295 (HY000): This command is not supported in the prepared
    /// statement protocol yet**. Passing `&str` to [`sqlx::Executor::execute`]
    /// sends COM_QUERY (`Execute::take_arguments` is `None`).
    pub(crate) async fn execute_text_on_conn(
        conn: &mut sqlx::pool::PoolConnection<sqlx::MySql>,
        sql: &str,
    ) -> Result<sqlx::mysql::MySqlQueryResult, sqlx::Error> {
        use sqlx::Executor;
        (&mut **conn).execute(sql).await
    }

    /// Execute `USE \`db\`` via the MySQL text protocol (COM_QUERY).
    ///
    /// Also clears sqlx's statement cache: MySQL resolves unqualified table names
    /// at PREPARE time, so cached statements would keep hitting the previous DB.
    pub(crate) async fn execute_use_on_conn(
        conn: &mut sqlx::pool::PoolConnection<sqlx::MySql>,
        use_sql: &str,
        db: &str,
    ) -> Result<(), DriverError> {
        use sqlx::Connection;
        Self::execute_text_on_conn(conn, use_sql)
            .await
            .map_err(|e| DriverError::QueryFailed(format!("Failed to USE database `{db}`: {e}")))?;
        conn.clear_cached_statements().await.map_err(|e| {
            DriverError::QueryFailed(format!(
                "Failed to clear statement cache after USE `{db}`: {e}"
            ))
        })?;
        Ok(())
    }

    pub(crate) async fn open_pool(
        opts: MySqlConnectOptions,
        timeout: Duration,
        max_connections: u32,
        min_connections: u32,
    ) -> Result<MySqlPool, DriverError> {
        let mut builder = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(timeout);
        if min_connections > 0 {
            builder = builder.min_connections(min_connections);
        }
        builder
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))
    }

    /// Run `USE` for the handle's active database on a concrete connection.
    pub(crate) async fn apply_active_database(
        &self,
        handle: &ConnectionHandle,
        conn: &mut sqlx::pool::PoolConnection<sqlx::MySql>,
    ) -> Result<(), DriverError> {
        let db = self
            .active_databases
            .read()
            .await
            .get(&handle.pool_id)
            .cloned();
        let Some(db) = db else {
            return Ok(());
        };
        let current = Self::current_database_on_conn(conn).await?;
        if current == db {
            return Ok(());
        }
        let sql = Self::build_use_database_sql(&db)?;
        Self::execute_use_on_conn(conn, &sql, &db).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mysql::MysqlDriver;
    use datazen_driver_api::DatabaseDriver;

    fn config(host: Option<&str>, port: Option<u16>) -> ConnectionConfig {
        ConnectionConfig {
            id: "cfg".into(),
            name: "n".into(),
            database_type: "mysql".into(),
            host: host.map(str::to_string),
            port,
            database: Some("app".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: 5,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    /// 反漂移闸：`default_host()`/`default_port()` 是宿主做端点物理身份摘要时
    /// 唯一的「默认 host/port」来源。它一旦与 `build_mysql_options` 实际拨号的
    /// 值分家，省略 host 的连接与显式写全 host 的连接就会算出两个不同的
    /// service_key，自覆盖在 admission 静默漏判。所以这里不测常量本身，
    /// 只测「声明 == 实拨」。
    #[test]
    fn the_declared_defaults_are_exactly_what_connect_dials() {
        let driver = MysqlDriver::new(false);
        let declared_host = driver.default_host().expect("mysql has an implicit host");
        let declared_port = driver.default_port().expect("mysql has an implicit port");

        let dialled = build_mysql_options(&config(None, None)).expect("options build");
        assert_eq!(
            dialled.get_host(),
            declared_host,
            "default_host() must be the host connect actually dials"
        );
        assert_eq!(
            dialled.get_port(),
            declared_port,
            "default_port() must be the port connect actually dials"
        );

        assert_eq!(declared_host, DEFAULT_HOST);
        assert_eq!(declared_port, DEFAULT_PORT);

        let explicit =
            build_mysql_options(&config(Some("db-a.example.com"), Some(6432))).expect("options");
        assert_eq!(explicit.get_host(), "db-a.example.com");
        assert_eq!(explicit.get_port(), 6432);
    }
}
