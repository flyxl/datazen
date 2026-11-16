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
    fn the_connection_form_saves_through_the_host_and_the_whitelist_refuses() {
        struct FormHost {
            saved: std::rc::Rc<std::cell::RefCell<Vec<datazen_ui_api::ConnectionDraft>>>,
        }

        impl DesktopAdapter for FormHost {
            fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
                Err(DESKTOP_NOT_IMPLEMENTED.into())
            }

            fn read_table_page(
                &mut self,
                _: &str,
                _: u32,
                _: u32,
                _: Option<&ColumnSort>,
                _: Option<&ColumnFilter>,
            ) -> Result<TablePage, String> {
                Err(DESKTOP_NOT_IMPLEMENTED.into())
            }

            fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
                Err(DESKTOP_NOT_IMPLEMENTED.into())
            }

            fn available_engines(&self) -> Vec<String> {
                vec!["sqlite".into(), "postgresql".into()]
            }

            fn save_connection(
                &mut self,
                draft: &datazen_ui_api::ConnectionDraft,
            ) -> Result<(), String> {
                self.saved.borrow_mut().push(draft.clone());
                Ok(())
            }
        }

        let saved = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut state = AppState::default();
        state.desktop =
            Box::new(FormHost { saved: std::rc::Rc::clone(&saved) });
        state.connection_name = rune::TextEdit::new("Warehouse");
        state.connection_engine = rune::TextEdit::new("postgresql");
        state.connection_address = rune::TextEdit::new("db.example.com:5432");
        state.connection_username = rune::TextEdit::new("app");
        state.connection_password = rune::TextEdit::new("s3cret");
        state.connection_database = rune::TextEdit::new("analytics");
        state.save_connection_form();
        assert!(state.connection_error.is_none(), "{:?}", state.connection_error);
        assert_eq!(saved.borrow().len(), 1, "the draft reached the host");
        let draft = saved.borrow()[0].clone();
        assert_eq!(draft.engine, "postgresql");
        assert_eq!(draft.address, "db.example.com:5432");
        assert_eq!(draft.username, "app");
        assert_eq!(draft.password, "s3cret");
        assert_eq!(draft.database, "analytics");
        assert_eq!(state.connection_name.text, "", "the form cleared");

        // The whitelist: an engine the host does not link is refused before
        // the host is asked; the sentence names the linked set.
        state.connection_engine = rune::TextEdit::new("oracle");
        state.connection_address = rune::TextEdit::new("db.example.com");
        state.save_connection_form();
        assert_eq!(saved.borrow().len(), 1, "nothing more reached the host");
        let error = state.connection_error.as_deref().expect("the refusal shows");
        assert!(error.contains("not among them"), "the sentence names it: {error}");

        // The empty engine asks for one — the live host links real types,
        // not the memory port's demo default.
        state.connection_error = None;
        state.connection_engine = rune::TextEdit::new("  ");
        state.connection_address = rune::TextEdit::new("db.example.com");
        state.save_connection_form();
        assert_eq!(saved.borrow().len(), 1, "still nothing more reached the host");
        let error = state.connection_error.as_deref().expect("the empty engine asks");
        assert!(error.contains("an engine is required"), "the sentence names it: {error}");
    }

    #[test]
    fn a_stale_read_delivery_is_dropped() {
        use crate::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);

        // Two reads in a row: the second supersedes the first, and both
        // answers arrive together — the stale one first.
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.reload_table_page();
        let stale = runtime.state.read_generation;
        runtime.state.reload_table_page();
        let current = runtime.state.read_generation;
        assert_ne!(stale, current, "the second read started its own generation");

        runtime.frame(0.0);
        assert!(runtime.state.table_loading.is_some(), "the newer read is in flight");
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.grid.cell(0, 0),
            format!("read-{current}"),
            "only the current generation's page lands"
        );
        assert_ne!(
            runtime.state.grid.cell(0, 0),
            format!("read-{stale}"),
            "the superseded read does not overwrite it"
        );
        assert!(runtime.state.table_loading.is_none());
    }

    #[test]
    fn an_accepted_query_job_lands_in_the_result_grid_when_it_delivers() {
        use crate::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.settle_all();
        runtime.frame(0.0);

        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.run_query();
        let generation = runtime.state.query_generation;
        assert!(runtime.state.query_loading.is_some(), "the query job is in flight");
        assert!(runtime.state.result_grid.is_none(), "no result before the delivery");
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("query-loading").is_some(), "the result area says the query runs");

        runtime.frame(0.0);
        assert!(runtime.state.query_loading.is_none(), "the marker leaves with the delivery");
        let grid = runtime.state.result_grid.as_ref().expect("the delivery landed");
        assert_eq!(grid.cell(0, 0), format!("query-{generation}"));
    }

    #[test]
    fn an_accepted_commit_job_keeps_the_dirty_marks_until_the_delivery() {
        use crate::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        assert_eq!(runtime.state.grid.dirty_cells(), 1, "one staged edit");

        // The host takes the batch as a job: the dirty marks stay and the
        // footer says the save is in flight.
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.commit_staged_edits();
        assert!(runtime.state.commit_loading.is_some(), "the commit job is in flight");
        assert_eq!(runtime.state.grid.dirty_cells(), 1, "the dirty marks stay while it runs");
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("commit-loading").is_some(), "the footer says the save is in flight");

        // The delivery lands: the marks clear through the reload the commit
        // asked for (itself a job on this host, delivered two polls later).
        runtime.frame(0.0);
        assert!(runtime.state.commit_loading.is_none(), "the commit marker leaves");
        assert!(runtime.state.table_loading.is_some(), "the reload after the commit is a read job");
        runtime.frame(0.0);
        runtime.frame(0.0);
        assert!(runtime.state.table_loading.is_none());
        assert_eq!(runtime.state.grid.dirty_cells(), 0, "the reloaded page carries no staged edits");
    }

    #[test]
    fn a_failed_commit_delivery_keeps_the_dirty_marks() {
        use crate::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        let before = runtime.state.grid.cell(0, 1).to_owned();

        runtime.state.desktop = Box::new(JobAdapter::failing_commit());
        runtime.state.commit_staged_edits();
        runtime.frame(0.0);
        runtime.frame(0.0);

        assert!(runtime.state.commit_loading.is_none(), "the marker leaves with the failure");
        assert_eq!(runtime.state.grid.dirty_cells(), 1, "a real failure keeps the dirty marks");
        assert_eq!(
            runtime.state.grid.cell(0, 1), before,
            "no reload happened, the staged value is untouched"
        );
        assert!(runtime.state.table_loading.is_none(), "a failed commit does not reload");
    }

    #[test]
    fn an_accepted_open_job_fills_the_tree_with_the_live_catalog() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.frame(0.0);

        // The live host takes the connect as a job: the connections page
        // keeps its list and says the connect is in flight.
        runtime.state.desktop = Box::new(JobAdapter::new());
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.connect_loading.is_some(), "the open job is in flight");
        assert_eq!(
            runtime.state.active,
            Section::Connections,
            "the page waits for the catalog"
        );
        assert!(runtime.find("connect-loading").is_some(), "the page says the connect runs");

        // The catalog lands: the workspace opens with the live tree, and the
        // first table's read is itself a job on this host.
        runtime.frame(0.0);
        assert!(runtime.state.connect_loading.is_none(), "the marker leaves with the delivery");
        assert_eq!(runtime.state.active, Section::Workspace);
        assert_eq!(runtime.state.desktop_connection.as_deref(), Some("local"));
        assert!(runtime.state.table_loading.is_some(), "the first table's read is a job too");
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.frame(0.0);
        assert!(runtime.find("tree-db-main").is_some(), "the live catalog's database");
        assert!(runtime.find("tree-table-users").is_some(), "the live catalog's table");
        assert!(runtime.find("tree-table-events").is_some());
        assert!(
            runtime.find("tree-table-customers").is_none(),
            "the memory port's demo tables are not the live tree"
        );

        // The disconnect button shows for a live session (the memory port's
        // session never opened), and disconnecting takes the catalog with it.
        let (x, y) = runtime.find("disconnect").expect("the disconnect button shows");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.desktop_catalog.is_none(), "the live catalog left with the session");
        assert!(runtime.state.desktop_connection.is_none());
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-users").is_none(), "the tree is empty again");
    }

    #[test]
    fn a_failed_open_keeps_the_connections_page_and_shows_the_sentence() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.frame(0.0);

        runtime.state.desktop = Box::new(JobAdapter::failing_open());
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.connect_loading.is_some(), "the open job is in flight");
        runtime.frame(0.0);

        assert!(runtime.state.connect_loading.is_none(), "the marker leaves with the failure");
        assert_eq!(
            runtime.state.active,
            Section::Connections,
            "a failed open never leaves the connections page"
        );
        assert_eq!(
            runtime.state.desktop_error.as_deref(),
            Some("cannot open 'local'"),
            "the sentence lands in the adapter's notice"
        );
        let message = runtime
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("desktop-adapter-message"))
            .and_then(|el| el.text)
            .expect("the failure sentence is displayed");
        assert_eq!(message, "cannot open 'local'");
        assert!(
            runtime.find("tree-table-users").is_none(),
            "no live tree without a session"
        );
    }
    #[test]
    fn the_commit_batch_addresses_rows_by_the_primary_key_with_originals() {
        use crate::data_grid::DataGrid;
        use datazen_ui_api::CellEditBatch;

        // A page the live host would serve: a real primary key rides along
        // (N6). The grid builds the commit batch from what it loaded.
        let mut grid = DataGrid::new();
        grid.load_page(&TablePage {
            table: "users".into(),
            columns: vec!["id".into(), "name".into()],
            rows: vec![vec!["7".into(), "alice".into()], vec!["8".into(), "bob".into()]],
            page: 0,
            page_size: 25,
            total_rows: 2,
            sort: None,
            filter: None,
            primary_key: vec!["id".into()],
            read_only: None,
        });

        // Nothing staged: no batch.
        assert!(grid.commit_batch().is_none(), "no edits, no batch");

        // Edit the first row's name and the second row's name.
        // The editor opens with the cell's value; insert appends (the same
        // semantics the B5 tests established).
        grid.select(0, 1);
        grid.begin_edit();
        grid.editor.insert("-carol");
        grid.commit_edit();
        grid.select(1, 1);
        grid.begin_edit();
        grid.editor.insert("-dave");
        grid.commit_edit();
        assert_eq!(grid.dirty_cells(), 2);

        let batch: CellEditBatch = grid.commit_batch().expect("the batch builds");
        assert_eq!(batch.table, "users");
        assert_eq!(batch.rows.len(), 2, "one entry per edited row");
        assert_eq!(batch.rows[0].index, 0);
        assert_eq!(
            batch.rows[0].identity,
            vec![("id".to_string(), "7".to_string())],
            "the row is addressed by the PK value the page showed"
        );
        assert_eq!(
            batch.rows[0].changes,
            vec![("name".to_string(), "alice".to_string(), "alice-carol".to_string())],
            "the change carries the original the page loaded and the staged value"
        );
        assert_eq!(
            batch.rows[1].identity,
            vec![("id".to_string(), "8".to_string())],
            "the second row by its own PK"
        );
        assert_eq!(batch.rows[1].changes[0].2, "bob-dave");

        // A page without a primary key builds identity-less rows: the host's
        // commit use case refuses them, which is the honest answer (and the
        // memory port's pages, which carry none, never take this path — the
        // declined commit falls back to the port's staged edits).
        let mut keyless = DataGrid::new();
        keyless.load_page(&TablePage {
            table: "log".into(),
            columns: vec!["at".into(), "what".into()],
            rows: vec![vec!["t0".into(), "boot".into()]],
            page: 0,
            page_size: 25,
            total_rows: 1,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        });
        keyless.select(0, 1);
        keyless.begin_edit();
        keyless.editor.insert("halt");
        keyless.commit_edit();
        let batch = keyless.commit_batch().expect("the batch still builds");
        assert!(batch.rows[0].identity.is_empty(), "no PK on the page, no identity");
    }

    #[test]
    fn layout_sizes_round_trip_through_the_settings_ports() {
        // The memory port answers while the demo desktop says not
        // implemented: the helpers ride the same fallback as the scalar
        // settings (L1).
        let mut state = AppState::default();
        assert_eq!(state.layout_size("navigator-width"), None, "a fresh app has no layout sizes");

        state.set_layout_size("navigator-width", 280).unwrap();
        assert_eq!(state.layout_size("navigator-width"), Some(280));
        state.set_layout_size("navigator-width", 300).unwrap();
        assert_eq!(state.layout_size("navigator-width"), Some(300), "the region's last size wins");
        state.set_layout_size("editor-height", 320).unwrap();
        assert_eq!(state.layout_size("editor-height"), Some(320));

        // One entry per region key, other regions coexist.
        let layouts = state
            .ui_settings()
            .into_iter()
            .filter(|s| matches!(s, Setting::Layout { .. }))
            .count();
        assert_eq!(layouts, 2, "one entry per region");

        // The port's rules apply on the UI path too: a bad key is refused
        // and nothing is stored.
        let error = state.set_layout_size("Navigator", 280).unwrap_err();
        assert!(error.contains("layout key"), "the refusal names the rule: {error}");
        assert_eq!(state.layout_size("Navigator"), None);
    }

    #[test]
    fn a_host_that_persists_settings_answers_the_reads_and_writes() {
        use datazen_ui_api::{Setting, SettingsApi};

        // The host's settings answer once it persists them (N8): the page
        // size the grid pages with comes from the host, and a write goes to
        // the host — the memory port never sees it.
        struct SettingsHost {
            settings: Vec<Setting>,
            writes: Vec<Setting>,
        }
        impl DesktopAdapter for SettingsHost {
            fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
                Ok(Vec::new())
            }
            fn read_table_page(
                &mut self,
                _: &str,
                _: u32,
                _: u32,
                _: Option<&ColumnSort>,
                _: Option<&ColumnFilter>,
            ) -> Result<TablePage, String> {
                Err(DESKTOP_NOT_IMPLEMENTED.into())
            }
            fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
                Err(DESKTOP_NOT_IMPLEMENTED.into())
            }
            fn read_ui_settings(&self) -> Result<Vec<Setting>, String> {
                Ok(self.settings.clone())
            }
            fn write_ui_setting(&mut self, setting: Setting) -> Result<(), String> {
                self.writes.push(setting.clone());
                let slot = |setting: &Setting| match setting {
                    Setting::Theme { .. } => "theme".to_string(),
                    Setting::TablePageSize { .. } => "tablePageSize".to_string(),
                    Setting::QueryMaxRows { .. } => "queryMaxRows".to_string(),
                    // One slot per region key, like the memory port.
                    Setting::Layout { key, .. } => format!("layout:{key}"),
                };
                self.settings.retain(|existing| slot(existing) != slot(&setting));
                self.settings.push(setting);
                Ok(())
            }
        }

        let mut runtime = runtime();
        runtime.state.desktop = Box::new(SettingsHost {
            settings: vec![Setting::TablePageSize { rows: 10 }],
            writes: Vec::new(),
        });

        // The grid pages with the host's page size, not the memory port's 25.
        assert_eq!(runtime.state.table_page_size(), 10);

        // A write reaches the host and becomes the value in force.
        runtime.state.write_ui_setting(Setting::TablePageSize { rows: 40 }).unwrap();
        assert_eq!(runtime.state.table_page_size(), 40);

        // The memory port behind the fallback is untouched: its own page
        // size is still the demo's 25.
        assert_eq!(
            SettingsApi::read_settings(&runtime.state.workspace)
                .iter()
                .find_map(|setting| match setting {
                    Setting::TablePageSize { rows } => Some(*rows),
                    _ => None,
                }),
            Some(25),
            "the memory port is the fallback, not a mirror"
        );
    }

    #[test]
    fn the_titlebar_renders_a_drag_band_label_and_theme_toggle() {
        // L3: the self-drawn chrome sits at the top of the root with the
        // section label and a theme toggle that flips through the settings
        // port.
        let mut rt = runtime();
        rt.frame(0.0);
        let elements = rt.elements();
        let keys: Vec<_> = elements.iter().filter_map(|e| e.key.as_deref()).collect();
        assert!(keys.contains(&"datazen-root"), "root present: {keys:?}");
        assert!(keys.contains(&"datazen-titlebar"), "titlebar band: {keys:?}");
        assert!(keys.contains(&"datazen-titlebar-label"), "section label in titlebar: {keys:?}");
        assert!(keys.contains(&"datazen-theme-toggle"), "theme toggle in titlebar: {keys:?}");
        assert!(!keys.contains(&"datazen-header"), "old in-page header is gone: {keys:?}");
        assert!(
            elements.iter().any(|e| e.key.as_deref() == Some("datazen-body")),
            "sidebar+page live under the titlebar body row"
        );
    }

    #[test]
    fn the_rail_renders_sections_toggles_collapse_and_persists() {
        // L4: 7 rail items + a collapse toggle; collapsed/expanded state
        // persists through the layout settings port and the rail renders.
        let mut rt = runtime();
        rt.frame(0.0);
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        assert!(keys.iter().any(|k| k == "datazen-rail"), "rail present: {keys:?}");
        for section in [
            Section::Connections,
            Section::Workflows,
            Section::Dashboards,
            Section::Workspace,
            Section::Wapps,
            Section::Settings,
        ] {
            let id = format!("rail-{}", section.key());
            assert!(keys.iter().any(|k| k == &id), "missing rail item: {id}");
        }
        assert!(
            !keys.iter().any(|k| k == "rail-tools"),
            "tools is not on the React-aligned rail"
        );
        assert!(keys.iter().any(|k| k == "rail-toggle"), "collapse toggle present");
        assert!(!keys.iter().any(|k| k == "datazen-sidebar"), "old sidebar is gone");

        // Default: expanded (112pt).
        assert!(rt.state.sidebar_expanded(), "expanded by default");
        assert_eq!(rt.state.sidebar_rail_width(), RAIL_WIDTH_EXPANDED);
        rt.frame(0.0);
        let rail_w = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("datazen-rail"))
            .map(|e| e.rect[2])
            .unwrap_or(0.0);
        assert!(
            (rail_w - RAIL_WIDTH_EXPANDED as f32).abs() < 1.0,
            "expanded rail width, got {rail_w}"
        );

        // Collapse/expand via the action and verify persistence.
        rt.state.toggle_sidebar_expanded();
        assert!(!rt.state.sidebar_expanded(), "collapsed after toggle");
        assert_eq!(rt.state.sidebar_rail_width(), RAIL_WIDTH_COLLAPSED);
        assert_eq!(
            rt.state.layout_size(SIDEBAR_RAIL_WIDTH_KEY),
            Some(RAIL_WIDTH_COLLAPSED),
            "collapse persisted"
        );
        rt.frame(1.0 / 60.0);
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("datazen-rail")));

        rt.state.toggle_sidebar_expanded();
        assert!(rt.state.sidebar_expanded(), "expanded again");
        assert_eq!(rt.state.sidebar_rail_width(), RAIL_WIDTH_EXPANDED);
        assert_eq!(rt.state.layout_size(SIDEBAR_RAIL_WIDTH_KEY), Some(RAIL_WIDTH_EXPANDED));
        rt.frame(1.0 / 60.0);
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("datazen-rail")));

        // A fresh runtime seeded from the same persisted width reads collapsed.
        let mut seeded = AppState::default();
        seeded.set_layout_size(SIDEBAR_RAIL_WIDTH_KEY, RAIL_WIDTH_COLLAPSED).unwrap();
        assert!(!seeded.sidebar_expanded());
        assert_eq!(seeded.sidebar_rail_width(), RAIL_WIDTH_COLLAPSED);
    }

    #[test]
    fn the_navigator_panel_holds_the_tree_and_persists_width() {
        // L4: the object tree moved into a resizable navigator panel in the
        // content area; its width persists through the layout port.
        let mut rt = runtime();
        rt.frame(0.0);
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        assert!(keys.iter().any(|k| k == "datazen-navigator"), "navigator panel: {keys:?}");
        assert!(keys.iter().any(|k| k == "object-tree"), "tree in navigator: {keys:?}");
        assert!(
            keys.iter().any(|k| k == "open-query"),
            "workspace action in navigator: {keys:?}"
        );
        assert!(
            keys.iter().any(|k| k.starts_with("splitter:") && k.contains(NAVIGATOR_WIDTH_KEY)),
            "navigator splitter present: {keys:?}"
        );

        assert_eq!(rt.state.navigator_width(), NAVIGATOR_WIDTH_DEFAULT, "default navigator width");
        rt.state.set_layout_size(NAVIGATOR_WIDTH_KEY, 340).unwrap();
        assert_eq!(rt.state.navigator_width(), 340, "persisted width reads back");
        // Out-of-range persisted values clamp to the resizable range.
        rt.state.set_layout_size(NAVIGATOR_WIDTH_KEY, 50).unwrap();
        assert_eq!(rt.state.navigator_width(), NAVIGATOR_WIDTH_MIN);
        rt.state.set_layout_size(NAVIGATOR_WIDTH_KEY, 9000).unwrap();
        assert_eq!(rt.state.navigator_width(), NAVIGATOR_WIDTH_MAX);
    }

    #[test]
    fn navigator_filter_filters_object_tree_nodes() {
        // A1 (tree): navigator exposes a filter input and the object tree
        // narrows to matching nodes while showing a clear empty state.
        let mut rt = runtime();
        rt.frame(0.0);
        assert!(
            rt.elements().iter().any(|e| e.key.as_deref() == Some("navigator-filter-input")),
            "navigator filter input present"
        );

        // Seed a catalog so the tree has content to filter.
        rt.state.desktop_catalog = Some(datazen_ui_api::Catalog {
            databases: vec![datazen_ui_api::DatabaseNode {
                name: "app".into(),
                schemas: vec![datazen_ui_api::SchemaNode {
                    name: "public".into(),
                    tables: vec!["users".into(), "orders".into()],
                }],
            }],
        });
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("tree-table-users")));
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("tree-table-orders")));

        rt.state.navigator_filter = rune::TextEdit::new("users".to_owned());
        rt.invalidate();
        rt.frame(0.0);
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("tree-table-users")));
        assert!(
            !rt.elements().iter().any(|e| e.key.as_deref() == Some("tree-table-orders")),
            "non-matching table filtered out"
        );

        rt.state.navigator_filter = rune::TextEdit::new("zzznomatch".to_owned());
        rt.invalidate();
        rt.frame(0.0);
        assert!(
            rt.elements().iter().any(|e| e.key.as_deref() == Some("object-tree-empty")),
            "no-matches empty state shown"
        );
    }

    #[test]
    fn navigator_connections_tree_shows_recent_and_all_groups_and_filters() {
        // A1 (tree): navigator shows a connections hierarchy with session-only
        // Recent and All groups; filter applies to connection names and opening
        // a connection records it as recent (UI state, no contract changes).
        let mut rt = runtime();
        rt.frame(0.0);
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        assert!(
            keys.iter().any(|k| k == "navigator-connections"),
            "connections tree present: {keys:?}"
        );
        assert!(keys.iter().any(|k| k == "nav-group-recent"), "recent group: {keys:?}");
        assert!(keys.iter().any(|k| k == "nav-group-all"), "all group: {keys:?}");
        assert!(
            keys.iter().any(|k| k == "nav-group-recent-empty"),
            "recent empty before opens: {keys:?}"
        );

        let visible = rt.state.visible_connections();
        assert!(!visible.is_empty(), "default connections available");
        let first = visible[0].clone();
        assert!(
            keys.iter().any(|k| k == &format!("nav-conn-{}", first.id)),
            "all-connections entry present: {keys:?}"
        );

        // Opening records a session-only recent entry (UI state).
        rt.state.open_connection(&first.id);
        assert_eq!(
            rt.state.recent_connection_ids.first().map(String::as_str),
            Some(first.id.as_str()),
            "recent connection recorded"
        );
        rt.invalidate();
        rt.frame(0.0);
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        assert!(
            keys.iter().any(|k| k == &format!("nav-conn-recent-{}", first.id)),
            "recent entry rendered: {keys:?}"
        );

        // Filter applies to connections (value search in navigator).
        rt.state.navigator_filter = rune::TextEdit::new("zzznomatch-conn".to_owned());
        rt.invalidate();
        rt.frame(0.0);
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
        assert!(
            keys.iter().any(|k| k == "nav-group-all-empty"),
            "all-connections filtered empty: {keys:?}"
        );
    }
