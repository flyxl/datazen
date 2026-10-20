//! Physical node identity for one database on one server.

use super::*;

impl MysqlDriver {
    pub(crate) async fn physical_database_node_identity(
        &self,
        pool: &MySqlPool,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        let driver = if self.is_mariadb { "mariadb" } else { "mysql" };
        if let Ok(row) = sqlx::query("SELECT @@server_uuid AS server_uuid")
            .fetch_one(pool)
            .await
        {
            if let Ok(server_uuid) = row.try_get::<String, _>("server_uuid") {
                let server_uuid = server_uuid.trim();
                if !server_uuid.is_empty() {
                    return Ok(Some(canonical_database_identity(
                        driver,
                        "node",
                        server_uuid,
                        database,
                    )));
                }
            }
        }

        let Ok(row) = sqlx::query(
            "SELECT @@hostname AS server_host, CAST(@@port AS CHAR) AS server_port, CAST(@@server_id AS CHAR) AS server_id",
        )
        .fetch_one(pool)
        .await
        else {
            return Ok(None);
        };
        let (Ok(server_host), Ok(server_port), Ok(server_id)) = (
            row.try_get::<String, _>("server_host"),
            row.try_get::<String, _>("server_port"),
            row.try_get::<String, _>("server_id"),
        ) else {
            return Ok(None);
        };
        let id = serde_json::json!({
            "host": server_host,
            "port": server_port,
            "serverId": server_id,
        })
        .to_string();
        Ok(Some(canonical_database_identity(
            driver, "node", &id, database,
        )))
    }
}
