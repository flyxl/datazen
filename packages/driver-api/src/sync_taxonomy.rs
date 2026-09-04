//! Sync / transfer pairing taxonomy — category and dialect family per driver type.

use crate::{create_driver, SyncCategory};

/// Resolve a connection `database_type` id to a registered driver id for lookup.
pub fn normalize_driver_id(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "postgres" => "postgresql".into(),
        "mssql" => "sqlserver".into(),
        "presto" => "trino".into(),
        "tidb" | "oceanbase" => "mysql".into(),
        other => other.to_string(),
    }
}

/// Sync category for a database type id (uses registered driver when available).
pub fn sync_category_of(raw: &str) -> SyncCategory {
    let id = normalize_driver_id(raw);
    if let Some(driver) = create_driver(&id) {
        return driver.sync_category();
    }
    sync_category_unregistered(&id)
}

/// Sync dialect family for a database type id (uses registered driver when available).
pub fn sync_family_of(raw: &str) -> String {
    let id = normalize_driver_id(raw);
    if let Some(driver) = create_driver(&id) {
        return driver.sync_family();
    }
    id
}

fn sync_category_unregistered(id: &str) -> SyncCategory {
    match id {
        "redis" => SyncCategory::Kv,
        "mongodb" => SyncCategory::Document,
        "kiwi" | "superset" => SyncCategory::Other,
        _ => SyncCategory::Sql,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_normalize_before_lookup() {
        assert_eq!(normalize_driver_id("Postgres"), "postgresql");
        assert_eq!(normalize_driver_id("MSSQL"), "sqlserver");
        assert_eq!(normalize_driver_id("presto"), "trino");
        assert_eq!(normalize_driver_id("TiDB"), "mysql");
    }

    #[test]
    fn unregistered_kiwi_is_other() {
        assert_eq!(sync_category_of("kiwi"), SyncCategory::Other);
    }

    #[test]
    fn unregistered_trino_family_is_trino() {
        assert_eq!(sync_family_of("trino"), "trino");
        assert_eq!(sync_family_of("presto"), "trino");
    }

    #[test]
    fn test_tester_unregistered_sync_categories() {
        assert_eq!(sync_category_of("redis"), SyncCategory::Kv);
        assert_eq!(sync_category_of("mongodb"), SyncCategory::Document);
        assert_eq!(sync_category_of("superset"), SyncCategory::Other);
        assert_eq!(sync_category_of("unknown_engine"), SyncCategory::Sql);
    }

    #[test]
    fn test_tester_oceanbase_alias_normalizes_to_mysql_id() {
        assert_eq!(normalize_driver_id("oceanbase"), "mysql");
        assert_eq!(sync_family_of("oceanbase"), "mysql");
        assert_eq!(sync_family_of("tidb"), "mysql");
    }

    #[test]
    fn test_tester_unknown_id_passthrough() {
        assert_eq!(normalize_driver_id("ClickHouse"), "clickhouse");
        assert_eq!(sync_family_of("clickhouse"), "clickhouse");
    }
}
