//! Data Transfer pairing: SQL Direct + IR; cross-category forbidden.

use crate::transfer::pairing::{resolve_sync_pairing, SyncPairing};
use datazen_driver_api::normalize_driver_id;

use super::error::TransferError;
use super::model::TransferPairingView;

pub fn classify_transfer_pair(source: &str, target: &str) -> TransferPairingView {
    match resolve_transfer_pairing(source, target) {
        SyncPairing::Direct { family } => TransferPairingView {
            path: "direct".into(),
            supported: true,
            family: Some(family),
            reason: None,
        },
        SyncPairing::Ir => TransferPairingView {
            path: "ir".into(),
            supported: true,
            family: None,
            reason: None,
        },
        SyncPairing::Unsupported { reason } => TransferPairingView {
            path: "unsupported".into(),
            supported: false,
            family: None,
            reason: Some(reason),
        },
    }
}

pub fn enforce_transfer_pairing(source: &str, target: &str) -> Result<SyncPairing, TransferError> {
    match resolve_transfer_pairing(source, target) {
        SyncPairing::Unsupported { reason } => Err(TransferError::unsupported(reason)),
        ok @ (SyncPairing::Direct { .. } | SyncPairing::Ir) => Ok(ok),
    }
}

fn resolve_transfer_pairing(source: &str, target: &str) -> SyncPairing {
    let pairing = resolve_sync_pairing(source, target);
    if !matches!(pairing, SyncPairing::Unsupported { .. })
        && (normalize_driver_id(source) == "redis" || normalize_driver_id(target) == "redis")
    {
        return SyncPairing::Unsupported {
            reason: "Data Transfer has no Redis source and target adapters".into(),
        };
    }
    pairing
}

pub fn is_same_family(pairing: &SyncPairing) -> bool {
    matches!(pairing, SyncPairing::Direct { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_mysql_is_supported_ir() {
        let view = classify_transfer_pair("postgresql", "mysql");
        assert!(view.supported);
        assert_eq!(view.path, "ir");
    }

    #[test]
    fn pg_redis_is_unsupported() {
        let view = classify_transfer_pair("postgresql", "redis");
        assert!(!view.supported);
        assert_eq!(view.path, "unsupported");
    }

    #[test]
    fn same_mysql_family_is_direct() {
        let view = classify_transfer_pair("mysql", "mariadb");
        assert!(view.supported);
        assert_eq!(view.path, "direct");
        assert_eq!(view.family.as_deref(), Some("mysql"));
    }

    #[test]
    fn enforce_rejects_cross_category() {
        assert!(enforce_transfer_pairing("postgresql", "mongodb").is_err());
    }

    #[test]
    fn redis_is_rejected_before_adapter_resolution() {
        let view = classify_transfer_pair("redis", "redis");
        assert!(!view.supported);
        assert_eq!(view.path, "unsupported");
        assert!(view.reason.unwrap().contains("Redis"));
        assert!(enforce_transfer_pairing("redis", "redis").is_err());
        assert!(classify_transfer_pair("mongodb", "mongodb").supported);
    }
}
