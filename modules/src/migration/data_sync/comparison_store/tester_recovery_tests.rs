use super::*;

#[cfg(unix)]
#[test]
fn test_tester_store_root_symlink_is_rejected_without_touching_target() {
    let temporary = tempfile::tempdir().unwrap();
    let target = temporary.path().join("real-store-root");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.txt"), b"keep").unwrap();
    let root_link = temporary.path().join("store-root-link");
    std::os::unix::fs::symlink(&target, &root_link).unwrap();

    let error = ensure_private_store_root(&root_link).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
    assert!(fs::symlink_metadata(root_link)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn test_tester_oversized_owner_marker_is_preserved_by_stale_sweep() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    fs::write(
        directory.join(STORE_OWNER_FILE),
        vec![b'x'; (MAX_STORE_OWNER_BYTES + 1) as usize],
    )
    .unwrap();
    let (owner_id, store_id) =
        parse_store_directory_name(directory.file_name().unwrap().to_str().unwrap()).unwrap();
    let cleanup_error =
        remove_stale_store_directory(&root, &directory, owner_id, store_id).unwrap_err();
    assert_eq!(cleanup_error.kind(), io::ErrorKind::InvalidData);
    drop(owner);

    scavenge_stale_stores(&root);

    assert!(directory.exists(), "oversized marker must fail closed");
    assert_eq!(
        fs::metadata(directory.join(STORE_OWNER_FILE))
            .unwrap()
            .len(),
        MAX_STORE_OWNER_BYTES + 1
    );
}

#[cfg(unix)]
#[test]
fn test_tester_symlinked_owner_marker_is_preserved_by_stale_sweep() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let outside = temporary.path().join("outside-marker.json");
    fs::write(&outside, b"{\"formatVersion\":1}").unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    let marker = directory.join(STORE_OWNER_FILE);
    fs::remove_file(&marker).unwrap();
    std::os::unix::fs::symlink(&outside, &marker).unwrap();
    drop(owner);

    scavenge_stale_stores(&root);

    assert!(directory.exists(), "symlink marker must fail closed");
    assert_eq!(fs::read(&outside).unwrap(), b"{\"formatVersion\":1}");
}

#[test]
fn test_tester_missing_empty_and_corrupt_owner_leases_fail_closed() {
    for invalid_lease in ["missing", "empty", "corrupt"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
        let owner = acquire_owner_lease(&root).unwrap();
        let directory = create_store_directory(&root, &owner).unwrap();
        let lease = root
            .join(OWNER_DIRECTORY)
            .join(format!("{}.sqlite", owner.id));
        drop(owner);

        match invalid_lease {
            "missing" => fs::remove_file(&lease).unwrap(),
            "empty" => fs::write(&lease, b"").unwrap(),
            "corrupt" => fs::write(&lease, b"not a sqlite database").unwrap(),
            _ => unreachable!(),
        }

        scavenge_stale_stores(&root);

        assert!(
            directory.exists(),
            "{invalid_lease} owner lease must fail closed"
        );
    }
}

#[cfg(unix)]
#[test]
fn test_tester_symlinked_owner_lease_and_noncanonical_store_ids_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    let lease = root
        .join(OWNER_DIRECTORY)
        .join(format!("{}.sqlite", owner.id));
    drop(owner);

    let outside = temporary.path().join("outside-owner.sqlite");
    fs::write(&outside, b"outside").unwrap();
    fs::remove_file(&lease).unwrap();
    std::os::unix::fs::symlink(&outside, &lease).unwrap();
    let name = directory.file_name().unwrap().to_str().unwrap();
    let noncanonical = name
        .split_once('-')
        .map(|(prefix, ids)| format!("{prefix}-{}", ids.to_uppercase()))
        .unwrap();
    assert!(parse_store_directory_name(&noncanonical).is_none());

    scavenge_stale_stores(&root);

    assert!(directory.exists(), "symlink lease must not prove staleness");
    assert!(fs::symlink_metadata(lease)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(outside).unwrap(), b"outside");
}

#[cfg(unix)]
#[test]
fn test_tester_unreadable_store_root_is_left_untouched() {
    use std::os::unix::fs::PermissionsExt;

    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    drop(owner);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();

    scavenge_stale_stores(&root);

    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(directory.exists(), "unreadable root must not be scavenged");
}

#[test]
fn test_tester_owned_cleanup_rejects_foreign_owner_and_is_idempotent_on_missing_path() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    let foreign_owner = Uuid::new_v4();

    remove_owned_store_directory(&root, &directory, foreign_owner);
    assert!(directory.exists(), "foreign owner must not clean the store");

    remove_owned_store_directory(&root, &directory, owner.id);
    assert!(!directory.exists());
    remove_owned_store_directory(&root, &directory, owner.id);
}

#[test]
fn test_tester_cleanup_not_found_is_best_effort() {
    let temporary = tempfile::tempdir().unwrap();
    let root = ensure_private_store_root(&temporary.path().join("stores")).unwrap();
    let owner = acquire_owner_lease(&root).unwrap();
    let directory = create_store_directory(&root, &owner).unwrap();
    drop(owner);

    scavenge_stale_stores_with(&root, |_root, _directory, _owner, _store| {
        Err(io::Error::new(io::ErrorKind::NotFound, "already removed"))
    });

    assert!(directory.exists());
}

#[tokio::test]
async fn test_tester_mutation_operation_counts_include_insert_update_delete_and_unchanged() {
    let options = SyncOptions::default();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("stores");
    let mut writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    crate::migration::data_sync::RowChangeSink::push(&mut writer, inserted_row(1))
        .await
        .unwrap();
    writer
        .push_row(RowChange::update(
            vec![Value::Integer(2)],
            vec![Some(Value::String("updated".into()))],
            vec![Some(Value::String("old".into()))],
            vec!["name".into()],
            &options,
        ))
        .unwrap();
    writer
        .push_row(RowChange::delete(
            vec![Value::Integer(3)],
            vec![Some(Value::String("deleted".into()))],
            &options,
        ))
        .unwrap();
    crate::migration::data_sync::RowChangeSink::unchanged(&mut writer)
        .await
        .unwrap();
    writer.finish_table(1).unwrap();
    let store = writer.finish().unwrap();

    let summary = store.summaries().unwrap().remove(0);
    assert_eq!(summary.row_count, 3);
    assert_eq!(summary.insert_count, 1);
    assert_eq!(summary.update_count, 1);
    assert_eq!(summary.delete_count, 1);
    assert_eq!(summary.unchanged_count, 1);
    assert_eq!(store.load().unwrap().tables[0].rows.len(), 3);
}

#[test]
fn test_tester_inline_page_bounds_and_writer_state_errors_cleanup() {
    let inline = comparison_store_from(comparison(8)).unwrap();
    assert!(inline.bytes() > 0);
    assert!(inline.path().is_none());
    assert!(inline.directory().is_none());
    assert!(inline
        .load_table_page("users", "users", 0, 0)
        .unwrap_err()
        .contains("limit must be greater than zero"));
    assert!(inline
        .load_table_page("users", "users", 2, 1)
        .unwrap_err()
        .contains("offset is outside"));

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("stores");
    let mut empty_writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    empty_writer
        .add_table(TableResult::matched("empty", "empty", Vec::new()))
        .unwrap();
    let empty_store = empty_writer.finish().unwrap();
    assert_eq!(empty_store.summaries().unwrap()[0].row_count, 0);

    let mut writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    let directory = writer.directory.clone().unwrap();
    assert!(writer
        .finish_table(0)
        .unwrap_err()
        .contains("not being written"));
    assert!(
        !directory.exists(),
        "invalid writer state must abort storage"
    );
    assert!(writer.finish().unwrap_err().contains("not being written"));

    let mut writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    assert!(writer
        .begin_table(TableResult::matched("orders", "orders", Vec::new()))
        .unwrap_err()
        .contains("already being written"));
    writer.push_row(inserted_row(1)).unwrap();
    writer.finish_table(0).unwrap();
    assert_eq!(
        writer.finish().unwrap().summaries().unwrap()[0].row_count,
        1
    );
    assert!(!directory.exists());

    let mut writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("unfinished", "unfinished", Vec::new()))
        .unwrap();
    assert!(writer.finish().unwrap_err().contains("not finalized"));
    assert!(!directory.exists(), "unfinished writer must drop its files");
}

#[test]
fn test_tester_row_index_creation_failure_closes_and_removes_partial_store() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("stores");
    let mut writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    let directory = writer.directory.clone().unwrap();
    fs::write(directory.join("table-0.index"), b"occupied").unwrap();

    let error = writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap_err();

    assert!(error.contains("comparison row index"));
    assert!(!directory.exists(), "partial row file should be removed");
    assert!(writer
        .begin_table(TableResult::matched("retry", "retry", Vec::new()))
        .unwrap_err()
        .contains("comparison row index"));
    assert!(writer
        .finish()
        .unwrap_err()
        .contains("comparison row index"));
}

#[test]
fn test_tester_manifest_creation_failure_removes_finalized_row_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("stores");
    let mut writer = StreamingComparisonStorageWriter::new_at(&root, None).unwrap();
    let directory = writer.directory.clone().unwrap();
    writer
        .begin_table(TableResult::matched("users", "users", Vec::new()))
        .unwrap();
    writer.push_row(inserted_row(1)).unwrap();
    writer.finish_table(0).unwrap();
    fs::write(directory.join(MANIFEST_FILE), b"occupied").unwrap();

    let error = writer.finish().unwrap_err();

    assert!(error.contains("cannot create Data Sync comparison manifest"));
    assert!(!directory.exists(), "unpublished store must be removed");
}
