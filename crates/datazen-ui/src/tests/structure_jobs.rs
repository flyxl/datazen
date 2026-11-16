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
    fn disconnect_clears_the_tree_and_blocks_editing() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("disconnect").is_none());
        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        assert!(runtime.state.grid.is_editable());

        runtime.frame(1.0);
        let (x, y) = runtime.find("disconnect").expect("disconnect");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.state.workspace.session().is_none());
        assert!(runtime.state.tabs.tabs().is_empty());
        assert!(runtime.find("tree-table-customers").is_none());
        assert!(!runtime.state.grid.is_editable());
        runtime.state.grid.begin_edit();
        assert!(!runtime.state.grid.is_editing());
    }

    /// N1's test host: it takes every job and answers each on the poll after
    /// the next one — one frame of work, then the delivery. The canned pages
    /// carry the requesting generation, so one read's answer is tellable
    /// from another's.
    use datazen_ui_api::{
        AdapterDelivery, AdapterJob, Catalog, ColumnFilter, ColumnSort, ConnectionProfile,
        DashboardWidget, DatabaseNode, DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, SchemaNode,
        TablePage,
    };


    #[test]
    fn an_accepted_read_job_keeps_the_previous_page_until_the_delivery_lands() {
        use crate::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        let before = runtime.state.grid.cell(0, 0).to_owned();
        assert!(runtime.state.grid.pages() > 1, "the demo table pages");

        // The host takes the page turn as a job: the marker shows, the page
        // the grid already loaded stays exactly as it was.
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.next_table_page();
        assert!(runtime.state.table_loading.is_some(), "the read job is in flight");
        assert_eq!(
            runtime.state.grid.cell(0, 0), before,
            "the previous page stays while the job runs"
        );
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("table-loading").is_some(), "the footer says the read is in flight");
        assert_eq!(runtime.state.grid.cell(0, 0), before, "still the previous page");

        // The delivery lands on the poll after the next one.
        runtime.frame(0.0);
        assert!(runtime.state.table_loading.is_none(), "the marker leaves with the delivery");
        assert_eq!(
            runtime.state.grid.cell(0, 0),
            format!("read-{}", runtime.state.read_generation),
            "the delivered page replaces the kept one"
        );
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("table-loading").is_none(), "the marker leaves the footer");
    }

    #[test]
    fn a_table_open_sends_one_structure_job_and_the_delivery_fills_the_cache() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.open_table_tab("events").unwrap();
        // The job went out; the entry waits on its generation.
        assert_eq!(runtime.state.structure_generation, 1, "the open asked once");
        let entry = runtime.state.structures.get("events").expect("the entry waits");
        assert!(entry.loading, "the read is in flight");
        assert!(entry.data.is_none(), "nothing landed yet");

        // The delivery lands on the poll after the next one.
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.frame(0.0);
        let entry = runtime.state.structures.get("events").expect("the entry stays");
        assert!(!entry.loading, "the marker leaves with the delivery");
        let data = entry.data.as_ref().expect("the four segments landed");
        assert_eq!(data.columns.first().map(|column| column.name.as_str()), Some("job"));
        assert!(data.indexes.first().is_some_and(|index| index.unique));
        assert!(!data.ddl.is_empty(), "the DDL segment renders");
    }

    #[test]
    fn a_late_structure_delivery_for_a_superseded_generation_is_dropped() {
        use crate::app_poll;
        use crate::TableView;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.open_table_tab("events").unwrap();
        // The view switch supersedes the open's read: the entry moves to
        // generation 2 while the generation-1 answer is still in flight.
        runtime.state.set_table_view(TableView::Columns);
        assert_eq!(
            runtime.state.structures.get("events").map(|entry| entry.generation),
            Some(2),
            "the view switch restarts the read"
        );

        // Both answers arrive on the same poll, the stale one first.
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.frame(0.0);
        let entry = runtime.state.structures.get("events").expect("the entry stays");
        assert_eq!(entry.generation, 2, "the current generation holds the entry");
        assert!(!entry.loading, "the current generation's answer landed");
        assert!(entry.data.is_some(), "the stale answer did not overwrite it");
    }

    #[test]
    fn the_outbound_fk_key_reads_the_cache_and_is_honest_while_it_lands() {
        use crate::app_poll;
        use crate::StructureState;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.open_table_tab("events").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.frame(0.0);
        // The grid has its page; the focused column is the job's one.
        assert_eq!(runtime.state.grid.column_name(0), Some("job"));

        // A structure still in flight answers with a sentence, not a no-op.
        runtime.state.structures.insert(
            "events".into(),
            StructureState { generation: 9, loading: true, error: None, data: None },
        );
        runtime.state.go_to_referenced_row();
        assert!(
            runtime.state.fk_error.as_deref().is_some_and(|error| error.contains("loading")),
            "the sentence says the structure is loading: {:?}",
            runtime.state.fk_error
        );

        // The landed structure serves the navigation: the key's own pairing
        // opens the target table filtered on the focused cell.
        runtime.state.structures.insert(
            "events".into(),
            StructureState {
                generation: 10,
                loading: false,
                error: None,
                data: Some(datazen_ui_api::TableStructure {
                    ddl: String::new(),
                    columns: Vec::new(),
                    indexes: Vec::new(),
                    foreign_keys: vec![datazen_ui_api::ForeignKeySpec {
                        name: "fk_job".into(),
                        columns: vec!["job".into()],
                        target_table: "users".into(),
                        target_columns: vec!["id".into()],
                    }],
                }),
            },
        );
        runtime.state.go_to_referenced_row();
        assert!(runtime.state.fk_error.is_none(), "{:?}", runtime.state.fk_error);
        assert_eq!(
            runtime.state.tabs.selected().map(|tab| tab.title.as_str()),
            Some("users"),
            "the referenced table opens"
        );
        assert!(
            runtime.state.filter.as_ref().is_some_and(|filter| filter.column == "id"),
            "the filter presets the target column"
        );
    }

    /// E1: a host that takes structure jobs must never be asked the four
    /// synchronous structure reads — the render path reads the cache the
    /// job fills. Every sync structure method panics, so any adapter call
    /// on the render path fails the test by failing the process.


    #[test]
    fn the_structure_views_render_from_the_cache_and_never_touch_the_adapter() {
        use crate::app_poll;
        use crate::TableView;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.desktop = Box::new(StructureStrictHost::new());
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);

        // The four structure views and the data view render; a synchronous
        // structure read on the render path would panic inside the host.
        for view in [
            TableView::Data,
            TableView::Columns,
            TableView::Indexes,
            TableView::ForeignKeys,
            TableView::Ddl,
        ] {
            runtime.state.set_table_view(view);
            runtime.frame(0.0);
        }

        // The ER page renders its boxes from the same cache.
        runtime.state.open_er();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("er-box-customers").is_some(), "the ER box renders");
        assert!(
            runtime.find("er-pending-customers").is_none(),
            "the box's structure landed, not the pending hint"
        );
    }

    /// E2: a host with one widget that takes bars jobs. The synchronous
    /// bars read answers nothing — the chart's render must not need it —
    /// and the bars answer on the second poll, so the in-flight state
    /// renders a frame of its own.


    #[test]
    fn entering_the_dashboards_page_sends_a_bars_job_and_the_chart_renders_the_answer() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(DashboardsJobHost::new());
        runtime.state.select(Section::Dashboards);
        // One widget, one bars job; the chart says the read is in flight.
        assert_eq!(runtime.state.widget_bars.len(), 1, "one widget, one bars read");
        assert!(
            runtime.state.widget_bars.get(&7).is_some_and(|entry| entry.loading),
            "the bars read is in flight"
        );
        runtime.frame(0.0);
        assert!(
            runtime.find("widget-chart-notice-7").is_some(),
            "the in-flight read says so"
        );

        // The delivery lands and the bars draw.
        runtime.frame(0.0);
        let entry = runtime.state.widget_bars.get(&7).expect("the entry stays");
        assert!(!entry.loading, "the marker leaves with the delivery");
        assert_eq!(entry.bars.as_deref(), Some([("mon".to_string(), 3u32)].as_slice()));
        runtime.frame(0.0);
        assert!(runtime.find("widget-block-7-mon").is_some(), "the delivered bars draw");
    }

    #[test]
    fn a_late_bars_delivery_for_a_superseded_generation_is_dropped() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(DashboardsJobHost::new());
        runtime.state.select(Section::Dashboards);
        // The refresh restarts the read: the entry moves to generation 2
        // while the generation-1 answer is still in flight.
        runtime.state.refresh_widget_bars();
        assert_eq!(
            runtime.state.widget_bars.get(&7).map(|entry| entry.generation),
            Some(2),
            "the refresh restarts the read"
        );

        // Both answers arrive on the same poll, the stale one first.
        runtime.frame(0.0);
        runtime.frame(0.0);
        let entry = runtime.state.widget_bars.get(&7).expect("the entry stays");
        assert_eq!(entry.generation, 2);
        assert!(entry.bars.is_some(), "the stale answer did not overwrite it");
    }

    #[test]
    fn the_inbound_list_arrives_as_a_job_and_only_for_the_loaded_table() {
        use crate::app_poll;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.open_table_tab("events").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.frame(0.0);
        // The grid has its page; the ask goes out as a job (E3).
        runtime.state.load_inbound_refs();
        assert!(runtime.state.inbound_loading.is_some(), "the scan is in flight");
        assert!(runtime.state.inbound_refs.is_none(), "nothing landed yet");

        // The delivery lands: the list describes the loaded table.
        runtime.frame(0.0);
        runtime.frame(0.0);
        assert!(runtime.state.inbound_loading.is_none(), "the marker leaves with the delivery");
        let refs = runtime.state.inbound_refs.as_ref().expect("the list lands");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].table, "orders");
        runtime.frame(0.0);
        assert!(runtime.find("fk-inbound-0").is_some(), "the delivered list renders");

        // A delivery for another table drops: the grid moved on.
        runtime.state.deliver_host_job(AdapterDelivery::InboundRefsRead {
            generation: runtime.state.inbound_generation,
            table: "elsewhere".into(),
            refs: Ok(vec![datazen_ui_api::InboundRef {
                table: "ghost".into(),
                column: "c".into(),
                referenced_column: "id".into(),
                constraint: "fk".into(),
            }]),
        });
        assert!(
            runtime.state.inbound_refs.as_ref().is_some_and(|refs| refs[0].table == "orders"),
            "a list for another table does not land"
        );

        // A stale generation drops too: a newer ask superseded the old one.
        let stale = runtime.state.inbound_generation;
        runtime.state.load_inbound_refs();
        runtime.state.deliver_host_job(AdapterDelivery::InboundRefsRead {
            generation: stale,
            table: "events".into(),
            refs: Ok(Vec::new()),
        });
        assert!(
            runtime.state.inbound_loading.is_some(),
            "the stale answer did not clear the newer ask's marker"
        );
    }
