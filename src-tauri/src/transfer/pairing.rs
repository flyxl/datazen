//! Sync pairing policy: same dialect family → Direct; cross SQL → IR; cross category → forbidden.
//!
//! Category/family rules mirror frontend `src/lib/transferPairing.ts` for Transfer UI gating.

/// High-level driver category for sync pairing (broader than wire protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncCategory {
    Sql,
    Document,
    Kv,
    Other,
}

/// Resolved sync path for a source/target database type pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncPairing {
    Direct { family: String },
    Ir,
    Unsupported { reason: String },
}

impl SyncPairing {
    pub fn path_label(&self) -> &'static str {
        match self {
            Self::Direct { .. } => "direct",
            Self::Ir => "ir",
            Self::Unsupported { .. } => "unsupported",
        }
    }
}

/// Normalize a database type id to its sync dialect family (extends schema-diff aliases).
pub fn normalize_sync_family(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "cloudberry" | "questdb" => "postgresql".into(),
        "mysql" | "mariadb" | "tidb" | "oceanbase" | "doris" | "starrocks" | "manticore"
        | "ob_oracle" => "mysql".into(),
        "sqlite" | "rqlite" | "turso" => "sqlite".into(),
        "sqlserver" | "mssql" => "sqlserver".into(),
        "clickhouse" => "clickhouse".into(),
        "duckdb" => "duckdb".into(),
        "elasticsearch" => "elasticsearch".into(),
        "mongodb" => "mongodb".into(),
        "redis" => "redis".into(),
        "influxdb" => "influxdb".into(),
        "victoriametrics" => "victoriametrics".into(),
        "hbase" => "hbase".into(),
        "vector" => "vector".into(),
        "trino" | "presto" => "trino".into(),
        other => other.to_string(),
    }
}

/// Map a database type id to a sync category.
pub fn sync_category(raw: &str) -> SyncCategory {
    match raw.to_ascii_lowercase().as_str() {
        "redis" => SyncCategory::Kv,
        "mongodb" => SyncCategory::Document,
        "kiwi" | "superset" => SyncCategory::Other,
        _ => SyncCategory::Sql,
    }
}

/// Classify how a source/target pair should sync.
pub fn resolve_sync_pairing(source: &str, target: &str) -> SyncPairing {
    let src_cat = sync_category(source);
    let tgt_cat = sync_category(target);

    if src_cat != tgt_cat {
        return SyncPairing::Unsupported {
            reason: format!(
                "Sync between {} ({src_cat:?}) and {} ({tgt_cat:?}) is not supported",
                source, target
            ),
        };
    }

    match src_cat {
        SyncCategory::Other => SyncPairing::Unsupported {
            reason: format!("Sync is not supported for database type '{source}'"),
        },
        SyncCategory::Sql | SyncCategory::Document | SyncCategory::Kv => {
            let src_family = normalize_sync_family(source);
            let tgt_family = normalize_sync_family(target);
            if src_family == tgt_family {
                SyncPairing::Direct { family: src_family }
            } else if src_cat == SyncCategory::Sql {
                SyncPairing::Ir
            } else {
                SyncPairing::Unsupported {
                    reason: format!("Sync between {source} and {target} is not supported"),
                }
            }
        }
    }
}

/// Fail fast when a pair is forbidden; returns the resolved pairing otherwise.
pub fn enforce_sync_pairing(source: &str, target: &str) -> Result<SyncPairing, String> {
    match resolve_sync_pairing(source, target) {
        SyncPairing::Unsupported { reason } => Err(reason),
        ok @ (SyncPairing::Direct { .. } | SyncPairing::Ir) => Ok(ok),
    }
}

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
    fn other_category_is_unsupported() {
        assert!(matches!(
            resolve_sync_pairing("kiwi", "postgresql"),
            SyncPairing::Unsupported { .. }
        ));
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
}
