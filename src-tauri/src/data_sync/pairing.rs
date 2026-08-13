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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSyncPairingView {
    pub path: String,
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// UI/IPC view: only V1 Direct families are selectable for Data Sync.
pub fn classify_data_sync_pair(source_db_type: &str, target_db_type: &str) -> DataSyncPairingView {
    match resolve_sync_pairing(source_db_type, target_db_type) {
        SyncPairing::Direct { family } if is_v1_family(&family) => DataSyncPairingView {
            path: "direct".into(),
            supported: true,
            family: Some(family),
            reason: None,
        },
        SyncPairing::Direct { family } => DataSyncPairingView {
            path: "direct".into(),
            supported: false,
            family: Some(family.clone()),
            reason: Some(format!(
                "Data Synchronization for family '{family}' is not available in V1"
            )),
        },
        SyncPairing::Ir => DataSyncPairingView {
            path: "ir".into(),
            supported: false,
            family: None,
            reason: Some(format!(
                "heterogeneous pair {source_db_type} → {target_db_type} is Data Transfer, not Data Synchronization"
            )),
        },
        SyncPairing::Unsupported { reason } => DataSyncPairingView {
            path: "unsupported".into(),
            supported: false,
            family: None,
            reason: Some(reason),
        },
    }
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

    #[test]
    fn classify_view_marks_ir_and_sqlite_unsupported() {
        let ok = classify_data_sync_pair("mysql", "mariadb");
        assert!(ok.supported);
        assert_eq!(ok.path, "direct");
        assert_eq!(ok.family.as_deref(), Some("mysql"));

        let ir = classify_data_sync_pair("postgresql", "mysql");
        assert!(!ir.supported);
        assert_eq!(ir.path, "ir");
        assert!(ir.reason.as_deref().unwrap().contains("Transfer"));

        let sqlite = classify_data_sync_pair("sqlite", "sqlite");
        assert!(!sqlite.supported);
        assert_eq!(sqlite.path, "direct");

        let redis = classify_data_sync_pair("redis", "mysql");
        assert!(!redis.supported);
        assert_eq!(redis.path, "unsupported");
    }
}
