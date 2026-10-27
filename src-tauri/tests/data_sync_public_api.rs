use datazen_desktop::data_sync::filter::{
    SyncRecordset as FilterSyncRecordset, SyncRecordsetBound as FilterSyncRecordsetBound,
};
use datazen_desktop::data_sync::{SyncRecordset, SyncRecordsetBound};

#[test]
fn filter_module_keeps_legacy_recordset_imports_public() {
    let recordset: FilterSyncRecordset =
        serde_json::from_str(r#"{"start":{"value":"1"}}"#).unwrap();
    let bound: FilterSyncRecordsetBound = serde_json::from_str(r#"{"value":"1"}"#).unwrap();

    let _: SyncRecordset = recordset;
    let _: SyncRecordsetBound = bound;
}
