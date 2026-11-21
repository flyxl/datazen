use serde_json::{json, Value};

use super::*;
use crate::{FilterOperator, GridViewport};

#[derive(Default)]
struct MockHost {
    source: Option<SourcePanelInfo>,
    viewports: Vec<(String, GridViewport)>,
    panels: Vec<CreatedPanel>,
    seeds: Vec<(String, TableChangeContext, Vec<crate::FilterCondition>)>,
    nav_entries: Vec<NavEntry>,
    origins: Vec<(String, FkOrigin)>,
    counter: usize,
}

impl ReferencingRowsHost for MockHost {
    fn next_panel_id(&mut self) -> String {
        self.counter += 1;
        format!("tbl-{}", self.counter)
    }
    fn source_panel(&self, _: &str) -> Option<SourcePanelInfo> {
        self.source.clone()
    }
    fn record_viewport(&mut self, panel_id: &str, viewport: &GridViewport) {
        self.viewports.push((panel_id.to_owned(), viewport.clone()));
    }
    fn add_panel(&mut self, panel: &CreatedPanel) {
        self.panels.push(panel.clone());
    }
    fn seed_panel(
        &mut self,
        panel_id: &str,
        context: &TableChangeContext,
        filters: &[crate::FilterCondition],
    ) {
        self.seeds
            .push((panel_id.to_owned(), context.clone(), filters.to_vec()));
    }
    fn push_nav(&mut self, entry: &NavEntry) -> String {
        self.nav_entries.push(entry.clone());
        format!("nav-{}", self.nav_entries.len())
    }
    fn update_panel(&mut self, panel_id: &str, origin: &FkOrigin) {
        self.origins.push((panel_id.to_owned(), origin.clone()));
    }
}

fn input() -> OpenReferencingRowsInput {
    OpenReferencingRowsInput {
        source_panel_id: "p0".to_owned(),
        source_table: "customers".to_owned(),
        db_session_id: "s1".to_owned(),
        database: "shop".to_owned(),
        schema: Some("public".to_owned()),
        connection_id: Some("conn-input".to_owned()),
        driver_type: Some("postgres".to_owned()),
        row: std::collections::BTreeMap::from([("id".to_owned(), json!(42))]),
        source_viewport: None,
    }
}

fn reference(table: &str, columns: &[&str], referenced: &[&str]) -> InboundForeignKeyRef {
    InboundForeignKeyRef {
        table: table.to_owned(),
        columns: columns.iter().map(|value| (*value).to_owned()).collect(),
        referenced_columns: referenced.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn catalog(references: Vec<InboundForeignKeyRef>) -> InboundForeignKeyCatalog {
    InboundForeignKeyCatalog { references }
}

fn fetch_ok(
    value: InboundForeignKeyCatalog,
) -> impl FnMut(&CatalogScope) -> Result<InboundForeignKeyCatalog, String> {
    move |_| Ok(value.clone())
}

fn no_classify(_: &str) -> Option<String> {
    None
}

#[test]
fn constants_and_codes_are_pinned() {
    assert_eq!(MAX_REFERENCING_TABS, 4);
    assert_eq!(
        OutcomeErrorCode::NoReferences.as_str(),
        "grid.fk.noReferences"
    );
    assert_eq!(OutcomeErrorCode::Unknown.as_str(), "unknown");
    assert_eq!(
        OutcomeErrorCode::Classified("grid.fk.bad".to_owned()).as_str(),
        "grid.fk.bad"
    );
    assert_eq!(
        PROBE_FAILED_MESSAGE,
        "grid.fk.probeFailed: the referencing-table catalog did not settle"
    );
    assert_eq!(
        NO_REFERENCES_EMPTY,
        "grid.fk.noReferences: no table references '"
    );
}

#[test]
fn catalog_key_is_a_json_array_of_scope_parts() {
    let scope = CatalogScope {
        db_session_id: "s1".to_owned(),
        database: "db".to_owned(),
        schema: Some("pub".to_owned()),
        table: "t".to_owned(),
    };
    assert_eq!(catalog_key(&scope), r#"["s1","db","pub","t"]"#);
    let no_schema = CatalogScope {
        schema: None,
        ..scope.clone()
    };
    assert_eq!(catalog_key(&no_schema), r#"["s1","db",null,"t"]"#);
}

#[test]
fn catalog_cache_serves_ready_and_invalidates_by_scope() {
    let mut cache = InboundCatalogCache::default();
    let a = CatalogScope {
        db_session_id: "s1".to_owned(),
        database: "db".to_owned(),
        schema: None,
        table: "t1".to_owned(),
    };
    let b = CatalogScope {
        table: "t2".to_owned(),
        ..a.clone()
    };
    cache.mark_ready(&a, catalog(vec![reference("orders", &["id"], &["id"])]));
    cache.mark_loading(&b);
    assert!(cache.get_ready(&a).is_some());
    assert!(cache.is_loading(&b));
    assert_eq!(cache.len(), 2);
    cache.invalidate("s1", Some("t1"));
    assert_eq!(cache.len(), 1);
    assert!(cache.get_ready(&a).is_none());
    cache.invalidate("s1", None);
    assert!(cache.is_empty());
    cache.mark_ready(&a, catalog(Vec::new()));
    cache.clear();
    assert!(cache.is_empty());
}

#[test]
fn catalog_read_uses_cache_and_propagates_fetch_failures() {
    use std::cell::Cell;
    use std::rc::Rc;

    let mut cache = InboundCatalogCache::default();
    let calls = Rc::new(Cell::new(0));
    let fetch_calls = calls.clone();
    let mut fetcher = move |_: &CatalogScope| {
        fetch_calls.set(fetch_calls.get() + 1);
        Ok(catalog(vec![reference("r", &["id"], &["id"])]))
    };
    let scope = CatalogScope {
        db_session_id: "s".to_owned(),
        database: "d".to_owned(),
        schema: None,
        table: "t".to_owned(),
    };
    let first = inbound_catalog(&scope, &mut cache, &mut fetcher).unwrap();
    let second = inbound_catalog(&scope, &mut cache, &mut fetcher).unwrap();
    assert_eq!(first, second);
    assert_eq!(calls.get(), 1);

    let other = CatalogScope {
        table: "other".to_owned(),
        ..scope.clone()
    };
    cache.mark_loading(&other);
    let mut unused = |_: &CatalogScope| -> Result<InboundForeignKeyCatalog, String> {
        panic!("fetch must not run")
    };
    assert_eq!(
        inbound_catalog(&other, &mut cache, &mut unused).unwrap_err(),
        PROBE_FAILED_MESSAGE
    );

    let failing = CatalogScope {
        table: "bad".to_owned(),
        ..scope.clone()
    };
    let mut fail = |_: &CatalogScope| -> Result<InboundForeignKeyCatalog, String> {
        Err("read failed".to_owned())
    };
    assert_eq!(
        inbound_catalog(&failing, &mut cache, &mut fail).unwrap_err(),
        "read failed"
    );
    assert!(cache.get_ready(&failing).is_none());
}

#[test]
fn referenced_values_require_every_column_to_be_present() {
    let ref_ = reference("orders", &["customer_id", "region"], &["id", "region"]);
    let row = std::collections::BTreeMap::from([
        ("id".to_owned(), json!(42)),
        ("region".to_owned(), json!("EU")),
    ]);
    assert_eq!(
        referenced_values_of_row(&ref_, &row),
        Some(vec![json!(42), json!("EU")])
    );
    let null_row = std::collections::BTreeMap::from([
        ("id".to_owned(), Value::Null),
        ("region".to_owned(), json!("EU")),
    ]);
    assert_eq!(referenced_values_of_row(&ref_, &null_row), None);
    let missing = std::collections::BTreeMap::from([("id".to_owned(), json!(42))]);
    assert_eq!(referenced_values_of_row(&ref_, &missing), None);
    let empty = reference("x", &[], &[]);
    assert_eq!(referenced_values_of_row(&empty, &row), Some(Vec::new()));
}

#[test]
fn inbound_filters_pair_columns_with_values_positionally() {
    let ref_ = reference("orders", &["customer_id", "region"], &["id", "region"]);
    let values = vec![json!(42), json!("EU")];
    let filters = build_inbound_filters(&ref_, &values);
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[0].column, "customer_id");
    assert_eq!(filters[0].operator, FilterOperator::Eq);
    assert_eq!(filters[0].value, Some(json!(42)));
    assert_eq!(filters[1].column, "region");
    assert_eq!(filters[1].value, Some(json!("EU")));
    assert!(build_inbound_filters(&ref_, &[json!(42)]).is_empty());
    assert!(build_inbound_filters(&ref_, &[json!(42), Value::Null]).is_empty());
}

fn run(
    catalog_value: InboundForeignKeyCatalog,
    host: &mut MockHost,
    source: Option<SourcePanelInfo>,
) -> ReferencedByOutcome {
    host.source = source;
    let mut cache = InboundCatalogCache::default();
    let mut fetcher = fetch_ok(catalog_value);
    open_referencing_rows(&input(), host, &mut cache, &mut fetcher, &no_classify)
}

#[test]
fn fetch_failure_is_classified_and_returns_no_panels() {
    let mut cache = InboundCatalogCache::default();
    let mut fetcher = |_: &CatalogScope| -> Result<InboundForeignKeyCatalog, String> {
        Err("db down".to_owned())
    };
    let classifier = |raw: &str| {
        if raw == "db down" {
            Some("grid.fk.bad".to_owned())
        } else {
            None
        }
    };
    let mut host = MockHost::default();
    let outcome = open_referencing_rows(&input(), &mut host, &mut cache, &mut fetcher, &classifier);
    assert!(!outcome.ok);
    assert!(outcome.panel_ids.is_empty());
    assert_eq!(outcome.error.as_ref().unwrap().code.as_str(), "grid.fk.bad");
    assert_eq!(outcome.error.as_ref().unwrap().raw, "db down");

    let mut cache2 = InboundCatalogCache::default();
    let mut fetcher2 =
        |_: &CatalogScope| -> Result<InboundForeignKeyCatalog, String> { Err("other".to_owned()) };
    let outcome2 = open_referencing_rows(
        &input(),
        &mut host,
        &mut cache2,
        &mut fetcher2,
        &no_classify,
    );
    assert_eq!(outcome2.error.as_ref().unwrap().code.as_str(), "unknown");
}

#[test]
fn empty_catalog_and_null_key_are_distinct_refusals() {
    let mut host = MockHost::default();
    let empty = run(catalog(Vec::new()), &mut host, None);
    assert!(!empty.ok);
    assert_eq!(
        empty.error.as_ref().unwrap().code.as_str(),
        "grid.fk.noReferences"
    );
    assert_eq!(
        empty.error.as_ref().unwrap().raw,
        "grid.fk.noReferences: no table references 'customers'"
    );

    let null_ref = reference("orders", &["customer_id"], &["missing_column"]);
    let null_outcome = run(catalog(vec![null_ref]), &mut host, None);
    assert_eq!(
        null_outcome.error.as_ref().unwrap().raw,
        "grid.fk.noReferences: the key of this row is NULL, so no row can reference it"
    );
    assert!(host.panels.is_empty());
    assert!(host.seeds.is_empty());
    assert!(host.nav_entries.is_empty());
}

#[test]
fn successful_jump_builds_panel_seed_nav_and_origin_in_order() {
    let mut host = MockHost::default();
    let refs = vec![
        reference("orders", &["customer_id"], &["id"]),
        reference("invoices", &["owner"], &["id"]),
    ];
    let outcome = run(catalog(refs), &mut host, None);
    assert!(outcome.ok);
    assert_eq!(outcome.panel_ids, ["tbl-1", "tbl-2"]);
    assert_eq!(outcome.skipped, 0);
    assert_eq!(host.panels.len(), 2);
    assert_eq!(host.panels[0].table_name, "orders");
    assert_eq!(host.panels[0].sub_tab, "data");
    assert_eq!(host.panels[0].connection_id, "conn-input");
    assert_eq!(host.panels[0].db_session_id, "s1");
    assert_eq!(host.panels[0].database_type, "postgres");
    assert_eq!(host.panels[0].connection_name, "");
    assert_eq!(host.seeds[0].2.len(), 1);
    assert_eq!(host.seeds[0].2[0].column, "customer_id");
    assert_eq!(host.nav_entries[0].origin.target_table, "orders");
    assert_eq!(host.origins[0].1.nav_entry_id, "nav-1");
    assert_eq!(host.origins[0].1.source_table, "customers");
    assert!(host.nav_entries[0].nav_key.contains("orders"));
}

#[test]
fn source_panel_overrides_input_and_viewport_is_recorded_first() {
    let mut host = MockHost::default();
    let source = SourcePanelInfo {
        connection_id: Some("conn-panel".to_owned()),
        db_session_id: Some("s-panel".to_owned()),
        connection_name: Some("Main".to_owned()),
        database_type: Some("mysql".to_owned()),
    };
    host.source = Some(source);
    let mut source_input = input();
    source_input.source_viewport = Some(GridViewport {
        scroll_top: 120.0,
        anchor_row: Some(3.0),
        anchor_column: Some("id".to_owned()),
    });
    let mut cache = InboundCatalogCache::default();
    let mut fetcher = fetch_ok(catalog(vec![reference(
        "orders",
        &["customer_id"],
        &["id"],
    )]));
    let outcome = open_referencing_rows(
        &source_input,
        &mut host,
        &mut cache,
        &mut fetcher,
        &no_classify,
    );
    assert!(outcome.ok);
    assert_eq!(host.panels[0].connection_id, "conn-panel");
    assert_eq!(host.panels[0].db_session_id, "s-panel");
    assert_eq!(host.panels[0].connection_name, "Main");
    assert_eq!(host.panels[0].database_type, "mysql");
    assert_eq!(host.viewports.len(), 1);
    assert_eq!(host.viewports[0].0, "p0");
    assert_eq!(host.viewports[0].1.scroll_top, 120.0);
}

#[test]
fn referencing_tabs_are_capped_and_skipped_counted_from_viable_refs() {
    let mut host = MockHost::default();
    let refs: Vec<_> = (0..6)
        .map(|index| reference(&format!("t{index}"), &["customer_id"], &["id"]))
        .collect();
    let null_ref = reference("null_table", &["customer_id"], &["absent"]);
    let mut all = refs;
    all.push(null_ref.clone());
    let outcome = run(catalog(all), &mut host, None);
    assert!(outcome.ok);
    assert_eq!(outcome.panel_ids.len(), MAX_REFERENCING_TABS);
    assert_eq!(outcome.skipped, 2);
    assert_eq!(host.panels.len(), 4);
    assert_eq!(host.panels[3].table_name, "t3");
}
