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

    #[test]
    fn a4_row_number_gutter_renders_header_and_row_indices() {
        // A4: the table grid shows a row-number gutter ("#" header + 1-based
        // row numbers per visible row) as a leading column.
        let mut rt = runtime();
        rt.state.workspace.connect("local").unwrap();
        rt.state.open_table_tab("customers").unwrap();
        rt.frame(0.0);

        assert!(rt.find("head-rownum").is_some(), "row-number gutter header present");
        assert!(rt.find("rownum-0").is_some(), "first data row has a row number cell");
        assert!(rt.find("rownum-1").is_some(), "second data row has a row number cell");

        // Header text is "#" and first row shows global 1-based index 1.
        let elements = rt.elements();
        let header = elements
            .iter()
            .find(|e| e.key.as_deref() == Some("head-rownum"))
            .expect("gutter header element");
        // The gutter cell contains a text child; search elements for the label.
        let texts: Vec<String> = elements.iter().filter_map(|e| e.text.clone()).collect();
        assert!(texts.iter().any(|t| t == "#"), "gutter header renders #: {texts:?}");
        assert!(texts.iter().any(|t| t == "1"), "first row renders number 1: {texts:?}");
        assert!(texts.iter().any(|t| t == "2"), "second row renders number 2: {texts:?}");
    }

    #[test]
    fn a4_column_width_persistence_round_trips_per_table() {
        use datazen_ui_api::WorkspaceApi;

        // A4: column widths persist per table via Setting::Layout keys and are
        // reapplied when the table page is (re)loaded.
        let mut rt = runtime();
        rt.state.workspace.connect("local").unwrap();
        rt.state.open_table_tab("customers").unwrap();
        rt.frame(0.0);

        let table = rt.state.grid.table_label().to_owned();
        let col_name = rt.state.grid.column_name(0).expect("column 0").to_owned();
        let default_w = rt.state.grid.col_w(0);
        assert!(default_w > 0.0, "default width is positive");

        // Set and persist a custom width for the first column.
        rt.state
            .set_table_column_width(&table, &col_name, 180)
            .expect("persist column width");
        assert_eq!(rt.state.grid.col_w(0) as u32, 180, "grid reflects updated width");

        // Persisted in layout settings under a stable per-table key.
        let key = rt.state.colw_key(&table, &col_name);
        assert_eq!(rt.state.layout_size(&key), Some(180), "layout setting stored");

        // Round-trip: reload the page and the persisted width is reapplied.
        rt.state.grid.set_col_w(0, 120);
        assert_eq!(rt.state.grid.col_w(0) as u32, 120, "width changed before reload");
        rt.state.reload_table_page();
        rt.frame(0.0);
        assert_eq!(
            rt.state.grid.col_w(0) as u32,
            180,
            "reloaded page reapplies persisted column width"
        );
    }

    #[test]
    fn the_titlebar_keeps_its_theme_toggle_inside_the_window() {
        // L3 regression: the section label is a grown leftover, not a
        // `w_full` band. A full-width label measures the padding box and
        // lays the trailing toggle out past the right edge — at every
        // window width, including the 960pt minimum.
        for (width, height) in [(1440.0, 900.0), (960.0, 640.0)] {
            let env = Env {
                system_dark: true,
                scale: 1.0,
            };
            let mut rt = Runtime::new(AppState::default(), view, (width, height), env);
            rt.frame(0.0);
            let elements = rt.elements();
            let titlebar = elements
                .iter()
                .find(|e| e.key.as_deref() == Some("datazen-titlebar"))
                .expect("titlebar band");
            assert!(
                (titlebar.rect[3] - 40.0).abs() < 1.0,
                "{width}: titlebar height {}, want 40",
                titlebar.rect[3]
            );
            let label = elements
                .iter()
                .find(|e| e.key.as_deref() == Some("datazen-titlebar-label"))
                .expect("section label");
            let toggle = elements
                .iter()
                .find(|e| e.key.as_deref() == Some("datazen-theme-toggle"))
                .expect("theme toggle");
            assert!(
                toggle.rect[0] + toggle.rect[2] <= width + 0.5,
                "{width}: theme toggle {:?} hangs past the right edge",
                toggle.rect
            );
            assert!(
                toggle.rect[0] >= width * 0.5,
                "{width}: theme toggle {:?} is not at the trailing edge",
                toggle.rect
            );
            assert!(
                label.rect[0] + label.rect[2] <= toggle.rect[0] + 0.5,
                "{width}: label {:?} overlaps the toggle {:?}",
                label.rect,
                toggle.rect
            );
            assert!(
                label.rect[0] >= 12.0,
                "{width}: label {:?} ignores the leading inset",
                label.rect
            );
        }
    }

    #[test]
    fn every_section_keeps_its_controls_inside_the_minimum_window_width() {
        // L3 pins the window at 960x640. There the rail (112) and navigator
        // (280) leave 567 for the page and ~471 inside its padding, so header
        // action rows, settings label/control pairs and the dashboard form
        // used to push their last control past the right edge. Nothing may
        // leave the window horizontally (taller scroll content is fine).
        for section in [
            Section::Connections,
            Section::Workspace,
            Section::Dashboards,
            Section::Workflows,
            Section::Wapps,
            Section::Settings,
        ] {
            let mut rt = Runtime::new(
                AppState::default(),
                view,
                (960.0, 640.0),
                Env {
                    system_dark: true,
                    scale: 1.0,
                },
            );
            rt.state.active = section;
            rt.frame(0.0);
            for e in rt.elements() {
                let [x, _, w, _] = e.rect;
                assert!(
                    x >= -0.5 && x + w <= 960.5,
                    "{section:?}: {:?} {:?} hangs past the right edge",
                    e.key,
                    e.rect
                );
            }
        }
    }

    #[test]
    fn every_settings_section_keeps_its_rows_inside_the_minimum_window() {
        use crate::settings_state::SETTINGS_SECTIONS;
        for def in SETTINGS_SECTIONS {
            let mut rt = Runtime::new(
                AppState::default(),
                view,
                (960.0, 640.0),
                Env {
                    system_dark: true,
                    scale: 1.0,
                },
            );
            rt.state.active = Section::Settings;
            rt.state.set_settings_section(def.id);
            rt.frame(0.0);
            let content = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(&format!("settings-content-{}", def.id)))
                .unwrap_or_else(|| panic!("{}: content column", def.id));
            assert!(
                content.rect[0] + content.rect[2] <= 960.5,
                "{}: content {:?} hangs past the right edge",
                def.id,
                content.rect
            );
        }
    }

    #[test]
    fn the_settings_page_ends_above_the_status_bar() {
        // The page is one row of the page column, next to the toolbar and the
        // status bar: measuring it against the whole column (h_full) pushed
        // the status bar out of the window.
        let mut rt = Runtime::new(
            AppState::default(),
            view,
            (960.0, 640.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.state.active = Section::Settings;
        rt.frame(0.0);
        let elements = rt.elements();
        let rect = |key: &str| {
            elements
                .iter()
                .find(|e| e.key.as_deref() == Some(key))
                .unwrap_or_else(|| panic!("{key} present"))
                .rect
        };
        let page = rect("settings-page");
        let bar = rect("content-status-bar");
        assert!(
            page[1] + page[3] <= bar[1] + 0.5,
            "settings page {:?} overlaps the status bar {:?}",
            page,
            bar
        );
        assert!(
            bar[1] + bar[3] <= 640.5,
            "status bar {:?} leaves the 640pt window",
            bar
        );
    }

    /// Both dialogs must stay inside the window, keep their footer pinned to
    /// the panel's bottom edge, and keep the primary action inside the visible
    /// footer — at every window size the migration gate renders.
    fn assert_dialogs_fit(width: f32, height: f32) {
        for (dialog_key, footer_key, action_key, open) in [
            (
                "connection-dialog",
                "connection-dialog-footer",
                "connection-save",
                0,
            ),
            ("import-dialog", "import-dialog-footer", "import-confirm", 1),
        ] {
            let mut rt = Runtime::new(
                AppState::default(),
                view,
                (width, height),
                Env {
                    system_dark: true,
                    scale: 1.0,
                },
            );
            if open == 0 {
                rt.state.open_connection_dialog();
            } else {
                rt.state.open_import_dialog();
            }
            rt.frame(0.0);
            let elements = rt.elements();
            let rect = |key: &str| {
                elements
                    .iter()
                    .find(|e| e.key.as_deref() == Some(key))
                    .unwrap_or_else(|| panic!("{key} present"))
                    .rect
            };
            let dialog = rect(dialog_key);
            let footer = rect(footer_key);
            let action = rect(action_key);
            let title = rect(&format!("{dialog_key}-title"));
            let close = rect(&format!("{dialog_key}-close"));
            assert!(
                dialog[0] >= -0.5
                    && dialog[0] + dialog[2] <= width + 0.5
                    && dialog[1] >= -0.5
                    && dialog[1] + dialog[3] <= height + 0.5,
                "{dialog_key}: {:?} leaves the {width}x{height} window",
                dialog
            );
            for (part, part_rect) in [("title", title), ("close", close)] {
                assert!(
                    part_rect[0] >= dialog[0] - 0.5
                        && part_rect[0] + part_rect[2] <= dialog[0] + dialog[2] + 0.5
                        && part_rect[1] >= dialog[1] - 0.5
                        && part_rect[1] + part_rect[3] <= dialog[1] + dialog[3] + 0.5,
                    "{dialog_key}: {part} {:?} is outside the panel {:?}",
                    part_rect,
                    dialog
                );
            }
            assert!(
                footer[1] + footer[3] <= dialog[1] + dialog[3] + 0.5 && footer[1] >= dialog[1],
                "{dialog_key}: footer {:?} leaves the dialog {:?}",
                footer,
                dialog
            );
            assert!(
                (footer[1] + footer[3] - (dialog[1] + dialog[3])).abs() <= 0.5,
                "{dialog_key}: footer {:?} is not pinned to the panel's bottom edge {:?}",
                footer,
                dialog
            );
            assert!(
                action[1] >= footer[1] - 0.5 && action[1] + action[3] <= footer[1] + footer[3] + 0.5,
                "{dialog_key}: {action_key} {:?} is outside the visible footer {:?}",
                action,
                footer
            );
        }
    }

    #[test]
    fn the_connection_and_import_dialogs_fit_the_minimum_window() {
        // Both dialogs are larger than the 960x640 window's leftover page
        // area, and the create/save action used to sit at the end of the
        // scrolling form body instead of the always-visible footer.
        assert_dialogs_fit(960.0, 640.0);
    }

    #[test]
    fn the_connection_and_import_dialogs_fit_the_default_window() {
        assert_dialogs_fit(1440.0, 816.0);
    }

#[test]
fn text_fields_put_their_line_in_the_middle() {
    // Rune lays a field's text out from the content box's top edge, so a field
    // taller than its line pins the text (and the caret) to the top. The shared
    // component hands the surplus back as symmetric padding; every single-line
    // field is built through it with its own height and font.
    for (style, height) in [
        (type_::LABEL, 24.0),
        (type_::LABEL, 28.0),
        (type_::LABEL, 36.0),
        (rune::TextStyle::new(13.0, 400.0), 24.0),
    ] {
        let pad = crate::ui::field::centered_pad(style, height);
        assert!(pad >= 0.0, "{height}: padding {pad} is negative");
        assert!(
            (pad * 2.0 + style.line_px() - height).abs() < 0.01,
            "{height}: padding {pad} does not centre the {}-point line",
            style.line_px()
        );
    }
    // The two navigator/home search fields: 28 points tall, 15-point line.
    assert_eq!(crate::ui::field::centered_pad(type_::LABEL, 28.0), 6.5);
    // A field shorter than its line keeps its text at the top instead of
    // asking for negative padding.
    assert_eq!(crate::ui::field::centered_pad(type_::LABEL, 10.0), 0.0);
}

#[test]
fn the_text_field_component_keeps_the_requested_geometry() {
    // The component owns the height and the padding that centres the line, so a
    // caller cannot build a field taller than its line and leave the text
    // pinned to the top. The padding sits inside the box: the field still takes
    // the height it asked for, and therefore keeps its whole click target.
    fn probe(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        rune::col()
            .child(
                crate::ui::field::TextField::new("probe-short", &state.connection_search)
                    .height(24.0)
                    .width(120.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_search)
                    .build(),
            )
            .child(
                crate::ui::field::TextField::new("probe-tall", &state.connection_search)
                    .height(36.0)
                    .width(120.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_search)
                    .build(),
            )
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), probe, (400.0, 200.0), env);
    rt.frame(0.0);
    let elements = rt.elements();
    for (key, height) in [("probe-short", 24.0), ("probe-tall", 36.0)] {
        let field = elements
            .iter()
            .find(|element| element.key.as_deref() == Some(key))
            .expect("the field is laid out");
        assert!(
            (field.rect[3] - height).abs() < 0.5,
            "{key}: height {}, want {height}",
            field.rect[3]
        );
        assert!(field.rect[2] > 0.0, "{key}: width {}", field.rect[2]);
    }
}

#[test]
fn the_button_variants_keep_one_height_and_baseline() {
    // A dialog footer mixes a ghost cancel with contrast and neutral actions.
    // The kit owns height, padding and ink, so every variant lands on one row at
    // one height and each label sits in the middle of its own box.
    fn probe(_state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
        use rune_kit::{Button, ButtonVariant, ControlSize};
        let kit = crate::ui::kit::kit(true);
        let variants = [
            ("probe-contrast", ButtonVariant::Contrast),
            ("probe-neutral", ButtonVariant::Neutral),
            ("probe-ghost", ButtonVariant::Ghost),
            ("probe-ghost-muted", ButtonVariant::GhostMuted),
        ];
        let mut row = rune::row().gap(8.0);
        for (key, variant) in variants {
            row = row.child(
                Button::new("保存")
                    .variant(variant)
                    .control_size(ControlSize::ExtraLarge)
                    .build(kit)
                    .key(key),
            );
        }
        row
    }
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    let mut rt = Runtime::new(AppState::default(), probe, (600.0, 120.0), env);
    rt.frame(0.0);
    let elements = rt.elements();
    let mut top = None;
    for key in ["probe-contrast", "probe-neutral", "probe-ghost", "probe-ghost-muted"] {
        let button = elements
            .iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("{key} is laid out"));
        assert!(
            (button.rect[3] - 36.0).abs() < 0.5,
            "{key}: height {}, want 36",
            button.rect[3]
        );
        assert!(button.rect[0] + button.rect[2] <= 600.5, "{key} hangs past the window");
        match top {
            None => top = Some(button.rect[1]),
            Some(y) => assert!((button.rect[1] - y).abs() < 0.5, "{key} sits off the row"),
        }
    }
}
