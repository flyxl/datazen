use super::*;
#[allow(unused_imports)]
use crate::{
    query_selected, sql_editor, type_, view, AppState, Section, Setting, APP_FONTS,
    APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX,
    NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY,
    TABLE_PAGE_SIZE,
};
#[allow(unused_imports)]
use rune::{Env, Runtime};

    #[test]
    fn content_toolbar_and_panel_tab_bar_render_at_forty_eight_points() {
        // L5: a 48pt content toolbar per section and a 48pt tablist tab bar
        // for workspace tabs (focusable tabs, active underline, close control).
        let mut rt = runtime();
        rt.frame(0.0);
        let toolbar = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("content-toolbar"))
            .expect("content toolbar present");
        assert!(
            (toolbar.rect[3] - 48.0).abs() < 1.0,
            "toolbar height {}, want 48",
            toolbar.rect[3]
        );

        // Workspace: tab strip becomes a 48pt tablist with focusable tab items.
        rt.state.active = Section::Workspace;
        let first = rt
            .state
            .tabs
            .open(datazen_ui_api::TabKind::Table, "customers")
            .expect("tab");
        let _second = rt
            .state
            .tabs
            .open(datazen_ui_api::TabKind::Table, "orders")
            .expect("tab");
        rt.invalidate();
        rt.frame(0.0);

        let strip = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("tab-strip"))
            .expect("tab strip present");
        assert_eq!(strip.role, "tablist");
        assert!(
            (strip.rect[3] - 48.0).abs() < 1.0,
            "tab strip height {}, want 48",
            strip.rect[3]
        );

        let tab = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(&format!("tab-{first}")))
            .expect("first tab");
        assert_eq!(tab.role, "tab");
        assert!(tab.focusable, "tab is focusable for roving keyboard nav");
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some(&format!("tab-close-{first}"))),
            "close control present"
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some(&format!("tab-underline-{first}"))),
            "active underline element present"
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("toolbar-new-query")),
            "workspace content toolbar exposes new query action"
        );
    }

    #[test]
    fn content_status_bar_replaces_migration_preview_with_context() {
        // L6: 40pt status bar shows connection state, context and shortcuts;
        // the migration preview banner is removed.
        let mut rt = runtime();
        rt.frame(0.0);
        let bar = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("content-status-bar"))
            .expect("status bar present");
        assert!(
            (bar.rect[3] - 40.0).abs() < 1.0,
            "status bar height {}, want 40",
            bar.rect[3]
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("status-connection-label")),
            "connection state label present"
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("status-context")),
            "context cluster present"
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("status-shortcuts")),
            "shortcut hints present"
        );
        let texts: Vec<String> = rt
            .elements()
            .iter()
            .filter_map(|e| e.text.clone())
            .collect();
        assert!(
            !texts.iter().any(|t| t.contains("Migration preview")),
            "migration preview banner removed: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("New query") && t.contains("Close")),
            "shortcut hints rendered: {texts:?}"
        );
    }

    #[test]
    fn light_theme_renders_grid_editor_and_er_with_theme_tokens() {
        // L7: hardcoded dark surfaces in grid/editor/ER follow the active theme.
        let mut rt = runtime();
        rt.state.write_ui_setting(Setting::Theme { dark: false }).ok();
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.find("content-toolbar").is_some(), "chrome renders in light mode");

        // Table grid renders under light theme.
        rt.state.active = Section::Workspace;
        let _ = rt.state.tabs.open(datazen_ui_api::TabKind::Table, "customers");
        rt.state.grid.load_page(&datazen_ui_api::TablePage {
            table: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            rows: vec![vec!["1".into(), "Ada".into()]],
            page: 0,
            page_size: 50,
            total_rows: 1,
            sort: None,
            filter: None,
            primary_key: vec!["id".into()],
            read_only: None,
        });
        rt.state.table_view = crate::TableView::Data;
        rt.state.er_schema = None;
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.find("data-grid").is_some(), "grid renders in light mode");

        // SQL editor renders under light theme (selected query tab).
        let _ = rt.state.tabs.open(datazen_ui_api::TabKind::Query, "Query");
        rt.state.er_schema = None;
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.find("sql-editor").is_some(), "sql editor renders in light mode");

        // ER canvas renders under light theme when schema is open and no query is selected.
        let table = rt.state.tabs.open(datazen_ui_api::TabKind::Table, "orders").expect("tab");
        rt.state.tabs.select(table);
        rt.state.er_schema = Some(("main".into(), "public".into()));
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.find("er-page").is_some(), "ER page renders in light mode");
        assert!(rt.find("er-canvas").is_some(), "ER canvas renders in light mode");
    }
