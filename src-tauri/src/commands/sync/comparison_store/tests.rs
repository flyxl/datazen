use super::*;
#[path = "tester_recovery_tests.rs"]
mod tester_recovery_tests;
use crate::data_sync::{
    compare_table_pages_to_sink, ChangeOperation, RowChange, SliceRowSource, SyncOptions,
    SyncSourceFilter, TableResult,
};
use uuid::Uuid;

const CHILD_OWNER_ROOT_ENV: &str = "DATAZEN_TEST_COMPARISON_OWNER_ROOT";

fn inserted_row(key: i64) -> RowChange {
    RowChange::insert(
        vec![Value::Integer(key)],
        vec![Some(Value::String(format!("row-{key}")))],
        &SyncOptions::default(),
    )
}

fn comparison(payload_size: usize) -> ComparisonResult {
    let options = SyncOptions::default();
    ComparisonResult::new(vec![TableResult::matched(
        "users",
        "users",
        vec![RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::String("x".repeat(payload_size)))],
            &options,
        )],
    )])
}

fn multi_table_comparison() -> ComparisonResult {
    let options = SyncOptions::default();
    let rows = |table: &str| {
        (0..4)
            .map(|value| {
                RowChange::insert(
                    vec![Value::Integer(value)],
                    vec![Some(Value::String(format!(
                        "{table}-{value}-{}",
                        "x".repeat(32)
                    )))],
                    &options,
                )
            })
            .collect()
    };
    ComparisonResult::new(vec![
        TableResult::matched("users", "users", rows("users")),
        TableResult::matched("orders", "orders", rows("orders")),
    ])
}

fn row_bearing_table() -> TableResult {
    let options = SyncOptions::default();
    let mut insert = RowChange::insert(
        vec![Value::Integer(1)],
        vec![Some(Value::String("new".into()))],
        &options,
    );
    insert.selected = false;
    let update = RowChange::update(
        vec![Value::Integer(2)],
        vec![Some(Value::String("after".into()))],
        vec![Some(Value::String("before".into()))],
        vec!["name".into()],
        &options,
    );
    let delete = RowChange::delete(
        vec![Value::Integer(3)],
        vec![Some(Value::String("removed".into()))],
        &options,
    );
    let unchanged = RowChange::unchanged(
        vec![Value::Integer(4)],
        vec![Some(Value::String("same".into()))],
        vec![Some(Value::String("same".into()))],
    );
    let mut table = TableResult::matched(
        "source_users",
        "target_users",
        vec![insert, update, delete, unchanged],
    );
    table.columns = vec!["id".into(), "name".into()];
    table.column_types = vec!["BIGINT".into(), "TEXT".into()];
    table.primary_keys = vec!["id".into()];
    table.unchanged_count = 2;
    table.warnings = vec!["kept for review".into()];
    table.source_filter = Some(SyncSourceFilter(serde_json::json!({
        "filters": [{ "column": "id", "operator": "GREATER_THAN", "value": 0 }],
        "logic": "AND"
    })));
    table
}

#[test]
fn small_comparison_stays_inline_and_round_trips() {
    let store = ComparisonStore::from_comparison(comparison(8)).unwrap();
    assert!(!store.is_spilled());
    assert_eq!(store.load().unwrap().tables.len(), 1);
    assert_eq!(store.summaries().unwrap()[0].row_count, 1);
    assert_eq!(
        store.load_table_page("users", "users", 0, 1).unwrap().len(),
        1
    );
}

#[test]
fn one_shot_add_table_preserves_rows_metadata_counts_and_order() {
    let expected = row_bearing_table();
    let inline =
        ComparisonStore::from_comparison(ComparisonResult::new(vec![expected.clone()])).unwrap();
    assert!(!inline.is_spilled());
    let inline_loaded = inline.load().unwrap();
    assert_eq!(
        serde_json::to_value(&inline_loaded.tables[0]).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("stores");
    let mut writer = StreamingComparisonStoreWriter::new_at(&root, None).unwrap();
    writer.add_table(expected.clone()).unwrap();
    let store = writer.finish().unwrap();

    assert!(store.is_spilled());
    let loaded = store.load().unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.tables[0]).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    let summary = &store.summaries().unwrap()[0];
    assert_eq!(summary.row_count, 4);
    assert_eq!(summary.insert_count, 1);
    assert_eq!(summary.update_count, 1);
    assert_eq!(summary.delete_count, 1);
    assert_eq!(summary.unchanged_count, 3);
    assert_eq!(
        serde_json::to_value(
            store
                .load_table_page("source_users", "target_users", 0, 4)
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&expected.rows).unwrap()
    );
    assert_eq!(
        serde_json::to_value(
            store
                .load_table_page("source_users", "target_users", 1, 2)
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&expected.rows[1..3]).unwrap()
    );
}

#[test]
fn one_shot_add_table_row_write_failures_abort_without_publishing() {
    for (failure, expected_message) in [
        (TestWriteFailure::RowPayload, "comparison row"),
        (TestWriteFailure::RowIndex, "comparison row index"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("stores");
        let mut writer = StreamingComparisonStoreWriter::new_at(&root, Some(failure)).unwrap();
        let directory = writer.directory.clone().unwrap();

        let error = writer.add_table(row_bearing_table()).unwrap_err();

        assert!(error.contains(expected_message));
        assert!(!directory.exists());
        assert!(writer.finish().unwrap_err().contains(expected_message));
        assert!(!directory.exists());
    }
}

#[test]
fn large_comparison_spills_to_private_indexed_store_and_round_trips() {
    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let path = store.path().unwrap();
    let directory = store.directory().unwrap();
    assert!(store.is_spilled());
    assert!(path.exists());
    assert!(directory.exists());
    #[cfg(unix)]
    {
        let root = directory.parent().unwrap();
        let owner: StoreOwnerMarker =
            serde_json::from_slice(&fs::read(directory.join(STORE_OWNER_FILE)).unwrap()).unwrap();
        assert_eq!(
            fs::metadata(root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.join("table-0.rows"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.join(STORE_OWNER_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(root.join(OWNER_DIRECTORY))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(
                root.join(OWNER_DIRECTORY)
                    .join(format!("{}.sqlite", owner.owner_id))
            )
            .unwrap()
            .permissions()
            .mode()
                & 0o777,
            0o600
        );
    }
    assert!(store.bytes() > COMPARISON_MEMORY_LIMIT);
    assert_eq!(store.load().unwrap().tables[0].rows.len(), 1);
    assert_eq!(store.summaries().unwrap()[0].row_count, 1);
    drop(store);
    assert!(!directory.exists());
}

#[test]
fn manifest_operation_count_tampering_fails_closed() {
    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let manifest = store.path().unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    document["tables"][0]["insertCount"] = serde_json::Value::from(0usize);
    fs::write(&manifest, serde_json::to_vec(&document).unwrap()).unwrap();

    let summary_error = store.summaries().unwrap_err();
    assert!(summary_error.contains("operation counts do not match"));
    let page_error = store.load_table_page("users", "users", 0, 1).unwrap_err();
    assert!(page_error.contains("operation counts do not match"));
    let load_error = store.load().unwrap_err();
    assert!(load_error.contains("operation counts do not match"));
}

#[test]
fn indexed_page_reads_only_requested_table_and_rows_without_full_load() {
    let comparison = multi_table_comparison();
    let store = ComparisonStore::from_comparison(ComparisonResult::new(vec![
        TableResult::matched(
            "large-users",
            "large-users",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::String("x".repeat(COMPARISON_MEMORY_LIMIT + 1)))],
                &SyncOptions::default(),
            )],
        ),
        comparison.tables[1].clone(),
    ]))
    .unwrap();
    assert!(store.is_spilled());
    let before = store.full_load_calls();
    let page = store.load_table_page("orders", "orders", 1, 2).unwrap();
    assert_eq!(store.full_load_calls(), before);
    assert_eq!(page.len(), 2);
    assert!(matches!(page[0].key.as_slice(), [Value::Integer(1)]));
    assert!(matches!(page[1].key.as_slice(), [Value::Integer(2)]));
    assert!(store.load_table_page("missing", "orders", 0, 1).is_err());
    assert!(store.load_table_page("orders", "orders", 99, 1).is_err());
}

#[test]
fn malformed_manifest_and_row_file_are_rejected_and_cleaned_on_drop() {
    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let manifest = store.path().unwrap();
    let directory = store.directory().unwrap();
    fs::write(&manifest, b"not-json").unwrap();
    assert!(store
        .summaries()
        .unwrap_err()
        .contains("cannot decode Data Sync comparison store"));
    drop(store);
    assert!(!directory.exists());

    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let directory = store.directory().unwrap();
    let row_path = directory.join("table-0.rows");
    fs::write(&row_path, b"broken").unwrap();
    let error = store.load().unwrap_err();
    assert!(error.contains("row file length does not match"));
    drop(store);
    assert!(!directory.exists());

    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let directory = store.directory().unwrap();
    let row_path = directory.join("table-0.rows");
    let mut bytes = fs::read(&row_path).unwrap();
    bytes[0] = bytes[0].wrapping_add(1);
    fs::write(&row_path, bytes).unwrap();
    let summary_error = store.summaries().unwrap_err();
    assert!(summary_error.contains("frame length does not match"));
    let load_error = store.load().unwrap_err();
    assert!(load_error.contains("frame length does not match"));
    let page_error = store.load_table_page("users", "users", 0, 1).unwrap_err();
    assert!(page_error.contains("frame length does not match"));
    drop(store);
    assert!(!directory.exists());
}

#[test]
fn trailing_manifest_data_is_rejected() {
    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let path = store.path().unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b" trailing");
    fs::write(&path, bytes).unwrap();
    assert!(store
        .load()
        .unwrap_err()
        .contains("store manifest contains trailing data"));
}

#[test]
fn cloned_owner_delays_cleanup_until_last_reference() {
    let store = ComparisonStore::from_comparison(comparison(COMPARISON_MEMORY_LIMIT + 1)).unwrap();
    let directory = store.directory().unwrap();
    let clone = store.clone();
    drop(store);
    assert!(directory.exists());
    drop(clone);
    assert!(!directory.exists());
}

#[test]
fn existing_path_is_rejected_instead_of_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collision.json");
    fs::write(&path, b"existing").unwrap();
    let error = create_new_file(&path).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&path).unwrap(), b"existing");
}

#[test]
fn streaming_writer_spills_rows_incrementally_and_cleans_unfinished_output() {
    let options = SyncOptions::default();
    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    for key in 0..2 {
        writer
            .push_row(RowChange::insert(
                vec![Value::Integer(key)],
                vec![Some(Value::Integer(key))],
                &options,
            ))
            .unwrap();
    }
    writer.finish_table(3).unwrap();
    let store = writer.finish().unwrap();
    assert!(store.is_spilled());
    let summary = store.summaries().unwrap();
    assert_eq!(summary[0].row_count, 2);
    assert_eq!(summary[0].unchanged_count, 3);
    assert_eq!(summary[0].insert_count, 2);
    assert_eq!(
        store.load_table_page("users", "users", 0, 2).unwrap().len(),
        2
    );
    drop(store);
    assert!(!directory.exists());

    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("cancelled", "cancelled", Vec::new()))
        .unwrap();
    writer
        .push_row(RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            &options,
        ))
        .unwrap();
    drop(writer);
    assert!(!directory.exists());
}

#[test]
fn test_tester_streaming_writer_round_trips_zero_one_and_large_row_counts() {
    let options = SyncOptions::default();
    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    writer
        .begin_table(TableResult::matched("empty", "empty", Vec::new()))
        .unwrap();
    writer.finish_table(0).unwrap();

    writer
        .begin_table(TableResult::matched("single", "single", Vec::new()))
        .unwrap();
    writer
        .push_row(RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            &options,
        ))
        .unwrap();
    writer.finish_table(0).unwrap();

    writer
        .begin_table(TableResult::matched("large", "large", Vec::new()))
        .unwrap();
    for key in 0..10_001 {
        writer
            .push_row(RowChange::insert(
                vec![Value::Integer(key)],
                vec![Some(Value::Integer(key))],
                &options,
            ))
            .unwrap();
    }
    writer.finish_table(0).unwrap();
    let store = writer.finish().unwrap();

    let summaries = store.summaries().unwrap();
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries[0].row_count, 0);
    assert_eq!(summaries[1].row_count, 1);
    assert_eq!(summaries[2].row_count, 10_001);
    assert!(store
        .load_table_page("empty", "empty", 0, 1)
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .load_table_page("large", "large", 10_000, 2)
            .unwrap()
            .len(),
        1
    );
    let loaded = store.load().unwrap();
    assert_eq!(loaded.tables[0].rows.len(), 0);
    assert_eq!(loaded.tables[1].rows.len(), 1);
    assert_eq!(loaded.tables[2].rows.len(), 10_001);
}

#[test]
fn test_tester_index_corruption_fails_summary_page_and_full_load() {
    let options = SyncOptions::default();
    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    writer
        .push_row(RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            &options,
        ))
        .unwrap();
    writer.finish_table(0).unwrap();
    let store = writer.finish().unwrap();
    let directory = store.directory().unwrap();
    let index_path = directory.join("table-0.index");
    let mut bytes = fs::read(&index_path).unwrap();
    bytes[8] = bytes[8].wrapping_add(1);
    fs::write(&index_path, bytes).unwrap();

    assert!(store.summaries().unwrap_err().contains("frame length"));
    assert!(store.load_table_page("users", "users", 0, 1).is_err());
    assert!(store.load().is_err());
    drop(store);
    assert!(!directory.exists());
}

#[tokio::test]
async fn test_tester_cancelled_sink_drops_partial_store() {
    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut source = SliceRowSource::new(vec![vec![Some(Value::Integer(1))]], vec![0]).unwrap();
    let mut target = SliceRowSource::new(Vec::new(), vec![0]).unwrap();
    let error = compare_table_pages_to_sink(
        "users",
        "users",
        &[0],
        &["id".into()],
        &SyncOptions::default(),
        &mut source,
        &mut target,
        Some(flag),
        &mut writer,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, DataSyncError::Cancelled(_)));
    drop(writer);
    assert!(!directory.exists());
}

#[test]
fn test_tester_unchanged_row_is_rejected_and_writer_cleanup_is_recoverable() {
    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    let error = writer
        .push_row(RowChange {
            operation: ChangeOperation::Unchanged,
            key: vec![Value::Integer(1)],
            source_row: Some(vec![Some(Value::Integer(1))]),
            target_row: Some(vec![Some(Value::Integer(1))]),
            changed_columns: Vec::new(),
            selected: false,
        })
        .unwrap_err();
    assert!(error.to_string().contains("unchanged rows"));
    drop(writer);
    assert!(!directory.exists());
}

#[test]
fn test_tester_full_load_fails_closed_above_64_mib_but_page_stays_available() {
    let options = SyncOptions::default();
    let mut writer = StreamingComparisonStoreWriter::new().unwrap();
    writer
        .begin_table(TableResult::matched("large", "large", Vec::new()))
        .unwrap();
    for key in 0..9 {
        writer
            .push_row(RowChange::insert(
                vec![Value::Integer(key)],
                vec![Some(Value::String("x".repeat(8_000_000)))],
                &options,
            ))
            .unwrap();
    }
    writer.finish_table(0).unwrap();
    let store = writer.finish().unwrap();
    assert!(store.bytes() > COMPARISON_FULL_LOAD_LIMIT as usize);
    assert!(store.load().unwrap_err().contains("64 MiB"));
    assert_eq!(store.summaries().unwrap()[0].row_count, 9);
    assert_eq!(
        store.load_table_page("large", "large", 0, 1).unwrap().len(),
        1
    );
}

#[test]
fn stale_owner_store_is_reclaimed_after_its_lease_is_released() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    assert!(format!("{owner:?}").contains(&owner.id.to_string()));
    let directory = create_store_directory(&root, &owner).unwrap();
    fs::write(directory.join("partial.rows"), b"incomplete").unwrap();
    let outside = temporary.path().join("outside.txt");
    fs::write(&outside, b"must survive stale cleanup").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, directory.join("outside-link")).unwrap();
    let owner_id = owner.id;
    let owner_lease_path = root
        .join(OWNER_DIRECTORY)
        .join(format!("{owner_id}.sqlite"));
    drop(owner);

    scavenge_stale_stores(&root);

    assert!(
        !directory.exists(),
        "stale store owned by {owner_id} remains"
    );
    assert!(
        outside.exists(),
        "stale cleanup followed a symlink outside the root"
    );
    assert!(
        !owner_lease_path.exists(),
        "unused stale owner lease remains"
    );
}

#[test]
fn malformed_and_unrecognized_entries_are_preserved_during_stale_sweep() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let outside = temporary.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"keep").unwrap();

    let owner = acquire_owner_lease(&root).unwrap();
    let malformed = create_store_directory(&root, &owner).unwrap();
    fs::write(malformed.join(STORE_OWNER_FILE), b"not-json").unwrap();
    let unrecognized = root.join("comparison-not-a-store");
    fs::create_dir(&unrecognized).unwrap();
    fs::write(unrecognized.join("keep.txt"), b"keep").unwrap();
    let symlink = root.join(format!(
        "{STORE_DIRECTORY_PREFIX}{}-{}",
        owner.id,
        Uuid::new_v4()
    ));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &symlink).unwrap();
    let symlink_target = outside.join("keep.txt");
    drop(owner);

    scavenge_stale_stores(&root);

    assert!(malformed.exists());
    assert!(unrecognized.join("keep.txt").exists());
    assert!(symlink_target.exists());
    #[cfg(unix)]
    assert!(fs::symlink_metadata(symlink)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn cleanup_error_is_best_effort_and_does_not_block_other_stale_stores() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let first_owner = acquire_owner_lease(&root).unwrap();
    let blocked = create_store_directory(&root, &first_owner).unwrap();
    drop(first_owner);
    let second_owner = acquire_owner_lease(&root).unwrap();
    let reclaimable = create_store_directory(&root, &second_owner).unwrap();
    drop(second_owner);

    scavenge_stale_stores_with(&root, |root, directory, owner, store| {
        if directory == blocked {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated cleanup failure",
            ))
        } else {
            remove_stale_store_directory(root, directory, owner, store)
        }
    });

    assert!(blocked.exists());
    assert!(!reclaimable.exists());
}

#[test]
fn cleanup_refuses_paths_outside_the_feature_store_root() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let outside = temporary.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let owner = Uuid::new_v4();
    let store = Uuid::new_v4();
    let outside_store = outside.join(format!("{STORE_DIRECTORY_PREFIX}{owner}-{store}"));
    fs::create_dir(&outside_store).unwrap();
    fs::write(outside_store.join("keep.txt"), b"keep").unwrap();

    let error = remove_stale_store_directory(&root, &outside_store, owner, store).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(outside_store.join("keep.txt").exists());

    remove_owned_store_directory(&root, &outside_store, owner);
    assert!(outside_store.join("keep.txt").exists());
}

#[test]
fn partial_row_and_index_write_failures_abort_and_cannot_be_finalized() {
    for (failure, expected) in [
        (TestWriteFailure::RowPayload, "comparison row"),
        (TestWriteFailure::RowIndex, "comparison row index"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("stores");
        let mut writer = StreamingComparisonStoreWriter::new_at(&root, Some(failure)).unwrap();
        let directory = writer.directory.clone().unwrap();
        writer
            .begin_table(TableResult::matched("users", "users", Vec::new()))
            .unwrap();

        let error = writer.push_row(inserted_row(1)).unwrap_err();

        assert!(error.to_string().contains(expected));
        assert!(!directory.exists());
        assert!(writer
            .begin_table(TableResult::matched("retry", "retry", Vec::new()))
            .unwrap_err()
            .contains(expected));
        assert!(writer.finish_table(0).unwrap_err().contains(expected));
        assert!(writer.finish().unwrap_err().contains(expected));
        assert!(!directory.exists());
    }
}

#[test]
fn partial_manifest_write_failure_removes_store_and_publishes_no_plan() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("stores");
    let mut writer =
        StreamingComparisonStoreWriter::new_at(&root, Some(TestWriteFailure::Manifest)).unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    writer.push_row(inserted_row(1)).unwrap();
    writer.finish_table(0).unwrap();

    let error = writer.finish().unwrap_err();

    assert!(error.contains("cannot write Data Sync comparison manifest"));
    assert!(!directory.exists());
}

#[test]
fn comparison_owner_lease_child_process() {
    let Ok(root) = std::env::var(CHILD_OWNER_ROOT_ENV) else {
        return;
    };
    let root = ensure_private_store_root(Path::new(&root)).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    // Publish through a rename: a reader that polls for the file's *existence*
    // must never observe it half-written, or it parses an empty path.
    let published = root.join("child-store-path");
    let staging = root.join("child-store-path.staging");
    fs::write(&staging, directory.to_string_lossy().as_bytes()).unwrap();
    fs::rename(&staging, &published).unwrap();
    while !root.join("child-release").exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(owner);
}

#[test]
fn live_store_owned_by_another_process_is_not_reclaimed() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("comparison_owner_lease_child_process")
        .env(CHILD_OWNER_ROOT_ENV, &root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let ready = root.join("child-store-path");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("lease child exited before publishing its store path: {status}");
        }
        if std::time::Instant::now() >= deadline {
            fs::write(root.join("child-release"), b"release").unwrap();
            let _ = child.wait();
            panic!("lease child did not become ready");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let directory = PathBuf::from(fs::read_to_string(&ready).unwrap());
    assert!(directory.exists());

    scavenge_stale_stores(&root);
    let preserved_while_live = directory.exists();

    fs::write(root.join("child-release"), b"release").unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    assert!(
        preserved_while_live,
        "live peer's comparison store was reclaimed"
    );
    scavenge_stale_stores(&root);
    assert!(
        !directory.exists(),
        "exited peer's comparison store was not reclaimed"
    );
}

#[test]
fn test_tester_abrupt_child_exit_releases_sqlite_owner_lease() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("comparison_owner_lease_child_process")
        .env(CHILD_OWNER_ROOT_ENV, &root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let ready = root.join("child-store-path");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("lease child exited before publishing its store path: {status}");
        }
        if std::time::Instant::now() >= deadline {
            fs::write(root.join("child-release"), b"release").unwrap();
            let _ = child.wait();
            panic!("lease child did not become ready");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let directory = PathBuf::from(fs::read_to_string(&ready).unwrap());
    let owner_id = parse_store_directory_name(directory.file_name().unwrap().to_str().unwrap())
        .unwrap()
        .0;
    let lease_path = root
        .join(OWNER_DIRECTORY)
        .join(format!("{owner_id}.sqlite"));
    assert!(directory.exists());

    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success(), "child should be terminated without Drop");

    scavenge_stale_stores(&root);

    assert!(
        !directory.exists(),
        "abruptly exited peer's comparison store was not reclaimed"
    );
    assert!(
        !lease_path.exists(),
        "abruptly exited peer's owner lease was not reaped"
    );
}
