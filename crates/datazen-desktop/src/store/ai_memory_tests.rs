//! AppDb v7 AI memory storage tests: migration, FTS trigger correctness,
//! CRUD/validation, audit, and connection-deletion cleanup.

use super::*;
use crate::store::app_db::{APP_DB_FILE, SCHEMA_VERSION};
use crate::store::AppDb;
use rusqlite::Connection;

const ORG: &str = "org-ai";
const OWNER: &str = "owner-ai";
const TS: &str = "2026-11-05T00:00:00Z";

fn note(connection_id: &str, content: &str) -> AiConnectionNoteRecord {
    AiConnectionNoteRecord {
        connection_id: connection_id.into(),
        organization_id: ORG.into(),
        owner_principal_id: OWNER.into(),
        content: content.into(),
        updated_at: TS.into(),
    }
}

fn memory(id: &str, content: &str) -> AiMemoryRecord {
    AiMemoryRecord {
        id: id.into(),
        organization_id: ORG.into(),
        owner_principal_id: OWNER.into(),
        scope_type: "user".into(),
        scope_id: OWNER.into(),
        connection_id: None,
        database_name: None,
        schema_name: None,
        table_name: None,
        column_name: None,
        kind: "preference".into(),
        subject: format!("subject-{id}"),
        content: content.into(),
        structured_json: None,
        source: "explicit".into(),
        source_ref: None,
        confidence: 0.5,
        sensitivity: "normal".into(),
        status: "active".into(),
        pinned: false,
        use_count: 0,
        last_used_at: None,
        expires_at: None,
        content_hash: format!("hash-{id}"),
        created_at: TS.into(),
        updated_at: TS.into(),
    }
}

fn audit(id: &str, memory_id: &str) -> AiMemoryAuditRecord {
    AiMemoryAuditRecord {
        id: id.into(),
        memory_id: memory_id.into(),
        action: "create".into(),
        actor_principal_id: OWNER.into(),
        before_hash: None,
        after_hash: Some("after".into()),
        detail_json: Some(r#"{"source":"test"}"#.into()),
        created_at: TS.into(),
    }
}

fn rewind_to_v6(db: &AppDb) {
    db.with_conn(|conn| {
        conn.execute_batch(
            "DROP TABLE IF EXISTS ai_session_messages;
             DROP TABLE IF EXISTS ai_sessions;
             DROP TRIGGER IF EXISTS ai_memories_ai;
             DROP TRIGGER IF EXISTS ai_memories_ad;
             DROP TRIGGER IF EXISTS ai_memories_au;
             DROP TABLE IF EXISTS ai_memories_fts;
             DROP TABLE IF EXISTS ai_memory_audit;
             DROP TABLE IF EXISTS ai_memories;
             DROP TABLE IF EXISTS ai_connection_notes;
             DELETE FROM schema_migrations WHERE version >= 7;",
        )?;
        Ok(())
    })
    .unwrap();
}

fn schema_version(db: &AppDb) -> i32 {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?)
    })
    .unwrap()
}

fn object_count(db: &AppDb, kind: &str, name: &str) -> i64 {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type=?1 AND name=?2",
            rusqlite::params![kind, name],
            |row| row.get(0),
        )?)
    })
    .unwrap()
}

#[test]
fn v6_database_upgrades_to_latest_and_reopens_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let db = AppDb::open(temp.path()).unwrap();
    rewind_to_v6(&db);
    drop(db);

    let db = AppDb::open(temp.path()).expect("v6 database upgrades to the latest schema");
    assert_eq!(schema_version(&db), 8);
    assert_eq!(schema_version(&db), SCHEMA_VERSION);
    for table in [
        "ai_connection_notes",
        "ai_memories",
        "ai_memories_fts",
        "ai_memory_audit",
    ] {
        assert_eq!(object_count(&db, "table", table), 1, "missing {table}");
    }
    for index in [
        "idx_ai_memories_scope",
        "idx_ai_memories_kind",
        "idx_ai_memories_last_used",
    ] {
        assert_eq!(object_count(&db, "index", index), 1, "missing {index}");
    }
    for trigger in ["ai_memories_ai", "ai_memories_ad", "ai_memories_au"] {
        assert_eq!(
            object_count(&db, "trigger", trigger),
            1,
            "missing {trigger}"
        );
    }
    drop(db);

    let db = AppDb::open(temp.path()).expect("reopening a v8 database is a no-op");
    assert_eq!(schema_version(&db), 8);
    let version_rows: i64 = db
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version=7",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(version_rows, 1);
}

#[test]
fn v7_migration_failure_rolls_back_the_whole_ddl_batch() {
    let temp = tempfile::tempdir().unwrap();
    let db = AppDb::open(temp.path()).unwrap();
    rewind_to_v6(&db);
    // A wrong-shaped ai_memories_fts makes the v7 batch fail after the three
    // ordinary tables were created. The transaction must roll those back.
    db.with_conn(|conn| {
        conn.execute("CREATE TABLE ai_memories_fts (bad TEXT)", [])?;
        Ok(())
    })
    .unwrap();
    drop(db);

    assert!(AppDb::open(temp.path()).is_err());
    let raw = Connection::open(temp.path().join(APP_DB_FILE)).unwrap();
    let version: i32 = raw
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 6, "failed v7 migration must not advance version");
    for table in ["ai_connection_notes", "ai_memories", "ai_memory_audit"] {
        let created: i64 = raw
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(created, 0, "failed v7 migration leaked {table}");
    }
}

#[test]
fn notes_upsert_read_and_delete() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_connection_note(&note("c1", "first")).unwrap();
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c1")
            .unwrap()
            .unwrap()
            .content,
        "first"
    );

    let mut updated = note("c1", "second");
    updated.updated_at = "2026-11-05T01:00:00Z".into();
    db.upsert_ai_connection_note(&updated).unwrap();
    let got = db
        .get_ai_connection_note(ORG, OWNER, "c1")
        .unwrap()
        .unwrap();
    assert_eq!(got.content, "second");
    assert_eq!(got.updated_at, "2026-11-05T01:00:00Z");

    assert!(db.delete_ai_connection_note(ORG, OWNER, "c1").unwrap());
    assert!(db
        .get_ai_connection_note(ORG, OWNER, "c1")
        .unwrap()
        .is_none());
    assert!(!db.delete_ai_connection_note(ORG, OWNER, "missing").unwrap());
}

#[test]
fn notes_are_scoped_to_organization_and_owner() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_connection_note(&note("c1", "private-note"))
        .unwrap();

    assert!(db
        .get_ai_connection_note("other-org", OWNER, "c1")
        .unwrap()
        .is_none());
    assert!(db
        .get_ai_connection_note(ORG, "other-owner", "c1")
        .unwrap()
        .is_none());
    assert!(!db
        .delete_ai_connection_note("other-org", OWNER, "c1")
        .unwrap());
    assert!(!db
        .delete_ai_connection_note(ORG, "other-owner", "c1")
        .unwrap());
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c1")
            .unwrap()
            .unwrap()
            .content,
        "private-note"
    );

    let rehome = db.set_ai_connection_note("other-org", OWNER, "c1", "takeover");
    assert!(matches!(rehome, Err(AppDbError::Validation(_))));
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c1")
            .unwrap()
            .unwrap()
            .content,
        "private-note"
    );
}

#[test]
fn note_set_enforces_4kb_and_empty_content_deletes() {
    let db = AppDb::open_in_memory().unwrap();
    db.set_ai_connection_note(ORG, OWNER, "c1", "hello")
        .unwrap();
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c1")
            .unwrap()
            .unwrap()
            .content,
        "hello"
    );

    let too_long = "a".repeat(datazen::ai::MAX_CONNECTION_NOTES_BYTES + 1);
    assert!(matches!(
        db.set_ai_connection_note(ORG, OWNER, "c1", &too_long),
        Err(AppDbError::Validation(_))
    ));
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c1")
            .unwrap()
            .unwrap()
            .content,
        "hello",
        "rejected writes leave the previous note intact"
    );

    let at_limit = "a".repeat(datazen::ai::MAX_CONNECTION_NOTES_BYTES);
    db.set_ai_connection_note(ORG, OWNER, "c1", &at_limit)
        .unwrap();
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c1")
            .unwrap()
            .unwrap()
            .content
            .len(),
        datazen::ai::MAX_CONNECTION_NOTES_BYTES
    );

    db.set_ai_connection_note(ORG, OWNER, "c1", "   \n")
        .unwrap();
    assert!(db
        .get_ai_connection_note(ORG, OWNER, "c1")
        .unwrap()
        .is_none());
}

#[test]
fn connection_context_counts_notes_and_memories_by_scope() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_connection_note(&note("c1", "note")).unwrap();

    let mut by_id = memory("m-by-id", "one");
    by_id.connection_id = Some("c1".into());
    by_id.scope_type = "connection".into();
    by_id.scope_id = "c1".into();
    db.upsert_ai_memory(&by_id).unwrap();

    let mut by_scope = memory("m-by-scope", "two");
    by_scope.scope_type = "connection".into();
    by_scope.scope_id = "c1".into();
    db.upsert_ai_memory(&by_scope).unwrap();

    let mut other = memory("m-other", "three");
    other.connection_id = Some("c2".into());
    other.scope_type = "connection".into();
    other.scope_id = "c2".into();
    db.upsert_ai_memory(&other).unwrap();

    let counts = db.count_ai_connection_context(ORG, OWNER, "c1").unwrap();
    assert_eq!(counts.note_count, 1);
    assert_eq!(counts.memory_count, 2);
    let json = serde_json::to_value(counts).unwrap();
    assert_eq!(json["noteCount"], 1);
    assert_eq!(json["memoryCount"], 2);

    let counts = db
        .count_ai_connection_context(ORG, OWNER, "missing")
        .unwrap();
    assert_eq!(counts.note_count, 0);
    assert_eq!(counts.memory_count, 0);

    let counts = db
        .count_ai_connection_context("other-org", OWNER, "c1")
        .unwrap();
    assert_eq!(counts.note_count, 0);
    assert_eq!(counts.memory_count, 0);
}

#[test]
fn memory_crud_is_scoped_to_organization_and_owner() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_memory(&memory("m1", "one")).unwrap();
    assert_eq!(
        db.get_ai_memory(ORG, OWNER, "m1").unwrap().unwrap().content,
        "one"
    );
    assert!(db
        .get_ai_memory("other-org", OWNER, "m1")
        .unwrap()
        .is_none());
    assert!(db
        .get_ai_memory(ORG, "other-owner", "m1")
        .unwrap()
        .is_none());
    assert_eq!(db.list_ai_memories(ORG, OWNER).unwrap().len(), 1);
    assert!(db.list_ai_memories("other-org", OWNER).unwrap().is_empty());

    let mut updated = memory("m1", "two");
    updated.updated_at = "2026-11-05T02:00:00Z".into();
    db.upsert_ai_memory(&updated).unwrap();
    assert_eq!(db.list_ai_memories(ORG, OWNER).unwrap()[0].content, "two");

    assert!(!db.delete_ai_memory("other-org", OWNER, "m1").unwrap());
    assert!(db.delete_ai_memory(ORG, OWNER, "m1").unwrap());
    assert!(db.get_ai_memory(ORG, OWNER, "m1").unwrap().is_none());
}

#[test]
fn fts_search_is_immediate_for_english_and_chinese_tokens() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_memory(&memory("m-en", "Customer prefers PostgreSQL"))
        .unwrap();
    db.upsert_ai_memory(&memory("m-zh", "客户 偏好 深色 主题"))
        .unwrap();

    let english = db.search_ai_memories(ORG, OWNER, "PostgreSQL", 10).unwrap();
    assert_eq!(english.len(), 1);
    assert_eq!(english[0].id, "m-en");

    let chinese = db.search_ai_memories(ORG, OWNER, "深色", 10).unwrap();
    assert_eq!(chinese.len(), 1);
    assert_eq!(chinese[0].id, "m-zh");

    // The FTS table is external-content: deleting the memory must delete its
    // index entry through ai_memories_ad, not leave a stale hit.
    assert!(db.delete_ai_memory(ORG, OWNER, "m-zh").unwrap());
    assert!(db
        .search_ai_memories(ORG, OWNER, "深色", 10)
        .unwrap()
        .is_empty());
}

#[test]
fn fts_update_replaces_the_old_index_entry() {
    let db = AppDb::open_in_memory().unwrap();
    let mut entry = memory("m1", "oldtoken only");
    db.upsert_ai_memory(&entry).unwrap();
    assert_eq!(
        db.search_ai_memories(ORG, OWNER, "oldtoken", 10)
            .unwrap()
            .len(),
        1
    );

    entry.content = "newtoken only".into();
    entry.updated_at = "2026-11-05T03:00:00Z".into();
    db.upsert_ai_memory(&entry).unwrap();
    assert!(db
        .search_ai_memories(ORG, OWNER, "oldtoken", 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        db.search_ai_memories(ORG, OWNER, "newtoken", 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn invalid_enums_and_confidence_are_rejected_before_write() {
    let db = AppDb::open_in_memory().unwrap();
    for (field, value) in [
        ("scope_type", "bogus"),
        ("kind", "bogus"),
        ("source", "bogus"),
        ("status", "bogus"),
        ("sensitivity", "bogus"),
    ] {
        let mut entry = memory("m-invalid", "content");
        match field {
            "scope_type" => entry.scope_type = value.into(),
            "kind" => entry.kind = value.into(),
            "source" => entry.source = value.into(),
            "status" => entry.status = value.into(),
            "sensitivity" => entry.sensitivity = value.into(),
            _ => unreachable!(),
        }
        assert!(
            matches!(db.upsert_ai_memory(&entry), Err(AppDbError::Validation(_))),
            "{field} must be validated"
        );
    }
    let mut entry = memory("m-confidence", "content");
    for confidence in [-0.1, 1.1, f64::NAN] {
        entry.confidence = confidence;
        assert!(matches!(
            db.upsert_ai_memory(&entry),
            Err(AppDbError::Validation(_))
        ));
    }
    assert!(db
        .get_ai_memory(ORG, OWNER, "m-confidence")
        .unwrap()
        .is_none());
}

#[test]
fn secret_memories_are_rejected_at_the_storage_boundary() {
    let db = AppDb::open_in_memory().unwrap();
    let mut entry = memory("m-secret", "do not persist");
    entry.sensitivity = "secret".into();
    let error = db.upsert_ai_memory(&entry).unwrap_err();
    assert!(matches!(error, AppDbError::Validation(ref message) if message.contains("secret")));
    assert!(db.get_ai_memory(ORG, OWNER, "m-secret").unwrap().is_none());
}

#[test]
fn audit_write_and_list_validate_action() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_memory(&memory("m1", "content")).unwrap();
    db.append_ai_memory_audit(&audit("a1", "m1")).unwrap();
    let rows = db.list_ai_memory_audit("m1").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action, "create");
    assert_eq!(rows[0].after_hash.as_deref(), Some("after"));

    let mut bad = audit("a2", "m1");
    bad.action = "bogus".into();
    assert!(matches!(
        db.append_ai_memory_audit(&bad),
        Err(AppDbError::Validation(_))
    ));
}

#[test]
fn connection_purge_removes_notes_memories_fts_and_audit_for_target_only() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_connection_note(&note("c1", "note-c1"))
        .unwrap();
    db.upsert_ai_connection_note(&note("c2", "note-c2"))
        .unwrap();

    let mut c1 = memory("m-c1", "c1token");
    c1.connection_id = Some("c1".into());
    c1.scope_type = "connection".into();
    c1.scope_id = "c1".into();
    db.upsert_ai_memory(&c1).unwrap();

    // connection_id may be NULL for a connection-scoped memory; the cleanup
    // predicate still catches it by (scope_type, scope_id).
    let mut c1_scope_only = memory("m-c1-scope", "c1scopetoken");
    c1_scope_only.scope_type = "connection".into();
    c1_scope_only.scope_id = "c1".into();
    db.upsert_ai_memory(&c1_scope_only).unwrap();

    let mut c2 = memory("m-c2", "c2token");
    c2.connection_id = Some("c2".into());
    c2.scope_type = "connection".into();
    c2.scope_id = "c2".into();
    db.upsert_ai_memory(&c2).unwrap();

    db.append_ai_memory_audit(&audit("a-c1", "m-c1")).unwrap();
    db.append_ai_memory_audit(&audit("a-c1-scope", "m-c1-scope"))
        .unwrap();
    db.append_ai_memory_audit(&audit("a-c2", "m-c2")).unwrap();

    db.purge_connection_ai_data("c1", ORG).unwrap();

    assert!(db
        .get_ai_connection_note(ORG, OWNER, "c1")
        .unwrap()
        .is_none());
    assert_eq!(
        db.get_ai_connection_note(ORG, OWNER, "c2")
            .unwrap()
            .unwrap()
            .content,
        "note-c2"
    );
    assert!(db.get_ai_memory(ORG, OWNER, "m-c1").unwrap().is_none());
    assert!(db
        .get_ai_memory(ORG, OWNER, "m-c1-scope")
        .unwrap()
        .is_none());
    assert!(db.get_ai_memory(ORG, OWNER, "m-c2").unwrap().is_some());
    assert!(db.list_ai_memory_audit("m-c1").unwrap().is_empty());
    assert!(db.list_ai_memory_audit("m-c1-scope").unwrap().is_empty());
    assert_eq!(db.list_ai_memory_audit("m-c2").unwrap().len(), 1);
    assert!(db
        .search_ai_memories(ORG, OWNER, "c1token", 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        db.search_ai_memories(ORG, OWNER, "c2token", 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn existing_memory_cannot_be_rehomed_to_another_owner_or_org() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_memory(&memory("m-owned", "original")).unwrap();

    let mut other_owner = memory("m-owned", "owner-hijack");
    other_owner.owner_principal_id = "other-owner".into();
    assert!(matches!(
        db.upsert_ai_memory(&other_owner),
        Err(AppDbError::Validation(_))
    ));

    let mut other_org = memory("m-owned", "org-hijack");
    other_org.organization_id = "other-org".into();
    assert!(matches!(
        db.upsert_ai_memory(&other_org),
        Err(AppDbError::Validation(_))
    ));

    let original = db.get_ai_memory(ORG, OWNER, "m-owned").unwrap().unwrap();
    assert_eq!(original.content, "original");
    assert_eq!(original.organization_id, ORG);
    assert_eq!(original.owner_principal_id, OWNER);
    assert!(db
        .get_ai_memory(ORG, "other-owner", "m-owned")
        .unwrap()
        .is_none());
    assert!(db
        .get_ai_memory("other-org", OWNER, "m-owned")
        .unwrap()
        .is_none());
}
