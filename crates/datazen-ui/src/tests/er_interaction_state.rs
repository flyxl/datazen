//! T564: `interactionState.ts` port — pinned positions, hover neighbourhoods,
//! hover styling and edge-label fitting, as pure transforms over the shared
//! graph model. Ported from `er/__tests__/interactionState.test.ts`.

use std::collections::{HashMap, HashSet};

use crate::views::er::edge_labels::{fit_edge_label, label_fits};
use crate::views::er::graph::{ErEdge, ErEdgeStyle, ErNode, ErPosition};
use crate::views::er::interaction_state::{
    apply_hover_to_edges, apply_hover_to_nodes, apply_pinned_positions, hovered_neighbourhood,
    HOVER_DIM_OPACITY, HOVER_EMPHASIS_WIDTH, PinnedPosition,
};

fn node(id: &str, x: f32, y: f32) -> ErNode {
    ErNode {
        id: id.to_owned(),
        position: ErPosition::new(x, y),
        width: 260.0,
        height: 100.0,
        ..ErNode::default()
    }
}

fn edge(id: &str, source: &str, target: &str) -> ErEdge {
    ErEdge {
        id: id.to_owned(),
        source: source.to_owned(),
        target: target.to_owned(),
        style: ErEdgeStyle {
            stroke: Some("red".to_owned()),
            ..ErEdgeStyle::default()
        },
        ..ErEdge::default()
    }
}

fn by_id<'a>(nodes: &'a [ErNode], id: &str) -> &'a ErNode {
    nodes.iter().find(|n| n.id == id).expect("node on canvas")
}

fn by_id_edge<'a>(edges: &'a [ErEdge], id: &str) -> &'a ErEdge {
    edges.iter().find(|e| e.id == id).expect("edge in list")
}

fn pins(entries: &[(&str, f32, f32)]) -> HashMap<String, PinnedPosition> {
    entries
        .iter()
        .map(|(id, x, y)| {
            (
                (*id).to_owned(),
                PinnedPosition {
                    x: *x,
                    y: *y,
                },
            )
        })
        .collect()
}

#[test]
fn pinned_positions_leave_the_nodes_untouched_when_nothing_is_pinned() {
    let nodes = vec![node("a", 10.0, 20.0)];
    assert_eq!(apply_pinned_positions(&nodes, &HashMap::new()), nodes);
}

#[test]
fn pinned_positions_override_only_the_pinned_nodes() {
    // Dragging one table must not disturb the rest of the diagram.
    let nodes = vec![node("a", 10.0, 20.0), node("b", 30.0, 40.0)];
    let laid = apply_pinned_positions(&nodes, &pins(&[("a", 500.0, 600.0)]));
    assert_eq!(by_id(&laid, "a").position, ErPosition::new(500.0, 600.0));
    assert_eq!(by_id(&laid, "b").position, ErPosition::new(30.0, 40.0));
}

#[test]
fn pinned_positions_do_not_mutate_the_nodes_they_are_given() {
    let nodes = vec![node("a", 10.0, 20.0)];
    let _ = apply_pinned_positions(&nodes, &pins(&[("a", 1.0, 2.0)]));
    assert_eq!(nodes[0].position, ErPosition::new(10.0, 20.0));
}

#[test]
fn pinned_positions_ignore_a_pin_for_a_table_no_longer_on_the_canvas() {
    let nodes = vec![node("a", 0.0, 0.0)];
    let laid = apply_pinned_positions(&nodes, &pins(&[("gone", 1.0, 2.0)]));
    assert_eq!(laid.len(), 1);
    assert_eq!(laid[0].id, "a");
    assert_eq!(laid[0].position, ErPosition::new(0.0, 0.0));
}

#[test]
fn hover_neighbourhood_returns_none_when_nothing_is_hovered() {
    let edges = vec![edge("e1", "a", "b")];
    assert!(hovered_neighbourhood(&edges, None).is_none());
}

#[test]
fn hover_neighbourhood_collects_both_directions_of_the_relationships() {
    let edges = vec![
        edge("e1", "a", "b"),
        edge("e2", "c", "a"),
        edge("e3", "d", "e"),
    ];
    // `a` is a source in one edge and a target in another; both count.
    let expected: HashSet<String> = ["a", "b", "c"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(hovered_neighbourhood(&edges, Some("a")), Some(expected));
}

#[test]
fn hover_neighbourhood_does_not_pull_in_unrelated_tables() {
    let edges = vec![
        edge("e1", "a", "b"),
        edge("e2", "c", "a"),
        edge("e3", "d", "e"),
    ];
    let neighbourhood = hovered_neighbourhood(&edges, Some("a")).expect("hovered");
    assert!(!neighbourhood.contains("d"));
    assert!(!neighbourhood.contains("e"));
}

#[test]
fn hover_neighbourhood_returns_just_the_table_when_it_has_no_relationships() {
    let edges = vec![edge("e1", "a", "b")];
    let expected: HashSet<String> = ["lonely"].iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        hovered_neighbourhood(&edges, Some("lonely")),
        Some(expected)
    );
}

#[test]
fn hover_to_nodes_dims_everything_outside_the_neighbourhood() {
    let nodes = vec![node("a", 0.0, 0.0), node("b", 0.0, 0.0), node("c", 0.0, 0.0)];
    let neighbourhood: HashSet<String> = ["a", "b"].iter().map(|s| (*s).to_owned()).collect();
    let styled = apply_hover_to_nodes(&nodes, Some(&neighbourhood));
    assert!(!by_id(&styled, "a").data.dimmed);
    assert!(!by_id(&styled, "b").data.dimmed);
    assert!(by_id(&styled, "c").data.dimmed);
}

#[test]
fn hover_to_nodes_leaves_the_nodes_alone_when_nothing_is_hovered() {
    let nodes = vec![node("a", 0.0, 0.0)];
    assert_eq!(apply_hover_to_nodes(&nodes, None), nodes);
}

#[test]
fn hover_to_nodes_preserves_the_dimming_search_already_applied() {
    // Search dims too; hovering must not un-dim a node search had dimmed.
    let mut searched = node("c", 0.0, 0.0);
    searched.data.dimmed = true;
    let neighbourhood: HashSet<String> = ["a"].iter().map(|s| (*s).to_owned()).collect();
    let styled = apply_hover_to_nodes(&[searched], Some(&neighbourhood));
    assert!(styled[0].data.dimmed);
}

#[test]
fn hover_to_edges_emphasises_an_incident_edge() {
    let edges = vec![edge("e1", "a", "b"), edge("e2", "c", "d")];
    let styled = apply_hover_to_edges(&edges, Some("a"));
    let incident = by_id_edge(&styled, "e1");
    // The literal values are the contract the React tests pin; asserting on
    // the constants themselves would let a slip in the constant pass.
    assert_eq!(incident.style.stroke_width, Some(2.5));
    assert_eq!(incident.z_index, Some(1000));
    assert!(incident.z_index.expect("raised") > 0);
}

#[test]
fn hover_to_edges_dims_an_unrelated_edge() {
    let edges = vec![edge("e1", "a", "b"), edge("e2", "c", "d")];
    let styled = apply_hover_to_edges(&edges, Some("a"));
    let dimmed = by_id_edge(&styled, "e2");
    assert_eq!(dimmed.style.opacity, Some(0.15));
    // Dimming only fades: the stroke the edge already had survives it.
    assert_eq!(dimmed.style.stroke, Some("red".to_owned()));
    assert_eq!(dimmed.z_index, None);
}

#[test]
fn hover_to_edges_counts_an_edge_where_the_hovered_table_is_the_target() {
    let edges = vec![edge("e1", "a", "b"), edge("e2", "c", "d")];
    let styled = apply_hover_to_edges(&edges, Some("b"));
    assert_eq!(
        by_id_edge(&styled, "e1").style.stroke_width,
        Some(HOVER_EMPHASIS_WIDTH)
    );
    let _ = HOVER_EMPHASIS_WIDTH;
}

#[test]
fn hover_to_edges_keeps_the_edge_styling_it_already_had() {
    let edges = vec![edge("e1", "a", "b")];
    let styled = apply_hover_to_edges(&edges, Some("a"));
    assert_eq!(styled[0].style.stroke, Some("red".to_owned()));
}

#[test]
fn hover_to_edges_leaves_the_edges_alone_when_nothing_is_hovered() {
    let edges = vec![edge("e1", "a", "b")];
    assert_eq!(apply_hover_to_edges(&edges, None), edges);
}

#[test]
fn hover_to_edges_does_not_mutate_the_edges_it_is_given() {
    let edges = vec![edge("e1", "a", "b")];
    let _ = apply_hover_to_edges(&edges, Some("a"));
    assert_eq!(edges[0].z_index, None);
    assert_eq!(edges[0].style.opacity, None);
}

#[test]
fn edge_labels_keep_a_single_column_label_when_it_fits() {
    // A 110px gap holds about 16 characters — every plain foreign key.
    let columns = vec!["user_id".to_owned()];
    assert_eq!(fit_edge_label(&columns, 110.0), "user_id");
}

#[test]
fn edge_labels_summarise_a_composite_label_that_would_not_fit() {
    // `tenant_id, account_id` needs 138px and would overlap the nodes.
    let columns = vec!["tenant_id".to_owned(), "account_id".to_owned()];
    assert_eq!(fit_edge_label(&columns, 110.0), "tenant_id +1");
}

#[test]
fn edge_labels_keep_a_composite_label_that_does_fit() {
    let columns = vec!["a".to_owned(), "b".to_owned()];
    assert_eq!(fit_edge_label(&columns, 110.0), "a, b");
}

#[test]
fn edge_labels_truncate_when_even_the_summary_is_too_wide() {
    let columns = vec!["a_very_long_column_name".to_owned(), "another".to_owned()];
    let label = fit_edge_label(&columns, 60.0);
    assert!(label_fits(Some(&label), 60.0));
    assert!(label.ends_with('…'));
}

#[test]
fn edge_labels_always_produce_a_label_that_fits_the_gap() {
    let columns = vec![
        "tenant_identifier".to_owned(),
        "account_identifier".to_owned(),
        "region_identifier".to_owned(),
    ];
    for gap in [20.0, 40.0, 60.0, 110.0, 200.0] {
        let label = fit_edge_label(&columns, gap);
        assert!(
            label_fits(Some(&label), gap),
            "label {label:?} must fit gap {gap}"
        );
    }
}

#[test]
fn edge_labels_return_nothing_for_a_relationship_with_no_columns() {
    assert_eq!(fit_edge_label(&[], 110.0), "");
}

#[test]
fn edge_labels_absent_labels_never_fit() {
    assert!(!label_fits(None, 110.0));
    assert!(!label_fits(Some(""), 110.0));
    assert!(label_fits(Some("user_id"), 110.0));
}
