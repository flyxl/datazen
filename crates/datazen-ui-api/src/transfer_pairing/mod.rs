#[cfg(test)]
mod tests;

use crate::{normalize_driver_id, normalize_sync_family, sync_category, SyncCategory, SyncRegistryMeta};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferPath {
    Direct,
    Ir,
    Unsupported,
}

impl TransferPath {
    pub fn as_str(self) -> &'static str {
        match self {
            TransferPath::Direct => "direct",
            TransferPath::Ir => "ir",
            TransferPath::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferPairingResult {
    pub path: TransferPath,
    pub supported: bool,
    pub family: Option<String>,
    pub reason: Option<String>,
}

pub type TransferRegistry<'a> = dyn Fn(&str) -> Option<SyncRegistryMeta> + 'a;

fn transfer_category(database_type: &str, registry: &TransferRegistry<'_>) -> SyncCategory {
    let id = normalize_driver_id(database_type);
    if id == "redis" {
        return SyncCategory::Kv;
    }
    if id == "mongodb" {
        return SyncCategory::Document;
    }
    let sized = |db: &str| registry(db);
    sync_category(database_type, &sized)
}

pub fn resolve_transfer_pairing(
    source_type: &str,
    target_type: &str,
    registry: &TransferRegistry<'_>,
) -> TransferPairingResult {
    let source_category = transfer_category(source_type, registry);
    let target_category = transfer_category(target_type, registry);

    if source_category != target_category {
        return TransferPairingResult {
            path: TransferPath::Unsupported,
            supported: false,
            family: None,
            reason: Some(format!(
                "Transfer between {} ({}) and {} ({}) is not supported",
                source_type,
                source_category.as_str(),
                target_type,
                target_category.as_str()
            )),
        };
    }

    if source_category == SyncCategory::Other {
        return TransferPairingResult {
            path: TransferPath::Unsupported,
            supported: false,
            family: None,
            reason: Some(format!(
                "Transfer is not supported for database type '{source_type}'"
            )),
        };
    }

    if normalize_driver_id(source_type) == "redis"
        || normalize_driver_id(target_type) == "redis"
    {
        return TransferPairingResult {
            path: TransferPath::Unsupported,
            supported: false,
            family: None,
            reason: Some("Data Transfer has no Redis source and target adapters".to_owned()),
        };
    }

    let sized = |db: &str| registry(db);
    let source_family = normalize_sync_family(source_type, &sized);
    let target_family = normalize_sync_family(target_type, &sized);
    if source_family == target_family {
        return TransferPairingResult {
            path: TransferPath::Direct,
            supported: true,
            family: Some(source_family),
            reason: None,
        };
    }

    if source_category == SyncCategory::Sql {
        return TransferPairingResult {
            path: TransferPath::Ir,
            supported: true,
            family: None,
            reason: None,
        };
    }

    TransferPairingResult {
        path: TransferPath::Unsupported,
        supported: false,
        family: None,
        reason: Some(format!(
            "Transfer between {source_type} and {target_type} is not supported"
        )),
    }
}

pub fn is_transfer_target_supported(
    source_type: &str,
    target_type: &str,
    registry: &TransferRegistry<'_>,
) -> bool {
    resolve_transfer_pairing(source_type, target_type, registry).supported
}
