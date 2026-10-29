//! Migration from the retired `favorite_queries` table.
//!
//! Split out of `tests.rs` only so both files stay inside the project's
//! single-file size budget. It remains a child module of `tests`, so
//! `use super::*` still reaches the same `Fixture` helpers.

use super::*;
use crate::store::favorites::migrate::{migrate_legacy_favorites, read_marker, MigrationOutcome};
use crate::store::history_db::HistoryDb;
use rusqlite::params;

/// `(uuid, connectionId, title, sql, createdAt)`, oldest first.
///
/// The timestamps are fixed rather than relative to "now" so that two
/// installs built from the same fixture derive the same ULIDs — which is
/// what makes the crash-retry test below able to place a leftover at the
/// exact path the retry will rewrite.
type LegacyRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const ROWS: [LegacyRow; 3] = [
    (
        "a1b2c3d4-0000-4000-8000-000000000001",
        "cfg-1",
        "Nightly recon",
        "SELECT * FROM orders WHERE d = CURRENT_DATE;",
        "2026-01-01T09:00:00+00:00",
    ),
    (
        "a1b2c3d4-0000-4000-8000-000000000002",
        "cfg-1",
        "Weekly rollup",
        "SELECT customer_id, SUM(total) FROM orders GROUP BY 1;",
        "2026-01-02T09:00:00+00:00",
    ),
    (
        "a1b2c3d4-0000-4000-8000-000000000003",
        "cfg-2",
        "Other db",
        "SELECT count(*) FROM users;",
        "2026-01-03T09:00:00+00:00",
    ),
];

/// A pre-§2.6 app data directory: `history.sqlite` holding the rows an
/// existing user has, and no favorites directory at all.
struct LegacyInstall {
    _fx: Fixture,
    data_dir: PathBuf,
}

impl LegacyInstall {
    fn new(rows: &[LegacyRow]) -> Self {
        let fx = Fixture::new();
        let data_dir = fx.path().join("appdata");
        fs::create_dir_all(&data_dir).unwrap();

        let db = HistoryDb::open(&data_dir).unwrap();
        db.with_raw_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE favorite_queries (
                     id TEXT PRIMARY KEY NOT NULL,
                     connection_id TEXT NOT NULL,
                     title TEXT NOT NULL,
                     sql TEXT NOT NULL,
                     created_at TEXT NOT NULL
                 );",
            )
            .unwrap();
            for (id, cid, title, sql, at) in rows.iter() {
                conn.execute(
                    "INSERT INTO favorite_queries (id, connection_id, title, sql, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, cid, title, sql, at],
                )
                .unwrap();
            }
        });

        Self { _fx: fx, data_dir }
    }

    fn favorites_root(&self) -> PathBuf {
        self.data_dir.join("favorites")
    }

    fn db(&self) -> std::sync::Arc<HistoryDb> {
        HistoryDb::open(&self.data_dir).unwrap()
    }
}

/// The acceptance criterion: switch to the file store and no old favorite
/// is missing, and it reads back exactly as it was.
#[test]
fn every_legacy_favorite_survives_the_switch_and_reads_back() {
    let install = LegacyInstall::new(&ROWS);
    let db = install.db();
    let store = FavoritesRepository::open(&install.favorites_root()).unwrap();

    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::Migrated { rows: 3 }
    );

    // Read back through the ordinary listing API, filtered per connection.
    assert_eq!(titles(&store, Some("cfg-1")).len(), 2);
    assert_eq!(titles(&store, Some("cfg-2")).len(), 1);
    assert_eq!(titles(&store, Some("never-existed")).len(), 0);
    assert_eq!(store.list(None).len(), 3);

    let mut cfg1 = titles(&store, Some("cfg-1"));
    cfg1.sort();
    assert_eq!(cfg1, ["Nightly recon", "Weekly rollup"]);

    // The SQL text survives intact, and the front-matter does not leak into
    // the statement. The only difference from the SQLite value is the
    // trailing newline every written file carries.
    let bodies: Vec<String> = store.list(None).into_iter().map(|f| f.sql).collect();
    for (_, _, _, original, _) in ROWS {
        let expected = format!("{original}\n");
        assert!(
            bodies.contains(&expected),
            "missing or altered: {original:?} in {bodies:?}"
        );
    }
    for favorite in store.list(None) {
        assert!(!favorite.sql.contains("-- title:"), "{:?}", favorite.sql);
        assert_eq!(favorite.folder, None);
        assert!(Ulid::is_valid(&favorite.id), "bad id {}", favorite.id);
    }

    // Every row kept its connection ownership — a favorite must not become
    // invisible in the panel just because it moved files.
    for favorite in store.list(Some("cfg-2")) {
        assert_eq!(favorite.title, "Other db");
        assert_eq!(favorite.connection_id, "cfg-2");
    }

    // One file per row, named by ULID.
    let files: Vec<String> = fs::read_dir(&install.favorites_root())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sql"))
        .collect();
    assert_eq!(files.len(), 3, "got {files:?}");

    // The rows are archived, not dropped — that is the rollback path.
    assert!(db.read_legacy_favorites().unwrap().is_none());
    let archived = db.with_raw_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM favorite_queries_legacy_v1", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
    });
    assert_eq!(archived, 3, "the original rows must stay recoverable");

    let marker = read_marker(&install.favorites_root()).expect("marker");
    assert_eq!(marker.version, 1);
    assert_eq!(marker.count, 3);
}

#[test]
fn a_relaunch_neither_duplicates_nor_re_exports() {
    let install = LegacyInstall::new(&ROWS);
    let root = install.favorites_root();
    let db = install.db();
    let store = FavoritesRepository::open(&root).unwrap();

    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::Migrated { rows: 3 }
    );
    // The table is no longer the live name, so the guard fires.
    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::NothingToDo
    );
    assert_eq!(store.list(None).len(), 3, "no second copy of anything");
    drop(db);
    drop(store);

    // Same after a full restart: the archive is not the live table name.
    let db2 = install.db();
    let store2 = FavoritesRepository::open(&root).unwrap();
    assert_eq!(
        migrate_legacy_favorites(&db2, &store2).unwrap(),
        MigrationOutcome::NothingToDo
    );
    assert_eq!(store2.list(None).len(), 3);
}

#[test]
fn a_retry_after_a_crash_overwrites_instead_of_duplicating() {
    // The crash the design has to survive: the export dies part way, so the
    // root already holds some of the files. Because ids are derived from
    // the row, the retry rewrites exactly those paths.
    let probe = LegacyInstall::new(&ROWS);
    let probe_store = FavoritesRepository::open(&probe.favorites_root()).unwrap();
    let probe_db = probe.db();
    migrate_legacy_favorites(&probe_db, &probe_store).unwrap();
    let ids: Vec<String> = probe_store.list(None).into_iter().map(|f| f.id).collect();
    assert_eq!(ids.len(), 3);

    // Rebuild the same install, but pre-seed one file as a half-written
    // leftover from the previous attempt.
    let install = LegacyInstall::new(&ROWS);
    let root = install.favorites_root();
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(format!("{}.sql", ids[0])),
        "-- title: half written\n-- created",
    )
    .unwrap();

    let db = install.db();
    let store = FavoritesRepository::open(&root).unwrap();
    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::Migrated { rows: 3 }
    );

    // 3 migrated + the leftover only if the retry failed to rewrite it.
    let after = store.list(None);
    assert_eq!(after.len(), 3, "a retry must not append to its own output");
    assert!(
        after.iter().any(|f| f.id == ids[0]),
        "the half-written file is the one that was rewritten"
    );
    for entry in fs::read_dir(&root).unwrap().flatten() {
        let text = fs::read_to_string(entry.path()).unwrap_or_default();
        assert!(
            !text.contains("half written"),
            "a truncated export survived at {}",
            entry.path().display()
        );
    }
}

#[test]
fn a_write_failure_leaves_the_table_for_the_next_launch() {
    let install = LegacyInstall::new(&ROWS);
    let db = install.db();
    let root = install.favorites_root();
    fs::create_dir_all(&root).unwrap();

    // Make the root unwritable. This needs a non-root test process; when the
    // suite runs as root the permission bits do not apply, so there is
    // nothing to assert and the test says so instead of passing silently.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        let store = FavoritesRepository::open(&root).unwrap();
        let failed = migrate_legacy_favorites(&db, &store);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        if failed.is_ok() {
            eprintln!("SKIPPED: running with privileges that ignore mode bits");
            assert_eq!(db.read_legacy_favorites().unwrap().unwrap().len(), 3);
            return;
        }

        // The whole point: a failed export consumes nothing.
        assert_eq!(db.read_legacy_favorites().unwrap().unwrap().len(), 3);

        // And the retry, once the volume is writable again, lands cleanly.
        let good = FavoritesRepository::open(&root).unwrap();
        assert_eq!(
            migrate_legacy_favorites(&db, &good).unwrap(),
            MigrationOutcome::Migrated { rows: 3 }
        );
        assert_eq!(good.list(None).len(), 3);
    }
}

#[test]
fn a_fresh_install_migrates_nothing_and_writes_no_marker() {
    let fx = Fixture::new();
    let data_dir = fx.path().join("appdata");
    fs::create_dir_all(&data_dir).unwrap();

    let db = HistoryDb::open(&data_dir).unwrap();
    let root = data_dir.join("favorites");
    let store = FavoritesRepository::open(&root).unwrap();
    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::NothingToDo
    );
    assert!(read_marker(&root).is_none());
    assert!(store.list(None).is_empty());
}

#[test]
fn an_empty_legacy_table_is_left_alone() {
    // A user who created and deleted every favorite still has the table.
    // Renaming it for zero rows would be churn with no upside, so the
    // export reports nothing and leaves the schema exactly as it found it.
    const NO_ROWS: [LegacyRow; 0] = [];
    let install = LegacyInstall::new(&NO_ROWS);
    let db = install.db();
    let root = install.favorites_root();
    let store = FavoritesRepository::open(&root).unwrap();

    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::NothingToDo
    );
    assert!(read_marker(&root).is_none());
    assert_eq!(
        db.read_legacy_favorites().unwrap().map(|r| r.len()),
        Some(0)
    );
    assert_eq!(fs::read_dir(&root).unwrap().flatten().count(), 0);
}

#[test]
fn migrated_ordering_matches_the_old_created_at_desc() {
    let install = LegacyInstall::new(&ROWS);
    let db = install.db();
    let store = FavoritesRepository::open(&install.favorites_root()).unwrap();
    migrate_legacy_favorites(&db, &store).unwrap();

    // The panel was ordered `ORDER BY created_at DESC`; a file store that
    // re-sorted would silently reshuffle every existing user's list.
    assert_eq!(
        titles(&store, None),
        ["Other db", "Weekly rollup", "Nightly recon"]
    );
}

/// The state the early-return in `archive_legacy_favorites_table` was written
/// for: **both** `favorite_queries` and `favorite_queries_legacy_v1` present.
///
/// How it happens in the field: the app data directory is restored from a
/// backup, or rolled back, while an archive from a completed run survives —
/// `history.sqlite` is a single file, so a partial restore is enough. At that
/// point the export is *done*; the resurrected table is stale input.
///
/// The hazard this pins down is not duplication (ids are deterministic, so a
/// re-export rewrites the same paths) but **silent clobbering**: the stale
/// rows would overwrite the user's edits and resurrect their deletions, on
/// every launch, forever.
#[test]
fn a_live_table_next_to_the_archive_is_never_re_exported() {
    let install = LegacyInstall::new(&ROWS);
    let root = install.favorites_root();
    let db = install.db();
    let store = FavoritesRepository::open(&root).unwrap();
    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::Migrated { rows: 3 }
    );
    let ids: Vec<String> = store.list(None).into_iter().map(|f| f.id).collect();
    assert_eq!(ids.len(), 3);
    drop(store);
    drop(db);

    // The user edits one favorite and deletes another, as they would.
    fs::write(
        root.join(format!("{}.sql", ids[0])),
        "-- title: my rewritten title\n-- connectionId: cfg-2\nSELECT 'mine';\n",
    )
    .unwrap();
    let deleted = root.join(format!("{}.sql", ids[1]));
    fs::remove_file(&deleted).unwrap();

    // Now the restore: the archive is still there and a live copy of the
    // *original* rows appears beside it.
    let db = install.db();
    db.with_raw_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE favorite_queries AS SELECT * FROM favorite_queries_legacy_v1;",
        )
        .unwrap();
    });
    assert!(db.legacy_favorites_archived().unwrap());

    let store = FavoritesRepository::open(&root).unwrap();
    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::NothingToDo,
        "an existing archive means the export already ran"
    );

    // The user's work is still theirs. (`list` is newest first, so the edited
    // file — the newest of the three — stays first.)
    assert_eq!(
        titles(&store, None),
        ["my rewritten title", "Nightly recon"]
    );
    assert!(
        !deleted.exists(),
        "a deleted favorite must not come back from a resurrected legacy table"
    );
    // And a second launch behaves identically — the state is stable, not a
    // one-launch accident.
    drop(store);
    let store = FavoritesRepository::open(&root).unwrap();
    assert_eq!(
        migrate_legacy_favorites(&db, &store).unwrap(),
        MigrationOutcome::NothingToDo
    );
    assert_eq!(store.list(None).len(), 2);
}
