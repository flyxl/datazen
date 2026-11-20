//! T155 audit: the `sqlEditorContextMenu` port, verified against the React
//! source re-read line by line (`lib/sqlEditorContextMenu.ts` plus the
//! driver-sdk contract it consumes, `types/menu.ts` /
//! `nativeContextMenu.ts`) and the React test
//! `__tests__/sqlEditorContextMenu.test.ts` (every case mirrored; the
//! platform-dependent shortcut copy gets both macOS and Ctrl rows the
//! React test cannot express in one environment).
//!
//! Tester-written: the coder's inline suites are treated as a black box;
//! every expectation below is re-derived from the React files.

use std::cell::RefCell;
use std::rc::Rc;

use crate::components::sql_editor::sql_editor_context_menu::{
    build_sql_editor_context_menu_items, format_mod_key, native_edit_menu_items,
    toggle_sql_line_comments, BuildSqlEditorContextMenuArgs, NativeMenuItemDef, Platform,
    SqlEditorContextMenuHandlers, SqlEditorContextMenuLabels,
};

fn labels() -> SqlEditorContextMenuLabels {
    SqlEditorContextMenuLabels {
        run: "Run".into(),
        run_selection: "Run Selection".into(),
        format: "Format".into(),
        comment: "Comment/Uncomment".into(),
        add_favorite: Some("Add to Favorites".into()),
    }
}

/// The `kinds()` helper from the React test: predefined names for edit
/// items, the id for real items, the kind otherwise.
fn kinds(items: &[NativeMenuItemDef]) -> Vec<&str> {
    items.iter().map(NativeMenuItemDef::kind_token).collect()
}

/// Find the item with `id` and return `(enabled, shortcut, action)`.
fn item_by_id<'a>(
    items: &'a [NativeMenuItemDef],
    id: &'a str,
) -> Option<(Option<bool>, Option<&'a str>, &'a Rc<dyn Fn()>)> {
    match items.iter().find(|i| i.is_item_with_id(id))? {
        NativeMenuItemDef::Item { enabled, shortcut, action, .. } => {
            Some((*enabled, shortcut.as_deref(), action))
        }
        _ => None,
    }
}

// ---- toggleSqlLineComments ---------------------------------------------------

/// React test 1: prefixes non-empty lines with `-- `.
#[test]
fn toggle_prefixes_non_empty_lines() {
    assert_eq!(toggle_sql_line_comments("SELECT 1\n\nSELECT 2"), "-- SELECT 1\n\n-- SELECT 2");
}

/// React test 2: strips `-- ` when every non-empty line is commented
/// (indent and at most one space are kept).
#[test]
fn toggle_strips_when_all_commented() {
    assert_eq!(toggle_sql_line_comments("-- SELECT 1\n\n-- SELECT 2"), "SELECT 1\n\nSELECT 2");
    assert_eq!(toggle_sql_line_comments("-- a\n\n  -- b"), "a\n\n  b");
    // Mixed: still adds (React only strips when ALL non-empty lines match).
    assert_eq!(toggle_sql_line_comments("-- a\nb"), "-- -- a\n-- b");
    // A comment marker with no trailing space still strips.
    assert_eq!(toggle_sql_line_comments("--a\n--b"), "a\nb");
}

// ---- buildSqlEditorContextMenuItems ------------------------------------------

/// React test 1: the full assembly order — OS edit block, separator, the
/// four SQL actions, separator, the favorite — and the favorite's action
/// receives the (trimmed) SQL.
#[test]
fn assembly_order_and_favorite_action() {
    let added = Rc::new(RefCell::new(Vec::<String>::new()));
    let ran = Rc::new(RefCell::new(0usize));
    let handlers = SqlEditorContextMenuHandlers {
        on_run: Some({
            let ran = Rc::clone(&ran);
            Rc::new(move || *ran.borrow_mut() += 1)
        }),
        // The React test binds every handler; the full order assertion
        // needs them all present.
        on_run_selection: Some(Rc::new(|| {})),
        on_format: Some(Rc::new(|| {})),
        on_comment: Some(Rc::new(|| {})),
        on_add_favorite: Some({
            let added = Rc::clone(&added);
            Rc::new(move |sql: &str| added.borrow_mut().push(sql.to_string()))
        }),
        ..SqlEditorContextMenuHandlers::default()
    };
    let args = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "SELECT 1",
        has_selection: true,
        platform: Platform::Windows,
    };
    let items = build_sql_editor_context_menu_items(&args);

    assert_eq!(
        kinds(&items),
        vec![
            "Cut",
            "Copy",
            "Paste",
            "SelectAll",
            "separator",
            "run",
            "run-selection",
            "format",
            "comment",
            "separator",
            "add-favorite",
        ]
    );
    // The React assertions: the favorite is enabled and its action runs
    // with the trimmed SQL.
    let (enabled, _, action) = item_by_id(&items, "add-favorite").expect("favorite present");
    assert_eq!(enabled, Some(true));
    action();
    assert_eq!(*added.borrow(), vec!["SELECT 1".to_string()]);
    // run-selection enabled with a selection.
    let (enabled, _, _) = item_by_id(&items, "run-selection").expect("run-selection present");
    assert_eq!(enabled, Some(true));
    // The run handler is the bound one (the same Rc fires).
    item_by_id(&items, "run").expect("run present").2();
    assert_eq!(*ran.borrow(), 1);
}

/// React test 2: run / run-selection / comment / add-favorite carry the
/// platform modifier shortcuts; format deliberately has none.
#[test]
fn shortcuts_are_attached() {
    let handlers = SqlEditorContextMenuHandlers {
        on_run: Some(Rc::new(|| {})),
        on_run_selection: Some(Rc::new(|| {})),
        on_format: Some(Rc::new(|| {})),
        on_comment: Some(Rc::new(|| {})),
        on_add_favorite: Some(Rc::new(|_: &str| {})),
    };
    let args = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "SELECT 1",
        has_selection: false,
        platform: Platform::Linux,
    };
    let items = build_sql_editor_context_menu_items(&args);
    assert_eq!(item_by_id(&items, "run").unwrap().1, Some("Ctrl+Enter"));
    assert_eq!(item_by_id(&items, "run-selection").unwrap().1, Some("Ctrl+Enter"));
    assert_eq!(item_by_id(&items, "comment").unwrap().1, Some("Ctrl+/"));
    assert_eq!(item_by_id(&items, "add-favorite").unwrap().1, Some("Ctrl+S"));
    assert_eq!(item_by_id(&items, "format").unwrap().1, None);
    // macOS copy: formatModKey is ⌘ there, Ctrl everywhere else.
    let mac = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "SELECT 1",
        has_selection: false,
        platform: Platform::Macos,
    };
    let mac_items = build_sql_editor_context_menu_items(&mac);
    assert_eq!(item_by_id(&mac_items, "run").unwrap().1, Some("⌘+Enter"));
    assert_eq!(format_mod_key(Platform::Macos), "⌘");
    assert_eq!(format_mod_key(Platform::Windows), "Ctrl");
    assert_eq!(format_mod_key(Platform::Linux), "Ctrl");
    assert_eq!(format_mod_key(Platform::Unknown), "Ctrl");
}

/// React test 3: run-selection disabled without a selection; run / format
/// / comment / favorite disabled on blank SQL; the disabled favorite's
/// action does not call the handler.
#[test]
fn disabled_states_and_no_fire_on_blank() {
    let added = Rc::new(RefCell::new(0usize));
    let handlers = SqlEditorContextMenuHandlers {
        on_run: Some(Rc::new(|| {})),
        on_run_selection: Some(Rc::new(|| {})),
        on_format: Some(Rc::new(|| {})),
        on_comment: Some(Rc::new(|| {})),
        on_add_favorite: Some({
            let added = Rc::clone(&added);
            Rc::new(move |_: &str| *added.borrow_mut() += 1)
        }),
    };
    let args = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "   ",
        has_selection: false,
        platform: Platform::Windows,
    };
    let items = build_sql_editor_context_menu_items(&args);
    assert_eq!(item_by_id(&items, "run-selection").unwrap().0, Some(false));
    assert_eq!(item_by_id(&items, "run").unwrap().0, Some(false));
    assert_eq!(item_by_id(&items, "format").unwrap().0, Some(false));
    // Comment stays enabled only through a selection; blank + no
    // selection disables it too.
    assert_eq!(item_by_id(&items, "comment").unwrap().0, Some(false));
    let (enabled, _, action) = item_by_id(&items, "add-favorite").unwrap();
    assert_eq!(enabled, Some(false));
    action();
    assert_eq!(*added.borrow(), 0, "disabled favorite must not fire");
    // With a selection, comment is enabled even on blank SQL.
    let sel = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "   ",
        has_selection: true,
        platform: Platform::Windows,
    };
    let sel_items = build_sql_editor_context_menu_items(&sel);
    assert_eq!(item_by_id(&sel_items, "comment").unwrap().0, Some(true));
}

/// React test 4: the favorite is omitted when its handler is missing —
/// and, per the React `handlers.onAddFavorite && labels.addFavorite`
/// guard, also when only the label is missing.
#[test]
fn favorite_omitted_without_handler_or_label() {
    let handlers = SqlEditorContextMenuHandlers {
        on_format: Some(Rc::new(|| {})),
        ..SqlEditorContextMenuHandlers::default()
    };
    let args = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "SELECT 1",
        has_selection: false,
        platform: Platform::Windows,
    };
    let items = build_sql_editor_context_menu_items(&args);
    assert!(!items.iter().any(|i| i.is_item_with_id("add-favorite")));
    // No handler but a label: still omitted.
    let no_handler = SqlEditorContextMenuHandlers::default();
    let a2 = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &no_handler,
        sql_text: "SELECT 1",
        has_selection: false,
        platform: Platform::Windows,
    };
    assert!(!build_sql_editor_context_menu_items(&a2)
        .iter()
        .any(|i| i.is_item_with_id("add-favorite")));
    // Handler but no label: still omitted, and no orphan separator is
    // left where the favorite block would have started (the separator is
    // pushed only inside the favorite branch, after the React guard).
    let mut bare = labels();
    bare.add_favorite = None;
    let handlers_with = SqlEditorContextMenuHandlers {
        on_comment: Some(Rc::new(|| {})),
        on_add_favorite: Some(Rc::new(|_: &str| {})),
        ..SqlEditorContextMenuHandlers::default()
    };
    let a3 = BuildSqlEditorContextMenuArgs {
        labels: &bare,
        handlers: &handlers_with,
        sql_text: "SELECT 1",
        has_selection: false,
        platform: Platform::Windows,
    };
    let items3 = build_sql_editor_context_menu_items(&a3);
    assert!(!items3.iter().any(|i| i.is_item_with_id("add-favorite")));
    assert_eq!(
        kinds(&items3),
        vec!["Cut", "Copy", "Paste", "SelectAll", "separator", "comment"]
    );
    // Every other handler absent: only the edit block + separator remain.
    assert_eq!(
        kinds(&items),
        vec!["Cut", "Copy", "Paste", "SelectAll", "separator", "format"]
    );
}

/// React test 5: the SQL is trimmed before it reaches onAddFavorite.
#[test]
fn favorite_receives_trimmed_sql() {
    let added = Rc::new(RefCell::new(Vec::<String>::new()));
    let handlers = SqlEditorContextMenuHandlers {
        on_add_favorite: Some({
            let added = Rc::clone(&added);
            Rc::new(move |sql: &str| added.borrow_mut().push(sql.to_string()))
        }),
        ..SqlEditorContextMenuHandlers::default()
    };
    let args = BuildSqlEditorContextMenuArgs {
        labels: &labels(),
        handlers: &handlers,
        sql_text: "  SELECT 2  ",
        has_selection: false,
        platform: Platform::Windows,
    };
    let items = build_sql_editor_context_menu_items(&args);
    let (_, _, action) = item_by_id(&items, "add-favorite").expect("favorite present");
    action();
    assert_eq!(*added.borrow(), vec!["SELECT 2".to_string()]);
}

/// The driver-sdk edit block itself: exactly the four predefined edit
/// actions, in declaration order, with no text overrides.
#[test]
fn native_edit_block_is_cut_copy_paste_select_all() {
    use crate::components::sql_editor::sql_editor_context_menu::NativeMenuPredefined;
    assert_eq!(kinds(&native_edit_menu_items()), vec!["Cut", "Copy", "Paste", "SelectAll"]);
    for item in native_edit_menu_items() {
        match item {
            NativeMenuItemDef::Predefined { text, .. } => assert!(text.is_none()),
            _ => panic!("edit block entries are predefined only"),
        }
    }
    // The predefined vocabulary carries all seven React kinds.
    assert_eq!(
        [
            NativeMenuPredefined::Separator,
            NativeMenuPredefined::Cut,
            NativeMenuPredefined::Copy,
            NativeMenuPredefined::Paste,
            NativeMenuPredefined::SelectAll,
            NativeMenuPredefined::Undo,
            NativeMenuPredefined::Redo
        ]
        .len(),
        7
    );
}
