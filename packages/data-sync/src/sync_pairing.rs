//! Compatibility path for the shared migration endpoint pairing policy.

pub use datazen_migration_common::pairing::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_postgresql_family_is_direct() {
        for pair in [
            ("postgresql", "postgresql"),
            ("postgresql", "cloudberry"),
            ("questdb", "postgresql"),
        ] {
            let p = resolve_sync_pairing(pair.0, pair.1);
            assert!(
                matches!(p, SyncPairing::Direct { ref family } if family == "postgresql"),
                "{pair:?} => {p:?}"
            );
        }
    }

    #[test]
    fn same_mysql_family_is_direct() {
        let p = resolve_sync_pairing("mysql", "mariadb");
        assert!(matches!(p, SyncPairing::Direct { ref family } if family == "mysql"));
    }

    #[test]
    fn cross_sql_dialect_uses_ir() {
        for pair in [
            ("postgresql", "mysql"),
            ("mysql", "postgresql"),
            ("sqlite", "postgresql"),
            ("clickhouse", "duckdb"),
        ] {
            assert_eq!(
                resolve_sync_pairing(pair.0, pair.1),
                SyncPairing::Ir,
                "{pair:?}"
            );
        }
    }

    #[test]
    fn same_document_and_kv_are_direct() {
        assert!(matches!(
            resolve_sync_pairing("mongodb", "mongodb"),
            SyncPairing::Direct { ref family } if family == "mongodb"
        ));
        assert!(matches!(
            resolve_sync_pairing("redis", "redis"),
            SyncPairing::Direct { ref family } if family == "redis"
        ));
    }

    #[test]
    fn cross_category_is_unsupported() {
        for pair in [
            ("postgresql", "mongodb"),
            ("mongodb", "redis"),
            ("redis", "mysql"),
        ] {
            assert!(
                matches!(
                    resolve_sync_pairing(pair.0, pair.1),
                    SyncPairing::Unsupported { .. }
                ),
                "{pair:?}"
            );
        }
    }

    #[test]
    fn kiwi_reclassified_as_sql_is_supported() {
        // Kiwi was re-classified as a SQL driver, so pairing it with another
        // SQL driver is supported (IR for a different dialect family) rather
        // than rejected as a cross-category / "other" pairing.
        assert!(
            matches!(
                resolve_sync_pairing("kiwi", "postgresql"),
                SyncPairing::Ir | SyncPairing::Direct { .. }
            ),
            "kiwi is SQL-category now; got {:?}",
            resolve_sync_pairing("kiwi", "postgresql")
        );
    }

    #[test]
    fn enforce_rejects_unsupported() {
        assert!(enforce_sync_pairing("postgresql", "redis").is_err());
    }

    #[test]
    fn pg_mysql_ir_still_allowed() {
        assert_eq!(
            enforce_sync_pairing("postgresql", "mysql").unwrap(),
            SyncPairing::Ir
        );
    }

    #[test]
    fn test_tester_alias_types_use_normalized_family() {
        assert!(matches!(
            resolve_sync_pairing("tidb", "mysql"),
            SyncPairing::Direct { ref family } if family == "mysql"
        ));
        assert!(matches!(
            resolve_sync_pairing("oceanbase", "mariadb"),
            SyncPairing::Direct { ref family } if family == "mysql"
        ));
    }
}
