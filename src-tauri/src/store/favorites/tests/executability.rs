//! The §2.6 promise that a favorite file *stays runnable*, proven with a real
//! SQLite engine rather than by comparing strings.
//!
//! The rest of this suite asserts on parsed fields, which cannot tell the
//! difference between "the metadata round-tripped" and "the file is still valid
//! SQL". This file closes that gap: it saves a favorite whose body carries
//! comments and string literals containing every sequence the front-matter
//! parser could plausibly get wrong, then hands the bytes on disk to
//! `rusqlite` and asserts the database it produces is identical to the one the
//! bare statement produces.

use super::*;
use rusqlite::Connection;

/// A statement engineered against the front-matter format itself:
/// - a leading `--` line comment, so the file does not *start* with metadata
/// - a `/* … */` block comment containing a `--` inside it
/// - string literals containing `--`, `/*`, `*/` and a doubled quote
/// - a final `SELECT` so the result set can be compared
const TRICKY_SQL: &str = r#"-- leading comment, not front-matter: no colon after -- here
/* a block comment
   that spans lines and contains a -- dash pair */
CREATE TABLE orders (id INTEGER PRIMARY KEY, note TEXT NOT NULL, total REAL);
INSERT INTO orders (note, total) VALUES
  ('a value with -- a dash pair', 10.5),
  ('a value with /* fake block */ inside', 20.25),
  ('it''s quoted, -- still not a comment', 30.0);
SELECT note, total FROM orders WHERE note LIKE '%--%' ORDER BY total;
"#;

/// A title that tries to escape its own metadata line. Nothing here may end
/// up as SQL: `\n` is escaped, so the embedded newline cannot start a line.
const HOSTILE_TITLE: &str = "-- title: forged\r\n-- keyword: forged\n/* */ ' \" \\  ";

/// Execute `sql` in a throwaway database and return the connection, so each
/// test can assert against its own statement. A favorite file goes through
/// exactly this path, unmodified.
fn memdb(sql: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(sql).unwrap();
    conn
}

/// `(note, total)` after executing `sql`, so two executions can be compared.
fn orders_after(sql: &str) -> Vec<(String, f64)> {
    let conn = memdb(sql);
    let mut stmt = conn
        .prepare("SELECT note, total FROM orders ORDER BY total")
        .unwrap();
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// The acceptance criterion, end to end: a favorite saved by DataZen, read back
/// off disk byte for byte, is accepted by a stock SQLite engine and produces
/// exactly the rows the bare statement produces.
#[test]
fn a_saved_favorite_file_still_executes_as_plain_sql() {
    let fx = Fixture::new();
    let store = fx.store();

    let saved = store
        .add(NewFavorite {
            connection_id: "cfg-1".into(),
            title: HOSTILE_TITLE.into(),
            sql: TRICKY_SQL.into(),
            database: Some("analytics".into()),
            keyword: Some("recon".into()),
        })
        .unwrap();

    // Exactly the bytes a user would open in `psql`, a text editor or `git`.
    let text = fs::read_to_string(fx.root().join(format!("{}.sql", saved.id))).unwrap();

    // Executing the file must not need any DataZen-specific decoder, and must
    // not need the metadata stripped first — that is the whole point of using
    // `--` comments.
    let from_file = orders_after(&text);
    let from_statement = orders_after(TRICKY_SQL);
    assert_eq!(from_file, from_statement);
    assert_eq!(from_file.len(), 3, "every INSERT must have run");
    assert_eq!(from_file[0].0, "a value with -- a dash pair");
    assert_eq!(from_file[1].0, "a value with /* fake block */ inside");
    assert_eq!(from_file[2].0, "it's quoted, -- still not a comment");

    // The embedded `\n` in the title did not manufacture a second metadata
    // line, so nothing forged reached the parse.
    let forged: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("--") && l.contains("forged"))
        .collect();
    assert_eq!(
        forged.len(),
        1,
        "the escaped title must stay on its own line, got {forged:?}"
    );
    assert!(!text.contains("\r"), "carriage returns are escaped too");

    // The same file is still *readable*, header and all: a hostile title is
    // data, not an injection into the format.
    store.invalidate_cache();
    let reread = listed_first(&store);
    // Trailing spaces are the one thing a title does not survive: the parser
    // trims each value (`frontmatter.rs`, `parse_value`) so a stray space in a
    // hand-edited header cannot shift a key. Display-only — reading never
    // rewrites the file, so the bytes on disk keep the spaces.
    assert_eq!(reread.title, HOSTILE_TITLE.trim_end());
    assert_eq!(reread.sql, TRICKY_SQL, "the body came through untouched");
    assert_eq!(reread.database.as_deref(), Some("analytics"));
    assert_eq!(reread.keyword.as_deref(), Some("recon"));
}

/// The same guarantee for a *hand-written* file: a user editing a favorite in
/// a sync folder, or a colleague committing one to a shared repository, must
/// get an executable file with no DataZen in the loop.
#[test]
fn a_hand_written_favorite_with_metadata_is_still_runnable() {
    let fx = Fixture::new();
    let store = fx.store();
    let text = "-- title: Nightly recon\n\
                -- connectionId: cfg-1\n\
                -- database: analytics\n\
                SELECT 'x' AS v;  -- trailing comment\n";
    fx.write("01J8XK2M9Q7B4F.sql", text);

    let listed = listed_first(&store);
    assert_eq!(listed.title, "Nightly recon");
    assert_eq!(listed.connection_id, "cfg-1");
    assert_eq!(listed.database.as_deref(), Some("analytics"));

    // `execute_batch` is the whole claim, and the file is handed over as-is.
    let file_batch = Connection::open_in_memory().unwrap().execute_batch(text);
    let stmt_batch = Connection::open_in_memory()
        .unwrap()
        .execute_batch("SELECT 'x' AS v;  -- trailing comment\n");
    assert_eq!(
        file_batch.is_ok(),
        stmt_batch.is_ok(),
        "file: {file_batch:?}"
    );
    file_batch.expect("a hand-written favorite file must be valid SQL");
}

/// Every field §2.6.2 puts in the header round-trips through a read →
/// re-render cycle, key order included.
///
/// This is what lets the keyword (§2.4) and folder (§2.5) tracks land on top
/// without a format change: a file written by this build re-renders itself
/// byte for byte, so a user who edits a header by hand, commits it, or syncs
/// it between machines does not accumulate diff churn.
///
/// The round trip is **field-based, not raw**: a favorite is re-rendered from
/// the six known fields, so a key a *future* build introduces is dropped if an
/// older build rewrites the file. That is deliberate (a half-understood header
/// is not worth preserving) and is asserted here so the behaviour stays
/// intentional rather than accidental.
#[test]
fn every_front_matter_field_round_trips_without_reordering() {
    let fx = Fixture::new();
    let store = fx.store();
    let text = "-- title: Round trip\n\
                -- connectionId: cfg-9\n\
                -- database: analytics\n\
                -- keyword: recon\n\
                -- createdAt: 2026-08-18T10:00:00+00:00\n\
                -- updatedAt: 2026-08-19T11:20:00+00:00\n\
                SELECT 1;\n";
    fx.write("01J8XK2M9Q7B4F.sql", text);

    let parsed = listed_first(&store);
    assert_eq!(parsed.title, "Round trip");
    assert_eq!(parsed.connection_id, "cfg-9");
    assert_eq!(parsed.database.as_deref(), Some("analytics"));
    assert_eq!(parsed.keyword.as_deref(), Some("recon"));
    assert_eq!(parsed.sql, "SELECT 1;\n");
    assert_eq!(parsed.created_at.to_rfc3339(), "2026-08-18T10:00:00+00:00");

    // Re-rendering reproduces the file exactly — same keys, same order, same
    // body. This is the "a save is a no-op on an untouched file" property.
    let rendered = FavoritesRepository::render_file(&parsed);
    assert_eq!(
        rendered, text,
        "the header must not be reordered on rewrite"
    );

    // The updated timestamp carries its value, not just its presence.
    store.invalidate_cache();
    let reread = listed_first(&store);
    assert_eq!(
        reread.updated_at.map(|t| t.to_rfc3339()),
        Some("2026-08-19T11:20:00+00:00".to_string())
    );

    // A key this build does not know is not carried through a rewrite. Recorded
    // so the limitation is a decision, not a surprise.
    fx.write(
        "01J8XK2MA1C2D3E.sql",
        "-- title: Future\n\
         -- connectionId: cfg-9\n\
         -- someFutureKey: from a later build\n\
         SELECT 1;\n",
    );
    store.invalidate_cache();
    let future = store
        .list(None)
        .into_iter()
        .find(|f| f.id == "01J8XK2MA1C2D3E")
        .unwrap();
    assert!(!FavoritesRepository::render_file(&future).contains("someFutureKey"));
    assert!(FavoritesRepository::render_file(&future).contains("-- title: Future\n"));

    // And the re-rendered file is still executable.
    let with_table =
        "-- title: t\n-- connectionId: c\nCREATE TABLE t (v INT);\nINSERT INTO t VALUES (7);\n";
    fx.write("01J8XK2MB1C2D3E.sql", with_table);
    store.invalidate_cache();
    let rerendered = FavoritesRepository::render_file(
        &store
            .list(None)
            .into_iter()
            .find(|f| f.id == "01J8XK2MB1C2D3E")
            .unwrap(),
    );
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&rerendered).unwrap();
    let v: i64 = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
    assert_eq!(
        v, 7,
        "the re-rendered file must still create and populate a table"
    );
}

/// A body that opens with a `/* */` block comment contributes no metadata
/// rather than swallowing the statement, and the block comment itself is what
/// `title` falls back to — recorded because it is the one surprising corner of
/// the derived-title rule.
#[test]
fn a_block_comment_at_the_top_is_not_front_matter() {
    let fx = Fixture::new();
    let store = fx.store();
    fx.write(
        "01J8XK2M9Q7B4F.sql",
        "/* title: this is not metadata */\nSELECT 1;\n",
    );
    let listed = listed_first(&store);
    assert_eq!(listed.sql, "/* title: this is not metadata */\nSELECT 1;\n");
    assert_eq!(listed.connection_id, "", "no header, no owner");
    assert_eq!(
        listed.title, "/* title: this is not metadata */",
        "the derived title is the first line that is not a `--` comment"
    );

    // And it runs.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&fs::read_to_string(fx.root().join("01J8XK2M9Q7B4F.sql")).unwrap())
        .unwrap();
    let v: i64 = conn.query_row("SELECT 1", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 1);
}
