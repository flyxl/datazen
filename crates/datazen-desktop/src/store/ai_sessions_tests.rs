//! AppDb v8 AI session storage tests: migration, CRUD, replace/append,
//! scoping, cascade deletion, and export prerequisites.

use super::*;
use crate::store::app_db::{APP_DB_FILE, SCHEMA_VERSION};
use rusqlite::Connection;

const ORG: &str = "org-session";
const OWNER: &str = "owner-session";
const TS: &str = "2026-11-06T00:00:00Z";

fn session(id: &str, key: &str, database: &str) -> AiSessionRecord {
    AiSessionRecord {
        id: id.into(),
        organization_id: ORG.into(),
        owner_principal_id: OWNER.into(),
        session_key: key.into(),
        connection_id: Some("conn-1".into()),
        database_name: Some(database.into()),
        title: format!("title-{id}"),
        metadata_json: Some(r#"{"templateId":"default"}"#.into()),
        created_at: TS.into(),
        updated_at: TS.into(),
        last_access_at: TS.into(),
    }
}

fn message(id: &str, content: &str, status: &str) -> AiSessionMessageRecord {
    AiSessionMessageRecord {
        id: id.into(),
        session_id: "session-1".into(),
        position: 0,
        role: if status == "complete" {
            "user"
        } else {
            "assistant"
        }
        .into(),
        content: content.into(),
        status: status.into(),
        metadata_json: Some(r#"{"memoryUsed":[{"id":"m1"}]}"#.into()),
        created_at: TS.into(),
    }
}

fn rewind_to_v7(db: &AppDb) {
    db.with_conn(|conn| {
        conn.execute_batch(
            "DROP TABLE IF EXISTS ai_session_messages;
             DROP TABLE IF EXISTS ai_sessions;
             DELETE FROM schema_migrations WHERE version >= 8;",
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
fn v7_database_upgrades_to_v8_and_reopens_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let db = AppDb::open(temp.path()).unwrap();
    rewind_to_v7(&db);
    drop(db);

    let db = AppDb::open(temp.path()).expect("v7 database upgrades to v8");
    assert_eq!(schema_version(&db), 8);
    assert_eq!(schema_version(&db), SCHEMA_VERSION);
    for table in ["ai_sessions", "ai_session_messages"] {
        assert_eq!(object_count(&db, "table", table), 1, "missing {table}");
    }
    for index in [
        "idx_ai_sessions_scope",
        "idx_ai_sessions_key",
        "idx_ai_session_messages_session",
    ] {
        assert_eq!(object_count(&db, "index", index), 1, "missing {index}");
    }
    drop(db);

    let db = AppDb::open(temp.path()).expect("reopening a v8 database is a no-op");
    assert_eq!(schema_version(&db), 8);
    let version_rows: i64 = db
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version=8",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(version_rows, 1);
}

#[test]
fn v8_migration_failure_rolls_back_the_whole_ddl_batch() {
    let temp = tempfile::tempdir().unwrap();
    let db = AppDb::open(temp.path()).unwrap();
    rewind_to_v7(&db);
    db.with_conn(|conn| {
        conn.execute("CREATE TABLE ai_session_messages (bad TEXT)", [])?;
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
    assert_eq!(version, 7, "failed v8 migration must not advance version");
    for table in ["ai_sessions"] {
        let created: i64 = raw
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(created, 0, "failed v8 migration leaked {table}");
    }
}

#[test]
fn sessions_are_scoped_and_messages_replace_or_append_explicitly() {
    let db = AppDb::open_in_memory().unwrap();
    let saved = db
        .upsert_ai_session(
            session("session-1", "conn-1::db", "db"),
            &[
                message("m1", "hello", "complete"),
                message("m2", "world", "cancelled"),
            ],
            false,
        )
        .unwrap();
    assert_eq!(saved.id, "session-1");
    assert_eq!(db.ai_session_message_count("session-1").unwrap(), 2);

    let listed = db
        .list_ai_sessions(ORG, OWNER, Some("conn-1"), Some("db"), Some("conn-1::db"))
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].message_count, 2);
    assert_eq!(listed[0].preview, "hello");
    assert!(db
        .list_ai_sessions(ORG, OWNER, Some("conn-2"), None, None)
        .unwrap()
        .is_empty());
    assert!(db
        .list_ai_sessions("other-org", OWNER, None, None, None)
        .unwrap()
        .is_empty());

    // Replace is the default and must remove old messages.
    db.upsert_ai_session(
        session("session-1", "conn-1::db", "db"),
        &[message("m3", "replacement", "failed")],
        false,
    )
    .unwrap();
    let messages = db
        .list_ai_session_messages(ORG, OWNER, "session-1")
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "replacement");
    assert_eq!(messages[0].status, "failed");

    // Append adds only new message ids and keeps the existing order.
    db.upsert_ai_session(
        session("session-1", "conn-1::db", "db"),
        &[message("m4", "appended", "complete")],
        true,
    )
    .unwrap();
    db.upsert_ai_session(
        session("session-1", "conn-1::db", "db"),
        &[message("m4", "appended", "complete")],
        true,
    )
    .unwrap();
    let messages = db
        .list_ai_session_messages(ORG, OWNER, "session-1")
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, "appended");
}

#[test]
fn deleting_a_session_cascades_its_messages_and_keeps_other_scopes() {
    let db = AppDb::open_in_memory().unwrap();
    db.upsert_ai_session(
        session("session-1", "conn-1::db", "db"),
        &[message("m1", "hello", "complete")],
        false,
    )
    .unwrap();
    db.upsert_ai_session(
        session("session-2", "conn-1::db", "db"),
        &[message("m2", "other", "complete")],
        false,
    )
    .unwrap();
    assert!(db.delete_ai_session(ORG, OWNER, "session-1").unwrap());
    assert_eq!(db.ai_session_message_count("session-1").unwrap(), 0);
    assert_eq!(db.ai_session_message_count("session-2").unwrap(), 1);
    assert!(db
        .get_ai_session(ORG, OWNER, "session-1")
        .unwrap()
        .is_none());
    assert!(db
        .get_ai_session(ORG, OWNER, "session-2")
        .unwrap()
        .is_some());
    assert!(!db
        .delete_ai_session("other-org", OWNER, "session-2")
        .unwrap());
}
