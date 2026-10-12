//! Sync pairing policy: same dialect family → Direct; cross SQL → IR; cross category → forbidden.
//!
//! Category/family rules are driven by driver-api taxonomy (`sync_category_of` /
//! `sync_family_of`); frontend Transfer UI reads `DatabaseTypeMeta` instead.

use datazen_driver_api::{sync_category_of, sync_family_of, SyncCategory};

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

/// Normalize a database type id to its sync dialect family.
pub fn normalize_sync_family(raw: &str) -> String {
    sync_family_of(raw)
}

/// Map a database type id to a sync category.
pub fn sync_category(raw: &str) -> SyncCategory {
    sync_category_of(raw)
}

/// Classify how a source/target pair should sync.
pub fn resolve_sync_pairing(source: &str, target: &str) -> SyncPairing {
    let src_cat = sync_category(source);
    let tgt_cat = sync_category(target);

    if src_cat != tgt_cat {
        return SyncPairing::Unsupported {
            reason: format!(
                "Sync between {} ({src_cat}) and {} ({tgt_cat}) is not supported",
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
