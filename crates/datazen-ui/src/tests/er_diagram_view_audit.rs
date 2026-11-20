//! Independent audit for T372: `ErDiagramView` port.
//!
//! Fill branch coverage the coder does not drive: wide-table collapse seeding,
//! zoom clamps, pin updates, and the relation-count arithmetic.

use crate::relation_metadata::{ColumnSchema, ForeignKeyInfo, TableSchema};
use crate::views::er::er_diagram_view::{ErDiagramState, ErEdge, Viewport};

fn schema(name: &str, columns: usize, fks: usize) -> TableSchema {
    TableSchema {
        table_name: name.to_owned(),
        columns: (0..columns).map(|_| ColumnSchema::default()).collect(),
        foreign_keys: (0..fks).map(|_| ForeignKeyInfo::default()).collect(),
        ..Default::default()
    }
}

#[test]
fn wide_tables_seed_collapse_but_user_toggles_stick_until_the_next_load() {
    let mut state = ErDiagramState::new("s", "db", None);
    state.set_schemas(vec![
        schema("narrow", 3, 0),
        schema("wide", 31, 0),
        schema("edge", 30, 0),
    ]);
    assert!(state.collapsed.iter().any(|name| name == "wide"), "31 columns collapses");
    assert!(!state.collapsed.iter().any(|name| name == "edge"), "30 columns does not");
    state.toggle_collapse("wide");
    assert!(!state.collapsed.iter().any(|name| name == "wide"), "the user can expand it");

    // A fresh data load re-seeds from the entity set (React's defaultCollapsedTables).
    state.set_schemas(vec![schema("wide", 31, 0)]);
    assert!(state.collapsed.iter().any(|name| name == "wide"));
    assert!(!state.loading && state.error.is_none());
}

#[test]
fn zoom_clamps_to_the_reactflow_min_and_max() {
    let mut state = ErDiagramState::new("s", "db", None);
    for _ in 0..40 {
        state.zoom_in();
    }
    assert!((state.viewport.zoom - 10.0).abs() < 0.001, "maxZoom is 10");
    for _ in 0..80 {
        state.zoom_out();
    }
    assert!((state.viewport.zoom - 0.1).abs() < 0.001, "minZoom is 0.1");
    state.fit_view();
    assert_eq!(state.viewport, Viewport::default());
}

#[test]
fn pinning_an_existing_table_replaces_its_viewport() {
    let mut state = ErDiagramState::new("s", "db", None);
    state.pin("users", Viewport { x: 10, y: 20, zoom: 1.0 });
    state.pin("users", Viewport { x: 30, y: 40, zoom: 2.0 });
    assert_eq!(state.pinned.len(), 1, "a table keeps at most one pin");
    assert_eq!(state.pinned[0].1, Viewport { x: 30, y: 40, zoom: 2.0 });
    state.pin("orders", Viewport { x: 1, y: 2, zoom: 1.0 });
    assert_eq!(state.pinned.len(), 2);
}

#[test]
fn exporter_constants_match_the_react_literals() {
    // The coder's test builds its expectation from the module constants, so a
    // constant mutation stays green there; pin the literal React values here.
    assert_eq!(
        crate::views::er::er_diagram_view::EXPORT_BACKGROUND,
        "#1a1a2e",
        "React's ER export backdrop"
    );
    assert_eq!(
        crate::views::er::er_diagram_view::EXPORT_PNG_QUALITY,
        1,
        "React's PNG quality"
    );
}

#[test]
fn relation_count_sums_declared_foreign_keys_and_predicted_edges() {
    let mut state = ErDiagramState::new("s", "db", None);
    state.set_schemas(vec![schema("a", 1, 2), schema("b", 1, 1)]);
    assert_eq!(state.declared_count(), 3);
    assert_eq!(state.predicted_count(), 0);
    assert_eq!(state.relation_count(), 3);

    state.predicted = vec![ErEdge {
        id: "p1".to_owned(),
        from_table: "a".to_owned(),
        to_table: "b".to_owned(),
        predicted: true,
        ambiguous: false,
        score: 7,
    }];
    assert_eq!(state.predicted_count(), 1);
    assert_eq!(state.relation_count(), 4);
}
