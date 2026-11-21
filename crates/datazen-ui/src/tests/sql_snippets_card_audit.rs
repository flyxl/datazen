//! T426 audit: the SQL snippets card, verified against the React
//! source re-read line by line
//! (`windows/settings/SqlSnippetsCard.tsx`). Element evidence comes
//! from `Runtime::elements()`, presses go through real pointer
//! events.

use rune::{col, El, Env, Runtime, TextEdit};

use crate::components::sql_editor::snippets::builtin::builtin_sql_snippets;
use crate::components::sql_editor::snippets::types::SqlSnippetItem;
use crate::views::sql_snippets_card::{
    EditorSettingsExportInput, SqlSnippetsCard, SqlSnippetsCardState,
};

fn snippet(id: &str, prefix: &str) -> SqlSnippetItem {
    SqlSnippetItem {
        id: id.to_owned(),
        prefix: prefix.to_owned(),
        description_key: format!("{prefix} description"),
        template: format!("SELECT {prefix}\nFROM t;"),
    }
}

#[derive(Clone)]
struct AuditState {
    card: SqlSnippetsCardState,
    dark: bool,
    // the host's records
    snippet_updates: Vec<Vec<SqlSnippetItem>>,
    export_presses: usize,
    import_presses: usize,
    fresh_ids: usize,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            card: SqlSnippetsCardState::new(&EditorSettingsExportInput::default()),
            dark: true,
            snippet_updates: Vec::new(),
            export_presses: 0,
            import_presses: 0,
            fresh_ids: 0,
        }
    }

    fn with_custom() -> Self {
        let mut scene = Self::empty();
        scene.card.resync(&EditorSettingsExportInput {
            sql_snippets: vec![snippet("user-s1", "myq*"), snippet("user-s2", "cnt*")],
            ..EditorSettingsExportInput::default()
        });
        scene
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let k = crate::ui::kit::kit(s.dark);
    let card = SqlSnippetsCard::<AuditState>::new(&s.card, s.dark, |s: &mut AuditState| {
        &mut s.card
    })
    .on_update_snippets(|s, updated| {
        s.snippet_updates.push(updated.clone());
        s.card.resync(&EditorSettingsExportInput {
            sql_snippets: updated,
            ..EditorSettingsExportInput::default()
        });
    })
    .on_export(|s| {
        // The host: build the JSON, write the clipboard, report.
        s.export_presses += 1;
        let _json = s.card.export_json("2026-01-01T00:00:00.000Z");
        s.card.note_export_ok();
    })
    .on_import(|s| {
        // The host: read the clipboard, parse, apply, report.
        s.import_presses += 1;
        let text = r#"{ "sqlSnippets": [ { "id": "imp-1", "prefix": "imp*", "template": "T" } ] }"#;
        if let Some(imported) = s.card.import_text(text) {
            s.card.resync(&EditorSettingsExportInput {
                sql_snippets: imported.sql_snippets.unwrap_or_default(),
                ..EditorSettingsExportInput::default()
            });
            s.card.note_import_ok();
        }
    })
    .fresh_id(|s| {
        s.fresh_ids += 1;
        format!("fresh-{}", s.fresh_ids)
    })
    .build(&k, &mut layer);
    col().max_w(680.0).h_full().child(card).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (900.0, 1200.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text.clone())
        .unwrap_or_default()
}

/// The empty face: the header trio, the empty custom state, the
/// builtin list, no dialog.
#[test]
fn the_empty_face_mounts() {
    let rt = rt(AuditState::empty());
    for key in [
        "sql-snippets-card",
        "sql-snippets-title",
        "sql-snippets-add",
        "sql-snippets-export",
        "sql-snippets-import",
        "sql-snippets-custom-count",
        "sql-snippets-custom-empty",
        "sql-snippets-builtin-count",
        "sql-snippets-builtin-list",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    assert!(!has(&rt, "sql-snippets-custom-list"), "no custom rows");
    assert!(!has(&rt, "snippet-edit-dialog"), "the dialog is closed");
    assert!(!has(&rt, "confirm-dialog"), "no confirm is pending");
    // The builtin rows: one per entry with its duplicate button.
    for item in builtin_sql_snippets() {
        assert!(has(&rt, &format!("sql-snippets-builtin-item-{}", item.id)));
        assert!(has(&rt, &format!("sql-snippets-duplicate-{}", item.id)));
    }
}

/// The populated custom face: one row per snippet with the prefix
/// chip, the folded template and the action pair (the delete keeps
/// React's data-testid).
#[test]
fn the_custom_rows_render() {
    let rt = rt(AuditState::with_custom());
    for key in [
        "sql-snippets-custom-list",
        "sql-snippets-item-user-s1",
        "sql-snippets-item-user-s2",
        "sql-snippets-item-user-s1-prefix",
        "sql-snippets-edit-user-s1",
        "delete-snippet-user-s1",
        "sql-snippets-edit-user-s2",
        "delete-snippet-user-s2",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    assert!(!has(&rt, "sql-snippets-custom-empty"));
    // The template's newlines fold to spaces.
    assert_eq!(text_of(&rt, "sql-snippets-item-user-s1-template"), "SELECT myq* FROM t;");
    // The builtin list still renders below.
    assert!(has(&rt, "sql-snippets-builtin-list"));
}

/// Add → the dialog opens seeded empty; Save with a bad prefix keeps
/// it open; a good save appends under the fresh id and fires
/// `onUpdateSnippets`.
#[test]
fn the_add_flow_persists_through_the_dialog() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "sql-snippets-add");
    assert!(has(&rt, "snippet-edit-dialog"), "the dialog opens");
    // An empty prefix is refused.
    press(&mut rt, "snippet-edit-save");
    assert!(has(&rt, "snippet-edit-dialog"), "a refused save keeps it open");
    assert_eq!(rt.state.snippet_updates.len(), 0);
    // Type a valid pair (through the dialog's fields).
    rt.update(|s, _| {
        s.card.dialog.prefix = TextEdit::new("mynew*".to_owned());
        s.card.dialog.template = TextEdit::new("SELECT 9;".to_owned());
    });
    rt.invalidate();
    settle(&mut rt);
    press(&mut rt, "snippet-edit-save");
    assert!(!has(&rt, "snippet-edit-dialog"), "the dialog closes");
    let updates = &rt.state.snippet_updates;
    assert_eq!(updates.len(), 1, "onUpdateSnippets fired once");
    assert_eq!(updates[0].len(), 1);
    assert_eq!(updates[0][0].id, "fresh-1", "the fresh id rides");
    assert_eq!(updates[0][0].prefix, "mynew*");
    // The custom row renders after the host's resync.
    assert!(has(&rt, "sql-snippets-item-fresh-1"));
}

/// Edit → the dialog seeds the row's values; saving replaces by id.
#[test]
fn the_edit_flow_replaces_by_id() {
    let mut rt = rt(AuditState::with_custom());
    press(&mut rt, "sql-snippets-edit-user-s1");
    assert!(has(&rt, "snippet-edit-dialog"));
    assert_eq!(
        rt.state.card.dialog.prefix.text, "myq*",
        "the seed carries the row's prefix"
    );
    rt.update(|s, _| {
        s.card.dialog.prefix = TextEdit::new("edited*".to_owned());
    });
    rt.invalidate();
    settle(&mut rt);
    press(&mut rt, "snippet-edit-save");
    let updates = &rt.state.snippet_updates;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].len(), 2, "both rows survive");
    assert_eq!(updates[0][0].id, "user-s1");
    assert_eq!(updates[0][0].prefix, "edited*", "the row is replaced in place");
}

/// Duplicate (a builtin row) → the `_copy` seed, a fresh id, the
/// duplicate title.
#[test]
fn the_duplicate_flow_takes_a_fresh_id() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "sql-snippets-duplicate-select-all");
    assert!(has(&rt, "snippet-edit-dialog"));
    assert_eq!(
        rt.state.card.dialog.prefix.text, "sel*_copy",
        "the duplicate seed carries _copy"
    );
    press(&mut rt, "snippet-edit-save");
    let updates = &rt.state.snippet_updates;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0][0].id, "fresh-1", "the duplicate saves under a fresh id");
    assert_eq!(updates[0][0].prefix, "sel*_copy");
}

/// Delete → the warning confirm; OK removes the row and fires the
/// update; Cancel keeps everything.
#[test]
fn the_delete_confirm_flow() {
    // Cancel keeps the row.
    let mut rt = rt(AuditState::with_custom());
    press(&mut rt, "delete-snippet-user-s1");
    assert!(has(&rt, "confirm-dialog"), "the confirm opens");
    assert_eq!(rt.state.card.pending_delete_id(), Some("user-s1"));
    press(&mut rt, "confirm-dialog-cancel");
    assert!(!has(&rt, "confirm-dialog"));
    assert_eq!(rt.state.snippet_updates.len(), 0, "nothing persisted");
    assert!(has(&rt, "sql-snippets-item-user-s1"));

    // OK removes it.
    press(&mut rt, "delete-snippet-user-s1");
    assert!(has(&rt, "confirm-dialog"));
    press(&mut rt, "confirm-dialog-ok");
    assert!(!has(&rt, "confirm-dialog"));
    let updates = &rt.state.snippet_updates;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].len(), 1);
    assert_eq!(updates[0][0].id, "user-s2", "only the other row survives");
    assert!(!has(&rt, "sql-snippets-item-user-s1"));
    assert!(has(&rt, "sql-snippets-item-user-s2"));
}

/// Export/import: the host halves fire, the status line carries the
/// success sentences and the 3s rule clears it.
#[test]
fn the_export_import_status_flow() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "sql-snippets-export");
    assert_eq!(rt.state.export_presses, 1);
    assert!(has(&rt, "sql-snippets-status"), "the status line renders");
    // The 3s timer clears the live status.
    let timer = rt.state.card.status_timer;
    assert!(timer.is_some());
    rt.update(|s, _| {
        s.card.status_fired(timer.unwrap());
    });
    rt.invalidate();
    settle(&mut rt);
    assert!(!has(&rt, "sql-snippets-status"));

    // The import applies the parsed snippets and shows its status.
    press(&mut rt, "sql-snippets-import");
    assert_eq!(rt.state.import_presses, 1);
    assert!(has(&rt, "sql-snippets-status"));
    assert!(has(&rt, "sql-snippets-item-imp-1"), "the imported row lands");
}
