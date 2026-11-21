//! Independent audit for T508: `QuerySidebarSection` port.
//!
//! Fill branch coverage the coder does not drive: unknown/blank database
//! grouping, `find_group` guards, and context-level index handling.

use rune::{row, Cx, El, Env, Runtime, TextEdit};

use crate::views::query::query_sidebar_section::{
    build, find_group, group_history, HistoryEntryVm, HistoryScope, SidebarSectionVm,
    FAVORITES_PANEL_KEY, HISTORY_PANEL_KEY,
};

/// React's `HISTORY_UNKNOWN_DB_KEY` literal.
const UNKNOWN_DB_KEY: &str = "__unknown_db__";

fn entry(sql: &str, database: Option<&str>, at: &str) -> HistoryEntryVm {
    HistoryEntryVm {
        id: sql.to_owned(),
        sql: sql.to_owned(),
        database: database.map(str::to_owned),
        executed_at: at.to_owned(),
        success: true,
        ..Default::default()
    }
}

#[test]
fn grouping_buckets_blank_databases_into_the_unknown_group() {
    let groups = group_history(
        &[
            entry("SELECT known", Some(" app "), "2026-01-02T00:00:00Z"),
            entry("SELECT blank", Some("   "), "2026-01-03T00:00:00Z"),
            entry("SELECT none", None, "2026-01-04T00:00:00Z"),
        ],
        "unrecorded",
    );
    assert_eq!(groups.len(), 2, "blank and absent collapse into one unknown group");
    let unknown = groups.iter().find(|g| g.key == UNKNOWN_DB_KEY).expect("unknown group");
    assert_eq!(unknown.label, "unrecorded");
    assert_eq!(unknown.entries.len(), 2);
    let app = groups.iter().find(|g| g.key == "app").expect("trimmed database group");
    assert_eq!(app.entries[0].sql, "SELECT known");
}

#[test]
fn find_group_rejects_absent_blank_and_untrimmed_matches() {
    let groups = group_history(
        &[entry("SELECT app", Some("app"), "2026-01-01T00:00:00Z")],
        "unrecorded",
    );
    assert!(find_group(&groups, None).is_none());
    assert!(find_group(&groups, Some("  ")).is_none());
    assert!(find_group(&groups, Some(" app ")).is_some(), "the lookup trims the database");
    assert!(find_group(&groups, Some("other")).is_none());
}

#[test]
fn context_level_accepts_an_index_past_the_path_end_and_ignores_blank_values() {
    let path = vec!["db".to_owned(), "public".to_owned()];
    assert_eq!(
        SidebarSectionVm::select_context_level(&path, 5, "schema"),
        Some(vec!["db".to_owned(), "public".to_owned(), "schema".to_owned()]),
        "slice(0, index) past the end keeps the whole prefix"
    );
    assert_eq!(
        SidebarSectionVm::select_context_level(&path, 0, ""),
        None,
        "React's `!value` ignores the empty string"
    );
    assert_eq!(
        SidebarSectionVm::select_context_level(&path, 1, "  "),
        Some(vec!["db".to_owned(), "  ".to_owned()]),
        "React's `!value` treats whitespace as a non-empty value"
    );
}

fn geo_vm() -> SidebarSectionVm {
    SidebarSectionVm {
        favorites_visible: true,
        history_visible: true,
        favorites_root: Some("/datazen/favorites".to_owned()),
        history: vec![entry("SELECT 1", Some("app"), "2026-03-01T00:00:00Z")],
        selected_database: Some("app".to_owned()),
        history_scope: HistoryScope::Current,
        history_search: TextEdit::new(String::new()),
        ..Default::default()
    }
}

fn geo_view(dark: &bool, _cx: &mut Cx<bool>) -> El {
    let mut layer = Vec::new();
    build::<bool>(&geo_vm(), &None, None, *dark, &mut layer);
    row().w_full().h_full().children(layer)
}

#[test]
fn sidebars_are_256pt_and_in_bounds_at_both_sizes_in_both_palettes() {
    for (w, h) in [(960.0_f32, 640.0_f32), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut rt = Runtime::new(dark, geo_view, (w, h), Env { system_dark: dark, scale: 1.0 });
            rt.frame(0.0);
            for key in [FAVORITES_PANEL_KEY, HISTORY_PANEL_KEY] {
                let el = rt
                    .elements()
                    .into_iter()
                    .find(|e| e.key.as_deref() == Some(key))
                    .unwrap_or_else(|| panic!("{key} laid out at {w}x{h} dark={dark}"));
                let [x, y, ew, eh] = el.rect;
                assert_eq!(ew, 256.0, "{key} w-64 width at {w}x{h} dark={dark}");
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{key} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{key} vertical overflow at {w}x{h} dark={dark}: {:?}", el.rect);
            }
        }
    }
}
