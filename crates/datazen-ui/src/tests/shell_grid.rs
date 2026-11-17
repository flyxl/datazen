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
    fn the_apps_font_pool_shapes_chinese_without_tofu() {
        // The pool run() installs (H8): Inter for the UI's Latin, Noto Sans SC
        // for Chinese in the interface and the grid, IBM Plex Mono for the
        // editor. A shaped Han character is a real glyph, not .notdef.
        let mut system = rune::text::TextSystem::new(APP_FONTS, APP_FONT_FAMILIES);
        let han = glyph_ids(&mut system, "中文数据");
        assert_eq!(han.len(), 4, "one glyph per Han character");
        assert!(han.iter().all(|id| *id != 0), "tofu: {han:?}");

        // The UI's Latin look is unchanged: Inter still serves Latin.
        let latin = glyph_ids(&mut system, "DataZen");
        assert!(latin.iter().all(|id| *id != 0), "tofu: {latin:?}");
    }

    #[test]
    fn page_home_and_end_move_the_focus_and_it_stays_in_the_model() {
        use crate::app_key;
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.focus(), (0, 0));

        let key = |runtime: &mut rune::Runtime<AppState>, key: Key| {
            runtime.key(KeyEvent { key, mods: Mods::default() });
            runtime.frame(0.0);
        };

        // PageDown jumps a loaded page of rows, clamped to the boundary.
        key(&mut runtime, Key::PageDown);
        assert_eq!(runtime.state.grid.focus(), (24, 0), "one page down lands on the last row");
        key(&mut runtime, Key::PageDown);
        assert_eq!(runtime.state.grid.focus(), (24, 0), "the clamp keeps the focus inside the model");

        // PageUp goes back a page.
        key(&mut runtime, Key::PageUp);
        assert_eq!(runtime.state.grid.focus(), (0, 0));

        // End is the row's last column; Home is the first again.
        key(&mut runtime, Key::End);
        let (_, cols) = {
            let grid = &runtime.state.grid;
            (grid.rows, grid.cols)
        };
        assert_eq!(runtime.state.grid.focus(), (0, cols - 1), "End is the last column");
        key(&mut runtime, Key::Home);
        assert_eq!(runtime.state.grid.focus(), (0, 0), "Home is the first column");

        // After every move the focus is still inside the grid's model.
        let (rows, cols) = {
            let grid = &runtime.state.grid;
            (grid.rows, grid.cols)
        };
        let (r, c) = runtime.state.grid.focus();
        assert!(r < rows && c < cols, "the focus stays in the model");
    }

    #[test]
    fn the_grid_reads_its_shape_and_current_cell_to_the_accessibility_tree() {
        use crate::app_key;
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use serde_json::Value;

        fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
            if v["key"] == key {
                return Some(v);
            }
            v["children"].as_array()?.iter().find_map(|child| find(child, key))
        }

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        // The grid reads its page's shape and where the focus is.
        let tree = runtime.accessibility_json();
        let grid = find(&tree, "data-grid-scroll").expect("the grid is in the tree");
        assert_eq!(grid["role"], "grid");
        assert_eq!(grid["name"], "customers");
        assert_eq!(grid["value"], "25 rows × 8 columns");
        assert_eq!(grid["activeDescendant"], "cell-0-0");
        let cell = find(&tree, "cell-0-0").expect("the focused cell is mounted");
        assert_eq!(cell["role"], "grid-cell");
        assert_eq!(cell["name"], "0");

        // Moving the focus moves what the tree reads.
        let key = |runtime: &mut rune::Runtime<AppState>, key: Key| {
            runtime.key(KeyEvent { key, mods: Mods::default() });
            runtime.frame(0.0);
        };
        key(&mut runtime, Key::Down);
        key(&mut runtime, Key::Down);
        key(&mut runtime, Key::Right);
        assert_eq!(runtime.state.grid.focus(), (2, 1));
        let tree = runtime.accessibility_json();
        let grid = find(&tree, "data-grid-scroll").unwrap();
        assert_eq!(grid["activeDescendant"], "cell-2-1", "the tree follows the focus");
        assert_eq!(find(&tree, "cell-2-1").unwrap()["name"], "customer-2");

        // The next page reads its own shape: 40 rows over a page size of 25 leaves 15.
        // `update` marks the state dirty; a bare mutation would leave the committed
        // view, and so the a11y tree, reading the old page.
        runtime.update(|state, _| state.next_table_page());
        runtime.frame(0.0);
        let tree = runtime.accessibility_json();
        let grid = find(&tree, "data-grid-scroll").unwrap();
        assert_eq!(grid["value"], "15 rows × 8 columns");
        assert_eq!(grid["name"], "customers");
    }

    #[test]
    fn twenty_thousand_rows_mount_a_window_and_two_hundred_columns_build_one_window_of_cells() {
        use crate::app_key;
        use datazen_ui_api::WorkspaceApi;
        use serde_json::Value;

        fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
            if v["key"] == key {
                return Some(v);
            }
            v["children"].as_array()?.iter().find_map(|child| find(child, key))
        }
        fn walk<'a>(v: &'a Value, f: &mut dyn FnMut(&'a Value)) {
            f(v);
            if let Some(children) = v["children"].as_array() {
                for child in children {
                    walk(child, f);
                }
            }
        }

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        // The POC's scale as a loaded page: 20,000 rows × 200 columns. The
        // records carry no values — the grid's fallback reads r{r}c{c} — so
        // the page costs 20,000 empty records, not four million cells. The
        // surface stays the product table tab's; the sheet virtualizes the
        // 20,000-row list at the table page's size.
        let columns: Vec<String> = (0..200).map(|c| format!("c{c}")).collect();
        let records: Vec<Vec<String>> = (0..20_000).map(|_| Vec::new()).collect();
        let page = datazen_ui_api::TablePage {
            table: "customers".into(),
            columns,
            rows: records,
            page: 0,
            page_size: 20_000,
            total_rows: 20_000,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        };
        runtime.update(|state, _| state.grid.load_page(&page));
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.rows, 20_000);
        assert_eq!(runtime.state.grid.cols, 200);
        assert!(runtime.state.grid.has_page());

        let tree = runtime.accessibility_json();
        let scroll = find(&tree, "data-grid-scroll").expect("the grid is in the tree");
        assert_eq!(scroll["value"], "20000 rows × 200 columns");
        assert_eq!(scroll["name"], "customers");

        // One row node per mounted row; one cell node per built cell.
        let mut rows: Vec<u32> = Vec::new();
        let mut cells: Vec<(u32, u32)> = Vec::new();
        let mut headers: Vec<u32> = Vec::new();
        walk(scroll, &mut |node| {
            let Some(key) = node["key"].as_str() else { return };
            if let Some(row) = key.strip_prefix("grid-row-").and_then(|r| r.parse().ok()) {
                rows.push(row);
            } else if let Some(col) = key.strip_prefix("head-").and_then(|c| c.parse().ok()) {
                headers.push(col);
            } else if let Some((r, c)) = key
                .strip_prefix("cell-")
                .and_then(|rest| rest.split_once('-'))
                .and_then(|(r, c)| Some((r.parse().ok()?, c.parse().ok()?)))
            {
                cells.push((r, c));
            }
        });
        rows.sort_unstable();
        rows.dedup();
        headers.sort_unstable();
        headers.dedup();
        cells.sort_unstable();
        cells.dedup();

        // 20,000 rows mount a 21-row window at the top of the list: the first
        // frame's window over the 380-point sheet at the 28-point row height,
        // whatever the list's length — not the list.
        assert_eq!(
            rows,
            (0..21).collect::<Vec<u32>>(),
            "20,000 rows mount a 21-row window, not the list"
        );

        // A 200-column page builds one column window per row: 12 cells at rest —
        // COL_WINDOW with the frozen first column inside it — and 12 headers.
        assert_eq!(headers, (0..12).collect::<Vec<u32>>(), "the header builds the window");
        for row in rows.iter().copied() {
            let built: Vec<u32> = cells.iter().filter(|(r, _)| *r == row).map(|(_, c)| *c).collect();
            assert_eq!(built, (0..12).collect::<Vec<u32>>(), "row {row} builds the 12-column window");
        }
        assert!(cells.contains(&(0, 0)), "the focus cell is mounted");
    }

    #[test]
    fn the_unmigrated_areas_say_not_yet_migrated_and_keep_the_react_pages() {
        use crate::app_key;
        use serde_json::Value;

        fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
            if v["key"] == key {
                return Some(v);
            }
            v["children"].as_array()?.iter().find_map(|child| find(child, key))
        }
        fn names(v: &Value, out: &mut Vec<String>) {
            if let Some(name) = v["name"].as_str() {
                out.push(name.to_owned());
            }
            if let Some(children) = v["children"].as_array() {
                for child in children {
                    names(child, out);
                }
            }
        }

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        // The area is one of the sidebar's; pick it like the nav does.
        runtime.state.select(Section::Wapps);
        runtime.frame(0.0);

        let tree = runtime.accessibility_json();
        let nav = find(&tree, "rail-wapps").expect("the sidebar lists the area");
        assert_eq!(nav["role"], "tab");
        assert_eq!(nav["name"], crate::i18n::t("nav.extensions"));

        let page = find(&tree, "wapps-page").expect("the page is in the tree");
        assert!(find(&tree, "wapps-pending").is_some(), "the WAPPS entry is mounted");
        assert!(find(&tree, "onboarding-pending").is_some(), "the onboarding entry is mounted");

        // It says the status, and says where each area still lives.
        let mut texts = Vec::new();
        names(page, &mut texts);
        let joined = texts.join(" | ");
        assert!(joined.contains("Not yet migrated"), "the page says it: {joined}");
        assert!(
            joined.contains("The React page is still there"),
            "extensions entry points at React: {joined}"
        );
        assert!(joined.contains("The React wizard is still there"), "onboarding points at React: {joined}");
        assert!(joined.contains("do not go into Rune"), "WAPP iframes stay out: {joined}");

        // And nowhere does the page claim a replacement or a removal.
        let lowered = joined.to_lowercase();
        assert!(!lowered.contains("replaced"), "no claim of replacement: {joined}");
        assert!(!lowered.contains("removed"), "no claim of removal: {joined}");
    }

    #[test]
    fn a_chinese_grid_cell_is_not_a_box_under_the_apps_fonts() {
        // The grid runs under the app's font pool; a Chinese cell value
        // renders with the glyphs the pool shaped above, not fallback boxes.
        let env = Env {
            system_dark: true,
            scale: 1.0,
        };
        let text = rune::text::TextSystem::new(APP_FONTS, APP_FONT_FAMILIES);
        let mut runtime =
            Runtime::with_text(AppState::default(), view, (1440.0, 900.0), env, text);
        use datazen_ui_api::WorkspaceApi;
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor = rune::TextEdit::new(String::new());
        runtime.state.grid.editor.insert("中文");
        runtime.state.commit_grid_edit();
        runtime.frame(0.0);

        assert_eq!(runtime.state.grid.cell(0, 1), "中文");
        let mut probe = rune::text::TextSystem::new(APP_FONTS, APP_FONT_FAMILIES);
        let ids = glyph_ids(&mut probe, "中文");
        assert!(ids.iter().all(|id| *id != 0), "tofu: {ids:?}");
    }

    #[test]
    fn shell_renders_the_datazen_navigation() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        assert!(runtime.find("datazen-root").is_some());
        for section in [
            Section::Connections,
            Section::Workflows,
            Section::Dashboards,
            Section::Workspace,
            Section::Wapps,
            Section::Settings,
        ] {
            let id = format!("rail-{}", section.key());
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
        assert!(runtime.find("rail-tools").is_none(), "tools not on the main rail");
    }

    #[test]
    fn connections_home_shows_hero_cards_and_recent_queries_section() {
        // A1: the connections page renders a hero summary, connection cards,
        // and a recent-queries section (empty state when no history source).
        let mut rt = runtime();
        rt.frame(0.0);
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        assert!(
            keys.iter().any(|k| k == "connections-home-hero"),
            "hero present: {keys:?}"
        );
        assert!(
            keys.iter().any(|k| k == "connections-home-title"),
            "hero title present"
        );
        assert!(
            keys.iter().any(|k| k == "connections-home-subtitle"),
            "hero subtitle present"
        );
        assert!(
            keys.iter().any(|k| k == "connections-card-list"),
            "card list present"
        );
        assert!(
            keys.iter().any(|k| k == "connections-section-title"),
            "connections section title present"
        );
        assert!(
            keys.iter().any(|k| k.starts_with("connection-")),
            "connection cards rendered: {keys:?}"
        );
        assert!(
            keys.iter().any(|k| k == "connections-recent-queries"),
            "recent queries section present"
        );
        assert!(
            keys.iter().any(|k| k == "connections-recent-empty"),
            "recent queries empty state present without history source"
        );
        let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
        assert!(
            texts.iter().any(|t| t.contains(crate::i18n::t("connWin.home.selectConnectionTitle"))),
            "hero heading text present: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Your connections")),
            "connections section label present: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Recent queries")),
            "recent queries label present: {texts:?}"
        );
    }

    #[test]
    fn connections_home_without_connections_centers_the_empty_state() {
        // A1: zero connections renders the centered empty state with the
        // import and create-first actions (React State 1).
        let mut rt = runtime();
        rt.frame(0.0);
        rt.state.desktop_connections = Some(Vec::new());
        rt.settle_all();
        rt.frame(0.0);
        assert!(
            rt.find("connections-empty").is_some(),
            "the empty state is on the page"
        );
        let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
        assert!(
            texts.iter().any(|t| t.contains("No connections yet")),
            "the empty state's title is present: {texts:?}"
        );
        assert!(
            rt.find("import-connections").is_some(),
            "the empty state keeps the import press"
        );
        assert!(
            rt.find("connections-create-first").is_some(),
            "the create-first press is present"
        );
        let (x, y) = rt.find("connections-create-first").expect("create first");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(
            rt.find("connection-dialog").is_some(),
            "the form dialog opens from the empty state"
        );
    }

    #[test]
    fn the_home_filter_takes_a_caret_from_a_click() {
        // Rune focuses a text box only when it can reach the edit it binds
        // (`can_focus` asks for the input's accessor, which only `on_edit`
        // sets). The header filter shipped without that binding, so a click
        // left no caret and typing went nowhere; bind it and the click types.
        let mut rt = runtime();
        rt.frame(0.0);
        let (x, y) = rt.find("home-connections-filter").expect("the header filter");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.paste("replica");
        rt.settle_all();
        rt.frame(0.0);
        assert_eq!(
            rt.state.connection_search.display().0,
            "replica",
            "a click puts a caret in the header filter and typing reaches it"
        );
        assert!(
            rt.find("connection-replica").is_some(),
            "the typed filter narrows the cards"
        );
        assert!(
            rt.find("connection-local").is_none(),
            "the typed filter drops the other cards"
        );
    }

    #[test]
    fn connections_home_filters_and_pages_the_card_list() {
        // A1: the header filter narrows the cards, a miss renders the
        // no-match state, and more than six connections page behind show all.
        let mut rt = runtime();
        rt.frame(0.0);

        rt.state.connection_search = rune::TextEdit::new("replica");
        rt.settle_all();
        rt.frame(0.0);
        assert!(
            rt.find("connection-replica").is_some(),
            "the matching card stays"
        );
        assert!(
            rt.find("connection-local").is_none(),
            "the filter drops non-matches"
        );

        rt.state.connection_search = rune::TextEdit::new("zzz");
        rt.settle_all();
        rt.frame(0.0);
        assert!(
            rt.find("connections-no-match").is_some(),
            "a filter miss shows the no-match state"
        );

        let mut many: Vec<ConnectionProfile> = (0..7)
            .map(|i| ConnectionProfile {
                id: format!("page-{i}"),
                name: format!("Conn {i}"),
                engine: "sqlite".into(),
            })
            .collect();
        many.push(ConnectionProfile { id: "local".into(), name: "Local Postgres".into(), engine: "postgres".into() });
        rt.state.desktop_connections = Some(many);
        rt.state.connection_search = rune::TextEdit::new(String::new());
        rt.state.connections_show_all = false;
        rt.settle_all();
        rt.frame(0.0);
        let cards = |rt: &Runtime<AppState>| {
            // Card rows are `connection-{id}`; the page also carries the
            // header/new/import keys that share the prefix.
            const NOT_CARDS: [&str; 4] = [
                "connection-list",
                "connection-new",
                "connection-import",
                "connection-import-form",
            ];
            rt.elements()
                .iter()
                .filter(|e| {
                    matches!(&e.key, Some(k) if k.starts_with("connection-")
                        && !NOT_CARDS.contains(&k.as_str()))
                })
                .count()
        };
        assert_eq!(cards(&rt), 6, "six cards before show all");
        let (x, y) = rt.find("connections-show-all").expect("the show-all row");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(cards(&rt), 8, "every card after show all");
    }

    #[test]
    fn connection_cards_show_the_status_pill_and_the_connected_open_button() {
        // A1: rows render the status pill; an open session flips the row to
        // the Connected pill with the Open button.
        let mut rt = runtime();
        rt.frame(0.0);
        assert!(
            rt.find("conn-status-local").is_some(),
            "the status pill is on the row"
        );
        let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
        assert!(
            texts.iter().any(|t| t == crate::i18n::t("connWin.home.status.offline")),
            "an unopened row reads offline: {texts:?}"
        );

        // The live host's open session marks the row connected.
        rt.state.desktop_connection = Some("local".into());
        rt.settle_all();
        rt.frame(0.0);
        let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
        assert!(
            texts.iter().any(|t| t == crate::i18n::t("conn.connected")),
            "the opened row reads connected: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == crate::i18n::t("connWin.home.connections.open")),
            "the opened row's button says Open: {texts:?}"
        );
    }

    #[test]
    fn the_sidebar_lists_the_primary_sections() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        for key in [
            "connections",
            "workflows",
            "dashboards",
            "workspace",
            "wapps",
            "settings",
        ] {
            let id = format!("rail-{key}");
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
        assert!(
            runtime.find("rail-tools").is_none(),
            "tools is not part of the React-aligned main rail"
        );
        for key in ["sql-editor", "data-browser", "schema"] {
            let id = format!("rail-{key}");
            assert!(runtime.find(&id).is_none(), "{id} is not a product page anymore");
        }
    }

    #[test]
    fn clicking_navigation_changes_the_active_section() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime
            .find("rail-workspace")
            .expect("Workspace navigation item should exist");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(runtime.state.active, Section::Workspace);
    }

    #[test]
    fn a_workspace_without_a_tab_or_a_loaded_page_is_not_the_demo_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("rail-workspace").expect("workspace navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Workspace);
        assert!(runtime.find("workspace-empty").is_some(), "the workspace has an empty state");
        assert!(runtime.find("data-grid").is_none(), "the POC demo grid is not a product page");
        assert!(runtime.find("grid-row-0").is_none(), "no demo row is mounted");
        assert!(runtime.find("datazen-preview-card").is_none(), "the placeholder card is gone");
    }

    #[test]
    fn sql_editor_scrolls_a_surface_wider_than_the_viewport() {
        let mut runtime = runtime();
        runtime.state.open_query_tab("Query");
        runtime.frame(0.0);

        let field = runtime.presented("sql-editor-field").expect("editor field");
        let scroll = runtime.presented("sql-editor-scroll").expect("editor scroll");
        assert!(field.0[2] > scroll.0[2] + 100.0, "field {} viewport {}", field.0[2], scroll.0[2]);
    }

    #[test]
    fn chinese_ime_is_not_part_of_the_document_until_commit() {
        use crate::sql_editor::on_key;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(|state, ev, tx| {
            state.active == Section::Workspace && query_selected(state) && on_key(&mut state.sql, ev, tx)
        });
        runtime.state.open_query_tab("Query");
        runtime.state.sql.focus();
        runtime.frame(0.0);
        assert!(runtime.presented("sql-caret").is_some(), "main caret is the input-method cursor");

        let before = runtime.state.sql.doc().to_owned();
        runtime.state.sql.preedit("中");
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), before);
        assert_eq!(runtime.state.sql.composition(), Some("中"));

        runtime.state.sql.commit("中文");
        runtime.frame(0.0);
        assert!(runtime.state.sql.composition().is_none());
        assert!(runtime.state.sql.doc().starts_with("中文"), "{}", runtime.state.sql.doc());

        runtime.key(KeyEvent { key: Key::Text("Q".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with("中文Q"), "{}", runtime.state.sql.doc());
    }

    #[test]
    fn data_grid_mounts_a_row_window_and_a_column_window() {
        use crate::app_poll;
        use datazen_ui_api::TablePage;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        let columns: Vec<String> = (0..200).map(|index| format!("c{index}")).collect();
        let rows: Vec<Vec<String>> = (0..6000)
            .map(|row| columns.iter().map(|col| format!("{col}-{row}")).collect())
            .collect();
        runtime.state.grid.load_page(&TablePage {
            table: "wide".into(),
            columns,
            rows,
            page: 0,
            page_size: 6000,
            total_rows: 6000,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        });
        runtime.state.active = Section::Workspace;
        runtime.frame(0.0);

        assert!(runtime.find("grid-row-0").is_some());
        assert!(runtime.find("cell-0-0").is_some());
        assert!(runtime.find("cell-0-11").is_some());
        assert!(runtime.find("grid-row-5000").is_none(), "rows far outside the viewport are not built");
        assert!(runtime.find("cell-0-40").is_none(), "columns outside the window are not built");

        let mut mounted = 0;
        for index in 0..200 {
            if runtime.find(&format!("grid-row-{index}")).is_some() {
                mounted += 1;
            }
        }
        assert!(mounted > 0 && mounted < 80, "mounted rows {mounted}");

        runtime.state.grid.scroll_x().set(crate::data_grid::COL_W * 50.0);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.origin_col(), 50);
        assert!(runtime.find("cell-0-0").is_some(), "frozen column");
        assert!(runtime.find("cell-0-50").is_some());
        assert!(runtime.find("cell-0-11").is_none());
    }

    #[test]
    fn data_grid_edits_a_cell_and_copies_the_selection() {
        use crate::data_grid::on_key;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(|state, ev, tx| {
            state.active == Section::Workspace && !query_selected(state) && on_key(&mut state.grid, ev, tx)
        });
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("cell-0-1").expect("cell");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.focus(), (0, 1));

        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.grid.is_editing());
        // The second Enter is the production commit: the focused input's on_submit
        // commits the cell and blurs, so later Command keys still reach the grid.
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert!(!runtime.state.grid.is_editing());

        runtime.state.grid.paste_tsv("A\tB");
        assert_eq!(runtime.state.grid.cell(0, 1), "A");
        assert_eq!(runtime.state.grid.cell(0, 2), "B");
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        let copied = runtime.copy_request.as_ref().map(|copy| copy.content().text().to_owned());
        assert_eq!(copied.as_deref(), Some("A"));
    }

    #[test]
    fn opening_a_connection_loads_the_table_page_into_the_grid() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Workspace);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert_eq!(runtime.state.grid.column_name(1), Some("name"));
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        assert_eq!(
            runtime.state.workspace.open_table("customers", 0, TABLE_PAGE_SIZE, None, None).unwrap().rows[0][1],
            "customer-0-edited"
        );
    }

    #[test]
    fn opening_a_table_loads_the_first_page_of_the_table() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let grid = &runtime.state.grid;
        assert_eq!(grid.page(), 0);
        assert_eq!(grid.page_size(), TABLE_PAGE_SIZE);
        assert_eq!(grid.total_rows(), 40);
        assert_eq!(grid.rows, 25, "only the page is loaded, not the whole table");
        assert_eq!(grid.cell(24, 1), "customer-24");
        assert_eq!(grid.cell(0, 1), "customer-0");
    }

    #[test]
    fn the_footer_pages_through_the_table_and_stops_at_the_edges() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        assert!(runtime.find("grid-footer").is_some());
        assert!(runtime.find("page-prev").is_some());
        assert!(runtime.find("page-next").is_some());

        let (x, y) = runtime.find("page-prev").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0, "prev on the first page is a no-op");

        let (x, y) = runtime.find("page-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);
        assert_eq!(runtime.state.grid.rows, 15);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-25");

        let (x, y) = runtime.find("page-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1, "next on the last page is a no-op");

        let (x, y) = runtime.find("page-prev").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert!(runtime.find("cell-0-1").is_some(), "the frozen column is still built");
    }

    #[test]
    fn clicking_a_header_cycles_none_ascending_descending() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        // Page to the second page first: a sort must return to the first page.
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);

        let (x, y) = runtime.find("head-1").expect("name header");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0, "sorting returns to the first page");
        assert_eq!(runtime.state.grid.sort(), Some((1, false)));
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert_eq!(runtime.state.grid.cell(2, 1), "customer-10", "lexicographic ascending order");

        let (x, y) = runtime.find("head-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.sort(), Some((1, true)));
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-9", "descending starts at the max");
        assert_eq!(runtime.state.grid.cell(6, 1), "customer-39");

        let (x, y) = runtime.find("head-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.sort(), None);
        assert_eq!(runtime.state.grid.cell(2, 1), "customer-2", "storage order returns");
    }
