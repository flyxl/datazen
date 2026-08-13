//! Data Sync pairing: Direct same-family only. IR / heterogeneous → Transfer.

use crate::sync::pairing::{normalize_sync_family, resolve_sync_pairing, SyncPairing};

use super::error::DataSyncError;

/// Families implemented for Data Synchronization V1 (P0).
pub const V1_FAMILIES: &[&str] = &["mysql", "postgresql"];

pub fn is_v1_family(family: &str) -> bool {
    V1_FAMILIES.contains(&family)
}

/// Require same dialect family and a V1-supported SQL family.
/// Cross-family SQL is Transfer, not Sync.
pub fn require_data_sync_family(
    source_db_type: &str,
    target_db_type: &str,
) -> Result<String, DataSyncError> {
    match resolve_sync_pairing(source_db_type, target_db_type) {
        SyncPairing::Direct { family } => {
            if is_v1_family(&family) {
                Ok(family)
            } else {
                Err(DataSyncError::incompatible(format!(
                    "Data Synchronization for family '{family}' is not available in V1"
                )))
            }
        }
        SyncPairing::Ir => Err(DataSyncError::incompatible(format!(
            "heterogeneous pair {} → {} is Data Transfer, not Data Synchronization",
            source_db_type, target_db_type
        ))),
        SyncPairing::Unsupported { reason } => Err(DataSyncError::incompatible(reason)),
    }
}

pub fn family_of(db_type: &str) -> String {
    normalize_sync_family(db_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_mariadb_is_direct_mysql_family() {
        assert_eq!(
            require_data_sync_family("mysql", "mariadb").unwrap(),
            "mysql"
        );
        assert_eq!(
            require_data_sync_family("mariadb", "mysql").unwrap(),
            "mysql"
        );
    }

    #[test]
    fn postgres_aliases_are_direct() {
        assert_eq!(
            require_data_sync_family("postgresql", "postgres").unwrap(),
            "postgresql"
        );
        assert_eq!(family_of("cloudberry"), "postgresql");
    }

    #[test]
    fn pg_to_mysql_is_transfer_not_sync() {
        let err = require_data_sync_family("postgresql", "mysql").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Transfer"), "{msg}");
    }

    #[test]
    fn sqlite_same_family_not_v1() {
        let err = require_data_sync_family("sqlite", "sqlite").unwrap_err();
        assert!(err.to_string().contains("V1"));
    }

    #[test]
    fn redis_and_kiwi_rejected() {
        assert!(require_data_sync_family("redis", "redis").is_err());
        assert!(require_data_sync_family("kiwi", "postgresql").is_err());
        assert!(require_data_sync_family("mysql", "mongodb").is_err());
    }

    #[test]
    fn v1_family_helpers() {
        assert!(is_v1_family("mysql"));
        assert!(is_v1_family("postgresql"));
        assert!(!is_v1_family("sqlite"));
        assert_eq!(V1_FAMILIES.len(), 2);
    }
}
