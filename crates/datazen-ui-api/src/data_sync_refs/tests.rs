use super::*;
use crate::{DataSyncOperation, DataSyncSelectionScopeMode};

#[test]
fn surface_and_gap_are_pinned() {
    assert_eq!(DATA_SYNC_REFS_SURFACE.len(), 3);
    assert_eq!(DATA_SYNC_REFS_GAP.len(), 3);
    assert_eq!(DATA_SYNC_REFS_SURFACE[0], "useDataSyncRefs");
}

#[test]
fn every_ref_starts_at_its_documented_initial_value() {
    let refs = create_data_sync_refs();
    assert_eq!(refs.job_id, None);
    assert_eq!(refs.job_kind, None);
    assert_eq!(refs.cancel_requested_job, None);
    assert_eq!(refs.cancelling_status_job, None);
    assert_eq!(refs.compare_generation, 0);
    assert!(!refs.write_in_flight);
    assert_eq!(refs.sync_state, DataSyncSyncState::Idle);
    assert!(refs.selected_rows.is_empty());
    assert!(refs.table_selections.is_empty());
    assert!(refs.loaded_pages.is_empty());
    assert_eq!(refs.pending_profile_mappings, None);
}

#[test]
fn the_factory_matches_the_default() {
    assert_eq!(create_data_sync_refs(), DataSyncSharedRefs::default());
}

#[test]
fn mutations_are_isolated_between_bundles() {
    let mut first = create_data_sync_refs();
    first.job_id = Some("job-1".to_owned());
    first.job_kind = Some(DataSyncJobKind::Execute);
    first.compare_generation = 3;
    first.write_in_flight = true;
    first.sync_state = DataSyncSyncState::Compared;
    first.selected_rows.push(DataSyncSelectedRow {
        source_table: "a".to_owned(),
        target_table: "a".to_owned(),
        operation: DataSyncOperation::Insert,
        key: vec![serde_json::json!(1)],
    });
    first.table_selections.push(DataSyncTableSelection {
        source_table: "a".to_owned(),
        target_table: "a".to_owned(),
        selection_mode: DataSyncSelectionScopeMode::All,
        operations: vec![DataSyncOperation::Insert],
    });
    first.loaded_pages.insert("page-1".to_owned());
    first.pending_profile_mappings = Some(vec![DataSyncTableMapping {
        source_table: "a".to_owned(),
        target_table: "a".to_owned(),
        enabled: true,
        source_filter: None,
    }]);

    let second = create_data_sync_refs();
    assert_ne!(first, second);
    assert!(second.job_id.is_none());
    assert_eq!(second.compare_generation, 0);
    assert!(second.loaded_pages.is_empty());
}

#[test]
fn crate_root_reexports_the_refs_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::DataSyncSharedRefs>();
    resolves::<crate::DataSyncTableMapping>();
    assert_eq!(crate::create_data_sync_refs().sync_state, crate::DataSyncSyncState::Idle);
    assert_eq!(crate::DATA_SYNC_REFS_GAP.len(), 3);
}
