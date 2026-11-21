use super::*;

fn no_meta(_db: &str) -> Option<SyncRegistryMeta> {
    None
}

fn document_meta(db: &str) -> Option<SyncRegistryMeta> {
    match db {
        "couch" | "mongo" => Some(SyncRegistryMeta {
            sync_category: Some("document".to_owned()),
            sync_family: Some(db.to_owned()),
            ..Default::default()
        }),
        _ => None,
    }
}

#[test]
fn path_literals_are_pinned() {
    assert_eq!(TransferPath::Direct.as_str(), "direct");
    assert_eq!(TransferPath::Ir.as_str(), "ir");
    assert_eq!(TransferPath::Unsupported.as_str(), "unsupported");
}

#[test]
fn the_same_sql_family_takes_the_direct_path() {
    let result = resolve_transfer_pairing("postgresql", "postgres", &no_meta);
    assert_eq!(result.path, TransferPath::Direct);
    assert!(result.supported);
    assert_eq!(result.family.as_deref(), Some("postgresql"));
    assert_eq!(result.reason, None);
    assert!(is_transfer_target_supported("postgresql", "postgres", &no_meta));
}

#[test]
fn cross_family_sql_pairs_take_the_ir_path() {
    let result = resolve_transfer_pairing("postgresql", "mysql", &no_meta);
    assert_eq!(result.path, TransferPath::Ir);
    assert!(result.supported);
    assert_eq!(result.family, None);
    assert_eq!(result.reason, None);
}

#[test]
fn cross_category_pairs_are_refused_with_both_categories_in_the_reason() {
    let result = resolve_transfer_pairing("postgresql", "redis", &no_meta);
    assert_eq!(result.path, TransferPath::Unsupported);
    assert!(!result.supported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Transfer between postgresql (sql) and redis (kv) is not supported")
    );
    assert!(!is_transfer_target_supported("postgresql", "redis", &no_meta));
}

#[test]
fn the_other_category_has_its_own_reason() {
    let result = resolve_transfer_pairing("kiwi", "kiwi", &no_meta);
    assert_eq!(result.path, TransferPath::Unsupported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Transfer is not supported for database type 'kiwi'")
    );
}

#[test]
fn redis_is_refused_even_on_a_same_category_pair() {
    let result = resolve_transfer_pairing("redis", "redis", &no_meta);
    assert_eq!(result.path, TransferPath::Unsupported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Data Transfer has no Redis source and target adapters")
    );
    let as_target = resolve_transfer_pairing("postgresql", "redis", &no_meta);
    assert!(!as_target.supported);
}

#[test]
fn mongodb_is_mapped_to_the_document_category_without_registry_metadata() {
    let result = resolve_transfer_pairing("mongodb", "mongodb", &no_meta);
    assert_eq!(result.path, TransferPath::Direct);
    assert_eq!(result.family.as_deref(), Some("mongodb"));
    let cross = resolve_transfer_pairing("mongodb", "postgresql", &no_meta);
    assert_eq!(
        cross.reason.as_deref(),
        Some("Transfer between mongodb (document) and postgresql (sql) is not supported")
    );
}

#[test]
fn same_non_sql_category_with_different_families_is_unsupported() {
    let result = resolve_transfer_pairing("couch", "mongo", &document_meta);
    assert_eq!(result.path, TransferPath::Unsupported);
    assert!(!result.supported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Transfer between couch and mongo is not supported")
    );
}

#[test]
fn registry_metadata_can_override_the_family() {
    let family_meta = |db: &str| {
        if db == "postgres" || db == "postgresql" {
            Some(SyncRegistryMeta {
                sync_family: Some("shared".to_owned()),
                ..Default::default()
            })
        } else {
            None
        }
    };
    let result = resolve_transfer_pairing("postgresql", "postgres", &family_meta);
    assert_eq!(result.path, TransferPath::Direct);
    assert_eq!(result.family.as_deref(), Some("shared"));
}

#[test]
fn empty_and_alias_ids_are_normalized_before_comparison() {
    let result = resolve_transfer_pairing("MSSQL", "sqlserver", &no_meta);
    assert_eq!(result.path, TransferPath::Direct);
    assert_eq!(result.family.as_deref(), Some("sqlserver"));
    let presto = resolve_transfer_pairing("presto", "trino", &no_meta);
    assert_eq!(presto.path, TransferPath::Direct);
}

#[test]
fn crate_root_reexports_the_transfer_pairing_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::TransferPath>();
    resolves::<crate::TransferPairingResult>();
    resolves::<crate::TransferRegistry<'static>>();
    assert_eq!(crate::TransferPath::Ir.as_str(), "ir");
    assert!(crate::is_transfer_target_supported("postgresql", "mysql", &no_meta));
}
