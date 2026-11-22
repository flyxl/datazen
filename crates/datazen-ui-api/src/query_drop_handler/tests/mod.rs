mod workflows_tests;

use std::cell::RefCell;

use super::*;
use crate::query_drop_handler::drop::{drop_pane_key, DropHost, EditorHandle, TableSchemaLike};

#[derive(Default)]
struct Editor {
    document: String,
    raw: RefCell<Vec<(String, usize)>>,
    inserted: RefCell<Vec<(String, Option<usize>)>>,
    focused: RefCell<usize>,
}

impl EditorHandle for Editor {
    fn get_document(&self) -> String {
        self.document.clone()
    }

    fn raw_insert(&mut self, text: &str, pos: usize) {
        self.raw.borrow_mut().push((text.to_owned(), pos));
    }

    fn focus(&mut self) {
        *self.focused.borrow_mut() += 1;
    }

    fn insert_at(&mut self, text: &str, pos: Option<usize>) {
        self.inserted.borrow_mut().push((text.to_owned(), pos));
    }
}

#[derive(Default)]
struct DropHostStub {
    schemas: RefCell<Vec<String>>,
}

impl DropHost for DropHostStub {
    fn format_table_identifier(&self, table: &str, db_type: &str, schema: Option<&str>) -> String {
        match schema {
            Some(schema) => format!("{schema}.{table}[{db_type}]"),
            None => format!("{table}[{db_type}]"),
        }
    }

    fn fetch_table_schema(
        &self,
        _db_session_id: &str,
        _connection_id: &str,
        table: &str,
        _schema: Option<&str>,
        _database: &str,
        _db_type: &str,
    ) -> Option<TableSchemaLike> {
        self.schemas.borrow_mut().push(table.to_owned());
        Some(TableSchemaLike { present: true })
    }

    fn generate_sql(
        &self,
        schema: Option<&TableSchemaLike>,
        _db_type: &str,
        _schema_prefix: Option<&str>,
        table_name: &str,
        table_ref_label: &str,
    ) -> String {
        format!(
            "SELECT * FROM {table_ref_label} -- {table_name} {}",
            schema.is_some()
        )
    }
}

fn table(name: &str, schema: Option<&str>) -> DroppedTable {
    DroppedTable {
        table_name: name.to_owned(),
        schema: schema.map(str::to_owned),
        connection_id: None,
    }
}

#[test]
fn an_empty_document_gets_a_full_select_statement() {
    let (text, raw) = detect_drop_context("", 0, "users");
    assert_eq!(text, "SELECT * FROM users ");
    assert!(raw);
    let (text, _) = detect_drop_context("   ", 0, "users");
    assert_eq!(text, "SELECT * FROM users ");
}

#[test]
fn from_join_comma_and_as_contexts_use_the_bare_name() {
    for doc in [
        "SELECT * FROM ",
        "SELECT * JOIN ",
        "SELECT * LEFT JOIN ",
        "SELECT * RIGHT  JOIN ",
        "SELECT * CROSS JOIN ",
        "SELECT * INNER JOIN ",
        "SELECT * FULL JOIN ",
        "SELECT a FROM t, ",
        "SELECT a AS ",
    ] {
        let (text, _) = detect_drop_context(doc, doc.len(), "users");
        assert_eq!(text, " users ", "{doc}");
    }
}

#[test]
fn an_unknown_context_keeps_boundary_spacing() {
    let (text, _) = detect_drop_context("SELECT WHERE", 12, "users");
    assert_eq!(text, " users");
    let (text, _) = detect_drop_context("SELECT ", 7, "users");
    assert_eq!(text, "users");
    let (text, _) = detect_drop_context("abc", 2, "users");
    assert_eq!(text, " users ");
}

#[test]
fn default_pane_is_the_bare_panel_id() {
    assert_eq!(drop_pane_key("p", None), "p");
    assert_eq!(drop_pane_key("p", Some("main")), "p");
    assert_eq!(drop_pane_key("p", Some("side")), "p::side");
}

#[test]
fn a_nonempty_editor_inserts_each_table_at_the_drop_position() {
    let options = DropOptions {
        connection_id: "c".to_owned(),
        db_session_id: "s".to_owned(),
        database_type: Some("postgres".to_owned()),
        database: "db".to_owned(),
    };
    let mut editor = Editor {
        document: "SELECT * FROM ".to_owned(),
        ..Editor::default()
    };
    let host = DropHostStub::default();
    let effects = create_query_drop_handler(
        &options,
        &[table("users", Some("public")), table("orders", None)],
        Some(4),
        &mut editor,
        &host,
    );
    assert_eq!(effects.len(), 3);
    assert_eq!(
        effects[0],
        DropEffect::RawInsert {
            text: " public.users[postgres] ".to_owned(),
            pos: 4
        }
    );
    assert_eq!(
        effects[1],
        DropEffect::RawInsert {
            text: " orders[postgres] ".to_owned(),
            pos: 4
        }
    );
    assert_eq!(effects[2], DropEffect::Focus);
    assert_eq!(*editor.focused.borrow(), 1);
    assert!(host.schemas.borrow().is_empty());
}

#[test]
fn an_empty_editor_generates_sql_with_a_fetched_schema() {
    let options = DropOptions {
        connection_id: "c".to_owned(),
        db_session_id: String::new(),
        database_type: None,
        database: "db".to_owned(),
    };
    let mut editor = Editor::default();
    let host = DropHostStub::default();
    let effects = create_query_drop_handler(
        &options,
        &[table("users", Some("public"))],
        Some(0),
        &mut editor,
        &host,
    );
    assert_eq!(host.schemas.borrow().as_slice(), &["users".to_owned()]);
    assert_eq!(
        effects,
        vec![DropEffect::InsertAt {
            text: "SELECT * FROM public.users -- users true".to_owned(),
            pos: Some(0),
        }]
    );
}

#[test]
fn without_a_database_the_generation_uses_a_null_schema() {
    let options = DropOptions {
        connection_id: "c".to_owned(),
        db_session_id: "s".to_owned(),
        database_type: Some("sqlite".to_owned()),
        database: String::new(),
    };
    let mut editor = Editor::default();
    let host = DropHostStub::default();
    let effects =
        create_query_drop_handler(&options, &[table("users", None)], None, &mut editor, &host);
    assert!(host.schemas.borrow().is_empty());
    assert_eq!(
        effects,
        vec![DropEffect::InsertAt {
            text: "SELECT * FROM users -- users false".to_owned(),
            pos: None,
        }]
    );
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(QUERY_DROP_HANDLER_SURFACE.len(), 9);
    assert!(QUERY_DROP_HANDLER_SURFACE.contains(&"detectDropContext"));
    assert_eq!(QUERY_DROP_HANDLER_GAP.len(), 8);
    assert_eq!(crate::QUERY_DROP_HANDLER_SURFACE.len(), 9);
    assert_eq!(crate::QUERY_DROP_HANDLER_GAP.len(), 8);
    let _: Option<crate::DroppedTable> = Some(table("t", None));
}
