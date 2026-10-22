//! File-first favorites storage, exercised against real directories.
//!
//! These tests use `tempfile` rather than mocks on purpose: the whole point of
//! the §2.6 change is the shape of the bytes on disk and how they survive a
//! restart, and a mock directory proves neither.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};

use super::ulid::Ulid;
use super::*;

/// A temp directory that is *not* auto-emptied, so a failure can be inspected
/// before `TempDir` cleans it up.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn root(&self) -> PathBuf {
        self.path().join("favorites")
    }

    fn store(&self) -> FavoritesRepository {
        FavoritesRepository::open(&self.root()).unwrap()
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}

fn draft(connection: &str, title: &str, sql: &str) -> NewFavorite {
    NewFavorite {
        connection_id: connection.to_string(),
        title: title.to_string(),
        sql: sql.to_string(),
        database: None,
        keyword: None,
    }
}

fn listed_first(store: &FavoritesRepository) -> FavoriteQuery {
    store.list(None).into_iter().next().unwrap()
}

fn titles(store: &FavoritesRepository, connection: Option<&str>) -> Vec<String> {
    store
        .list(connection)
        .into_iter()
        .map(|f| f.title)
        .collect()
}

// ── Round trip ─────────────────────────────────────────────────────────────

#[test]
fn add_writes_a_runnable_file_and_list_reads_it_back() {
    let fx = Fixture::new();
    let store = fx.store();

    let saved = store
        .add(draft("cfg-1", "Nightly recon", "SELECT * FROM orders;"))
        .unwrap();

    assert!(Ulid::is_valid(&saved.id));
    let file = fx.root().join(format!("{}.sql", saved.id));
    let text = fs::read_to_string(&file).unwrap();

    // A valid statement with comments on top, not a JSON blob. The key order
    // is the file's own, not alphabetical — a re-save must not churn the diff.
    assert_eq!(
        text,
        format!(
            "-- title: Nightly recon\n\
             -- connectionId: cfg-1\n\
             -- createdAt: {}\n\
             -- updatedAt: {}\n\
             SELECT * FROM orders;\n",
            saved.created_at.to_rfc3339(),
            saved.created_at.to_rfc3339(),
        )
    );

    // And it reads back through the public API, not by re-reading the file.
    let listed = store.list(Some("cfg-1"));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Nightly recon");
    assert_eq!(
        listed[0].sql, "SELECT * FROM orders;\n",
        "the writer's one trailing newline is the whole of the normalization"
    );
    assert_eq!(listed[0].id, saved.id);
    assert_eq!(listed[0].updated_at, Some(saved.created_at));
}

#[test]
fn a_renamed_title_does_not_rename_the_file() {
    // The §2.6.2 argument: a ULID file name is an identity, not a label.
    let fx = Fixture::new();
    let store = fx.store();
    let saved = store.add(draft("c", "Before", "SELECT 1;")).unwrap();

    let file = fx.root().join(format!("{}.sql", saved.id));
    let mut favorite = listed_first(&store);
    favorite.title = "After".into();
    write_atomic(
        &file,
        FavoritesRepository::render_file(&favorite).as_bytes(),
    )
    .unwrap();

    // Re-read from disk with a cold store — the file is the source of truth,
    // and the id, not the title, is what names it.
    let reopened = fx.store();
    assert_eq!(listed_first(&reopened).title, "After");
    assert!(file.exists());
    assert_eq!(
        fs::read_dir(&fx.root()).unwrap().flatten().count(),
        1,
        "no second file was created for the new title"
    );
}

#[test]
fn ordering_is_newest_first() {
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "01ARZ3NDEKTSV4RRFFQ69G5FAV.sql",
        "-- title: oldest\n-- connectionId: c\n-- createdAt: 2020-01-01T00:00:00Z\nSELECT 1;",
    );
    fx.write(
        "01J8XK2M9Q7B4F.sql",
        "-- title: newest\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\nSELECT 2;",
    );
    fx.write(
        "01J8XK4T7B2C9D.sql",
        "-- title: middle\n-- connectionId: c\n-- createdAt: 2023-01-01T00:00:00Z\nSELECT 3;",
    );

    assert_eq!(titles(&store, None), ["newest", "middle", "oldest"]);
}

#[test]
fn listing_filters_by_connection() {
    let fx = Fixture::new();
    let store = fx.store();
    store.add(draft("cfg-1", "One", "SELECT 1;")).unwrap();
    store.add(draft("cfg-1", "Two", "SELECT 2;")).unwrap();
    store.add(draft("cfg-2", "Three", "SELECT 3;")).unwrap();

    assert_eq!(titles(&store, Some("cfg-1")).len(), 2);
    assert_eq!(titles(&store, Some("cfg-2")).len(), 1);
    assert_eq!(titles(&store, Some("cfg-nonexistent")).len(), 0);
    assert_eq!(titles(&store, None).len(), 3);
}

// ── Directory layout ───────────────────────────────────────────────────────

#[test]
fn nested_folders_are_scanned_recursively_and_reported() {
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "root.sql",
        "-- title: root\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\nSELECT 1;",
    );
    fx.write(
        "orders/a.sql",
        "-- title: nested\n-- connectionId: c\n-- createdAt: 2026-01-02T00:00:00Z\nSELECT 2;",
    );
    fx.write(
        "orders/archive/old.sql",
        "-- title: deep\n-- connectionId: c\n-- createdAt: 2026-01-03T00:00:00Z\nSELECT 3;",
    );

    let all = store.list(None);
    assert_eq!(all.len(), 3, "a nested favorite must not be invisible");
    let folder_of = |title: &str| {
        all.iter()
            .find(|f| f.title == title)
            .unwrap()
            .folder
            .clone()
    };
    assert_eq!(folder_of("root"), None);
    assert_eq!(folder_of("nested").as_deref(), Some("orders"));
    assert_eq!(folder_of("deep").as_deref(), Some("orders/archive"));
}

#[test]
fn the_trash_directory_is_never_listed() {
    let fx = Fixture::new();
    let store = fx.store();
    let saved = store.add(draft("c", "Doomed", "SELECT 1;")).unwrap();
    store.delete(&saved.id).unwrap();
    store.add(draft("c", "Kept", "SELECT 2;")).unwrap();

    assert_eq!(titles(&store, None), ["Kept"]);
}

#[test]
fn only_sql_files_are_loaded() {
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "ok.sql",
        "-- title: sql\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\nSELECT 1;",
    );
    fx.write("notes.md", "not a favorite");
    fx.write("script.sh", "echo hi");
    fx.write("data.json", "{}");

    assert_eq!(
        titles(&store, None),
        ["sql"],
        "extension allow-list must hold"
    );
}

#[test]
fn a_symlink_is_not_followed() {
    // A link pointing at an ancestor would loop the walk forever; one pointing
    // outside would read files the user never put in their favorites folder.
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "real.sql",
        "-- title: real\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\nSELECT 1;",
    );

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(fx.path(), fx.root().join("loop")).unwrap();
        fs::write(fx.path().join("outside.sql"), "SELECT secret;").unwrap();
        std::os::unix::fs::symlink(fx.path().join("outside.sql"), fx.root().join("out.sql"))
            .unwrap();
    }

    assert_eq!(titles(&store, None), ["real"]);
}

#[test]
fn a_file_without_front_matter_still_loads_verbatim() {
    // A user who drops a hand-written query in the folder owns it; we must not
    // mangle it or hide it.
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "hand.sql",
        "-- exports nightly\nSELECT count(*) FROM users;\n",
    );

    let listed = store.list(None);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].sql,
        "-- exports nightly\nSELECT count(*) FROM users;\n"
    );
    assert_eq!(listed[0].title, "SELECT count(*) FROM users;");
    assert_eq!(listed[0].connection_id, "", "unowned, not misfiled");
}
#[test]
fn creation_time_falls_back_to_the_ulid_then_the_file() {
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "01ARZ3NDEKTSV4RRFFQ69G5FAV.sql",
        "-- title: no date\nSELECT 1;",
    );
    fx.write("hand-written.sql", "SELECT 2;");

    let all = store.list(None);
    let by_ulid = all
        .iter()
        .find(|f| f.id == "01ARZ3NDEKTSV4RRFFQ69G5FAV")
        .unwrap();
    assert_eq!(by_ulid.created_at.timestamp_millis(), 1_469_922_850_259);

    let by_mtime = all.iter().find(|f| f.id == "hand-written").unwrap();
    assert!(
        by_mtime.created_at > Utc::now() - Duration::minutes(5),
        "falls back to mtime, got {:?}",
        by_mtime.created_at
    );
}

#[test]
fn optional_front_matter_fields_are_read() {
    // Forward compatibility: the keyword / database fields the plan's §2.5
    // tracks will use are part of the format now, so the files those tracks
    // write will load without a second format change.
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "f.sql",
        "-- title: T\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\n\
         -- updatedAt: 2026-02-01T00:00:00Z\n-- keyword: recon\n-- database: analytics\nSELECT 1;",
    );

    let favorite = listed_first(&store);
    assert_eq!(favorite.keyword.as_deref(), Some("recon"));
    assert_eq!(favorite.database.as_deref(), Some("analytics"));
    assert_eq!(
        favorite.updated_at.map(|t| t.to_rfc3339()),
        Some("2026-02-01T00:00:00+00:00".to_string())
    );
}

// ── Mutation ───────────────────────────────────────────────────────────────

#[test]
fn delete_moves_the_file_into_trash_and_keeps_the_sql() {
    let fx = Fixture::new();
    let store = fx.store();
    let saved = store.add(draft("c", "Precious", "SELECT 'rare';")).unwrap();

    store.delete(&saved.id).unwrap();

    assert!(store.list(None).is_empty());
    let trash: Vec<PathBuf> = fs::read_dir(fx.root().join(".trash"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(trash.len(), 1);
    let name = trash[0].file_name().unwrap().to_str().unwrap().to_string();
    assert!(
        name.ends_with(&format!("__{}.sql", saved.id)),
        "got: {name}"
    );
    assert!(
        name.starts_with("20"),
        "trash name starts with the time: {name}"
    );
    assert!(fs::read_to_string(&trash[0])
        .unwrap()
        .contains("SELECT 'rare';"));
}

#[test]
fn deleting_a_missing_favorite_reports_not_found() {
    let fx = Fixture::new();
    let store = fx.store();
    match store.delete("01ARZ3NDEKTSV4RRFFQ69G5FAV") {
        Err(FavoritesError::NotFound(id)) => assert_eq!(id, "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn a_traversal_id_cannot_escape_the_root() {
    let fx = Fixture::new();
    let store = fx.store();
    for hostile in [
        "../../etc/passwd",
        "..",
        ".",
        "a/b",
        "a\\b",
        "",
        "with space",
        "nul\0byte",
        "CON",
    ] {
        assert!(
            matches!(store.delete(hostile), Err(FavoritesError::UnsafeId(_))),
            "id {hostile:?} must be rejected"
        );
    }
    // A favorite the user renamed in Finder stays editable, because the
    // allow-list is a superset of the ULIDs the app itself mints.
    assert!(store.resolve_file("my-renamed-favorite").is_ok());
    assert!(store.resolve_file("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
}

#[test]
fn the_cache_reflects_app_writes_without_a_rescan() {
    let fx = Fixture::new();
    let store = fx.store();
    assert!(store.list(None).is_empty(), "warms the cache as empty");

    let saved = store.add(draft("c", "New", "SELECT 1;")).unwrap();
    assert_eq!(store.list(None).len(), 1);

    store.delete(&saved.id).unwrap();
    assert!(store.list(None).is_empty());
}

#[test]
fn an_out_of_band_file_needs_an_explicit_refresh() {
    // The cache exists for speed, so a folder arriving from a sync client is
    // invisible until something says to look again. The UI calls this on focus.
    let fx = Fixture::new();
    let store = fx.store();
    assert!(store.list(None).is_empty());

    fx.write(
        "synced.sql",
        "-- title: from cloud\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\nSELECT 1;",
    );
    assert!(
        store.list(None).is_empty(),
        "cached listing does not re-read"
    );

    store.invalidate_cache();
    assert_eq!(store.list(None).len(), 1);
}

#[test]
fn set_root_moves_the_whole_universe() {
    let fx = Fixture::new();
    let store = fx.store();
    store.add(draft("c", "In old", "SELECT 1;")).unwrap();
    assert_eq!(store.list(None).len(), 1);

    let other = fx.path().join("synced-favorites");
    store.set_root(&other).unwrap();

    assert_eq!(store.root(), other);
    assert!(other.is_dir(), "set_root creates the directory");
    assert!(store.list(None).is_empty(), "the new tree is what counts");
    // The old favorite is still on disk, just not ours any more.
    assert_eq!(titles(&fx.store(), None), ["In old"]);

    fs::write(
        other.join("01ARZ3NDEKTSV4RRFFQ69G5FAV.sql"),
        "-- title: In new\n-- connectionId: c\n-- createdAt: 2026-01-01T00:00:00Z\nSELECT 2;\n",
    )
    .unwrap();
    // A folder that appeared under a synced root needs the refresh the UI
    // performs on focus; the cache is not a watcher.
    store.invalidate_cache();
    assert_eq!(titles(&store, None), ["In new"]);
}

// ── Path safety ────────────────────────────────────────────────────────────

/// The whole allow-list, as an accept/reject table.
///
/// `resolve_file` is the only place an id becomes a path, so this function is
/// the traversal gate. Both halves are asserted: the gate rejects what it must
/// (reaching a `UnsafeId`, never a `NotFound`, which is what "rejected by the
/// name check" looks like) and it accepts what the app itself mints, plus a
/// user-renamed file (the reason the set is wider than a bare ULID check).
#[test]
fn the_name_gate_accepts_only_a_strict_stem() {
    // Over the 64-byte cap. (Bytes, not chars — deliberately, so a multi-byte
    // name cannot smuggle a long path past the bound.)
    let at_cap = "a".repeat(64);
    let over_cap = "a".repeat(65);
    let over_cap_suffixed = format!("{}suffix", at_cap);

    // Rejected. Each of these either escapes the root, names a directory the
    // user never asked for, or cannot be represented as a file name at all.
    for hostile in [
        // Traversal, absolute paths, and mixed separators.
        "..",
        "../..",
        "../../etc/passwd",
        "..%2F..%2Fetc",
        "a/../b",
        "a/b",
        "a\\b",
        "/etc/passwd",
        "/tmp/x",
        "C:\\Windows\\system32",
        "\\??\\C:\\x",
        // The current directory, and the two names that mean "no name".
        ".",
        "",
        "   ",
        // Windows device names, in the casing Windows itself accepts and in
        // the casing a user is likely to type.
        "CON",
        "con",
        "CoN",
        "PRN",
        "AUX",
        "NUL",
        "COM1",
        "com9",
        "LPT1",
        "lpt9",
        // Characters outside ASCII alphanumerics + `-` + `_`.
        "with space",
        "with\ttab",
        "with\nnewline",
        "nul\0byte",
        ".hidden",
        "trailing.",
        "a.sql",
        "emoji😀",
        // Homoglyphs: a Cyrillic "а" and a full-width "Ａ" both render like
        // ASCII, and both are outside the ASCII allow-list.
        "pаypal",
        "ＡＢＣ",
        // Over the 64-byte cap. (Bytes, not chars — deliberately, so a
        // multi-byte name cannot smuggle a long path past the bound.)
        over_cap.as_str(),
        over_cap_suffixed.as_str(),
    ] {
        assert!(
            !is_safe_stem(hostile),
            "must be rejected as a name: {hostile:?}"
        );
    }

    // Accepted: the app's own ids, a user-renamed stem, and the exact boundary.
    for safe in [
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "my-favorite",
        "my_favorite",
        "My_Favorite-2",
        "0",
        "a",
        "-",
        "_",
        at_cap.as_str(),
    ] {
        assert!(is_safe_stem(safe), "must be accepted as a name: {safe:?}");
    }
}

/// The gate has to be reachable from the public API and it has to hold for a
/// user who is not calling an internal function: a hostile id passed to
/// `delete` is refused, and nothing lands outside — or inside — the root.
#[test]
fn a_hostile_id_is_refused_before_any_path_is_touched() {
    let fx = Fixture::new();
    let store = fx.store();
    // A real favorite, so "refused" cannot be confused with "nothing there".
    let saved = store.add(draft("c", "Real", "SELECT 1;")).unwrap();

    // A canary outside the root, to prove no probe ever left the sandbox.
    let outside = fx.path().join("outside.sql");
    fs::write(&outside, b"canary").unwrap();
    let outside_before = fs::metadata(&outside).unwrap().modified().unwrap();

    let over_cap = "a".repeat(65);
    for hostile in [
        "..",
        "../..",
        "../../etc/passwd",
        "/etc/passwd",
        "a/b",
        "a\\b",
        "CON",
        "con",
        ".hidden",
        "nul\0byte",
        over_cap.as_str(),
    ] {
        let err = store
            .delete(hostile)
            .expect_err("a hostile id must not be deletable");
        assert!(
            matches!(err, FavoritesError::UnsafeId(ref id) if id.as_str() == hostile),
            "{hostile:?} must fail the name check, got {err:?}"
        );
    }

    // The canary is untouched, and the root holds only the one real favorite —
    // no `.sql` file, no stray directory, nothing created by a rejected call.
    assert_eq!(fs::read(&outside).unwrap(), b"canary");
    assert_eq!(
        fs::metadata(&outside).unwrap().modified().unwrap(),
        outside_before
    );
    let entries: Vec<String> = fs::read_dir(fx.root())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, [format!("{}.sql", saved.id)]);

    // A well-formed id that is simply absent is a *different* error: the gate
    // let it through and the lookup is what failed.
    assert!(matches!(
        store.delete("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        Err(FavoritesError::NotFound(_))
    ));
    // …and the real favorite is still deletable, so the gate is not simply
    // refusing everything.
    store.delete(&saved.id).unwrap();
    assert!(store.list(None).is_empty());
}

/// A root that is a file, not a directory, is the one configuration mistake
/// worth naming in a log — and it is reported as such rather than as a
/// generic IO failure.
#[test]
fn a_root_that_is_a_file_is_reported_as_such() {
    let fx = Fixture::new();
    let not_a_dir = fx.path().join("favorites");
    fs::write(&not_a_dir, b"a leftover from the JSON store").unwrap();

    assert!(matches!(
        FavoritesRepository::open(&not_a_dir),
        Err(FavoritesError::RootNotADirectory(_))
    ));
    let good = fx.path().join("elsewhere");
    let store = FavoritesRepository::open(&good).unwrap();
    assert!(matches!(
        store.set_root(&not_a_dir),
        Err(FavoritesError::RootNotADirectory(_))
    ));
    // A refused repoint leaves the old root in place, and the store still works.
    assert_eq!(store.root(), good);
    assert!(store
        .add(draft("c", "Still writing here", "SELECT 1;"))
        .is_ok());
}

#[test]
fn a_failed_write_leaves_no_temp_file_behind() {
    let fx = Fixture::new();
    // A directory where a file must go: the rename cannot succeed.
    let blocked = fx.root().join("blocked");
    fs::create_dir_all(&blocked).unwrap();
    assert!(write_atomic(&blocked, b"x").is_err());

    let leftovers: Vec<String> = fs::read_dir(&blocked)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files must be cleaned up: {leftovers:?}"
    );
}

// ── Migration from the retired `favorite_queries` table ────────────────────

mod executability;
mod migration;
