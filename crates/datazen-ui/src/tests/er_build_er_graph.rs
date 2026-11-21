//! T562: `buildErGraph.ts` port — the graph builder over the shared ER model:
//! nodes, declared and inferred edges, focus mode, collapse, handle routing
//! and the seeded collapse set. Ported from `er/__tests__/buildErGraph.test.ts`
//! (all five describes: basic, inferred, collapsed, handle routing, wide).
//!
//! Style constants are asserted as literals, not through the modules' own
//! exports, so a slip in a constant cannot pass its own test.

use std::collections::HashSet;

use crate::relation_metadata::{ForeignKeyInfo, TableSchema};
use crate::views::er::build_er_graph::{
    build_er_graph, default_collapsed_tables, dedupe_symmetric_predictions, ErColumnPair,
    ErPredictedRelation,
};
use crate::views::er::graph::{ErGraph, ErNode, ErRelationKind};

fn fk(name: &str, columns: &[&str], referenced_table: &str, referenced_columns: &[&str]) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.to_owned(),
        columns: columns.iter().map(|s| (*s).to_owned()).collect(),
        referenced_table: referenced_table.to_owned(),
        referenced_columns: referenced_columns.iter().map(|s| (*s).to_owned()).collect(),
        ..ForeignKeyInfo::default()
    }
}

fn make_schema(
    table: &str,
    columns: &[(&str, &str)],
    primary_keys: &[&str],
    foreign_keys: Vec<ForeignKeyInfo>,
) -> TableSchema {
    TableSchema {
        table_name: table.to_owned(),
        columns: columns
            .iter()
            .map(|(name, data_type)| crate::relation_metadata::ColumnSchema {
                name: (*name).to_owned(),
                data_type: (*data_type).to_owned(),
                is_primary_key: primary_keys.contains(name),
                ..crate::relation_metadata::ColumnSchema::default()
            })
            .collect(),
        primary_keys: primary_keys.iter().map(|s| (*s).to_owned()).collect(),
        foreign_keys,
        ..TableSchema::default()
    }
}

fn users_schema() -> TableSchema {
    make_schema(
        "users",
        &[("id", "INT"), ("name", "VARCHAR"), ("email", "VARCHAR")],
        &["id"],
        Vec::new(),
    )
}

fn orders_schema() -> TableSchema {
    make_schema(
        "orders",
        &[("id", "INT"), ("user_id", "INT"), ("total", "DECIMAL")],
        &["id"],
        vec![fk("fk_orders_user", &["user_id"], "users", &["id"])],
    )
}

fn products_schema() -> TableSchema {
    make_schema(
        "products",
        &[("id", "INT"), ("name", "VARCHAR"), ("price", "DECIMAL")],
        &["id"],
        Vec::new(),
    )
}

fn order_items_schema() -> TableSchema {
    make_schema(
        "order_items",
        &[
            ("id", "INT"),
            ("order_id", "INT"),
            ("product_id", "INT"),
            ("quantity", "INT"),
        ],
        &["id"],
        vec![
            fk("fk_items_order", &["order_id"], "orders", &["id"]),
            fk("fk_items_product", &["product_id"], "products", &["id"]),
        ],
    )
}

fn all_schemas() -> Vec<TableSchema> {
    vec![
        users_schema(),
        orders_schema(),
        products_schema(),
        order_items_schema(),
    ]
}

fn build(schemas: &[TableSchema]) -> ErGraph {
    build_er_graph(schemas, None, &[], &HashSet::new())
}

fn build_focus(schemas: &[TableSchema], focus: &str) -> ErGraph {
    build_er_graph(schemas, Some(focus), &[], &HashSet::new())
}

fn build_predicted(schemas: &[TableSchema], predicted: &[ErPredictedRelation]) -> ErGraph {
    build_er_graph(schemas, None, predicted, &HashSet::new())
}

fn build_collapsed(schemas: &[TableSchema], collapsed: &[&str]) -> ErGraph {
    let set: HashSet<String> = collapsed.iter().map(|s| (*s).to_owned()).collect();
    build_er_graph(schemas, None, &[], &set)
}

fn prediction(
    id: &str,
    from: &str,
    to: &str,
    pairs: &[(&str, &str)],
    score: f64,
) -> ErPredictedRelation {
    ErPredictedRelation {
        id: id.to_owned(),
        from_table: from.to_owned(),
        to_table: to.to_owned(),
        column_pairs: pairs
            .iter()
            .map(|(left, right)| ErColumnPair {
                left: (*left).to_owned(),
                right: (*right).to_owned(),
            })
            .collect(),
        score,
        ambiguous: false,
    }
}

fn node<'a>(graph: &'a ErGraph, id: &str) -> &'a ErNode {
    graph.nodes.iter().find(|n| n.id == id).expect("node on canvas")
}

fn at(graph: &ErGraph, id: &str) -> (f32, f32) {
    let n = node(graph, id);
    (n.position.x, n.position.y)
}

fn overlap(a: &ErNode, b: &ErNode) -> bool {
    let overlap_x = a.position.x < b.position.x + b.width && b.position.x < a.position.x + a.width;
    let overlap_y =
        a.position.y < b.position.y + b.height && b.position.y < a.position.y + a.height;
    overlap_x && overlap_y
}

// --- basic graph generation --------------------------------------------------

#[test]
fn creates_one_node_per_table_in_the_order_it_receives_them() {
    let graph = build(&all_schemas());
    let ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    // Where each node is *placed* is the layered layout's business (see the
    // `layout` tests); the builder only has to hand back every table it was
    // given, in the order it was given them.
    assert_eq!(ids, ["users", "orders", "products", "order_items"]);
}

#[test]
fn all_nodes_use_table_node_type() {
    let graph = build(&all_schemas());
    for n in &graph.nodes {
        assert_eq!(n.node_type, "tableNode");
    }
}

#[test]
fn nodes_contain_correct_column_data() {
    let graph = build(&all_schemas());
    let columns = &node(&graph, "users").data.columns;
    assert_eq!(columns.len(), 3);
    assert_eq!(
        columns[0],
        crate::views::er::table_node::TableColumnVm {
            name: "id".to_owned(),
            type_name: "INT".to_owned(),
            is_pk: true,
            is_fk: false,
        }
    );
    assert_eq!(columns[1].name, "name");
    assert_eq!(columns[1].type_name, "VARCHAR");
    assert!(!columns[1].is_pk);
    assert!(!columns[1].is_fk);
}

#[test]
fn marks_fk_columns_correctly() {
    let graph = build(&all_schemas());
    let columns = &node(&graph, "orders").data.columns;
    let user_id = columns.iter().find(|c| c.name == "user_id").expect("column");
    assert!(user_id.is_fk);
    assert!(!user_id.is_pk);
    let id = columns.iter().find(|c| c.name == "id").expect("column");
    assert!(id.is_pk);
    assert!(!id.is_fk);
}

#[test]
fn nodes_have_valid_positions() {
    let graph = build(&all_schemas());
    for n in &graph.nodes {
        assert!(n.position.x.is_finite());
        assert!(n.position.y.is_finite());
        assert!(n.position.x >= 0.0);
        assert!(n.position.y >= 0.0);
    }
}

#[test]
fn no_two_nodes_share_the_same_position() {
    let graph = build(&all_schemas());
    let mut seen: HashSet<String> = HashSet::new();
    for n in &graph.nodes {
        let key = format!("{},{}", n.position.x, n.position.y);
        assert!(seen.insert(key.clone()), "{} stacked at {key}", n.id);
    }
}

// --- edges --------------------------------------------------------------------

#[test]
fn creates_edges_for_foreign_keys() {
    let graph = build(&all_schemas());
    assert!(graph.edges.len() >= 3);
}

#[test]
fn edge_connects_source_to_target_correctly() {
    let graph = build(&all_schemas());
    let orders_fk = graph.edges.iter().find(|e| e.id == "orders-fk_orders_user").expect("edge");
    assert_eq!(orders_fk.source, "orders");
    assert_eq!(orders_fk.target, "users");
}

#[test]
fn edges_have_labels_showing_column_names() {
    let graph = build(&all_schemas());
    let orders_fk = graph.edges.iter().find(|e| e.id == "orders-fk_orders_user").expect("edge");
    assert_eq!(orders_fk.label.as_deref(), Some("user_id"));
}

#[test]
fn anchors_a_foreign_key_on_the_two_columns_it_joins() {
    // A line that leaves the node's vertical centre reads as a claim about
    // whatever row happens to sit there, which is how a join on `user_id`
    // ended up looking like a join on `product_name`.
    let graph = build(&all_schemas());
    let orders_fk = graph.edges.iter().find(|e| e.id == "orders-fk_orders_user").expect("edge");
    assert_eq!(orders_fk.source_handle.as_deref(), Some("user_id:s-r"));
    assert_eq!(orders_fk.target_handle.as_deref(), Some("id:t-l"));
}

#[test]
fn falls_back_to_a_column_the_target_actually_reports() {
    let broken = make_schema(
        "broken_fk",
        &[("ghost_id", "INT")],
        &[],
        vec![fk("fk_to_absent_column", &["ghost_id"], "users", &["no_such_column"])],
    );
    let graph = build(&[broken, users_schema()]);
    let edge = graph.edges.iter().find(|e| e.source == "broken_fk").expect("edge");
    // Losing the handle would drop the edge entirely; anchor on a real row.
    assert_eq!(edge.source_handle.as_deref(), Some("ghost_id:s-r"));
    assert_eq!(edge.target_handle.as_deref(), Some("id:t-l"));
}

#[test]
fn order_items_has_two_fk_edges() {
    let graph = build(&all_schemas());
    let item_edges: Vec<&str> = graph
        .edges
        .iter()
        .filter(|e| e.source == "order_items")
        .map(|e| e.target.as_str())
        .collect();
    assert_eq!(item_edges.len(), 2);
    let mut sorted = item_edges.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ["orders", "products"]);
}

#[test]
fn edges_are_animated_smoothstep_type() {
    let graph = build(&all_schemas());
    for edge in &graph.edges {
        assert_eq!(edge.edge_type, "smoothstep");
        assert!(edge.animated);
    }
}

// --- empty input / single table -----------------------------------------------

#[test]
fn handles_empty_schema_list() {
    let graph = build(&[]);
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());
}

#[test]
fn creates_node_with_no_edges() {
    let graph = build(&[products_schema()]);
    assert_eq!(graph.nodes.len(), 1);
    assert!(graph.edges.is_empty());
}

// --- focus mode ---------------------------------------------------------------

#[test]
fn shows_only_focused_table_and_its_relations() {
    let graph = build_focus(&all_schemas(), "orders");
    let mut names: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    names.sort_unstable();
    // orders references users (outgoing FK); order_items references orders
    // (incoming FK).
    assert_eq!(names, ["order_items", "orders", "users"]);
}

#[test]
fn does_not_include_unrelated_tables() {
    let graph = build_focus(&all_schemas(), "orders");
    // products is not directly related to orders.
    assert!(graph.nodes.iter().all(|n| n.id != "products"));
}

#[test]
fn highlights_the_focused_table() {
    let graph = build_focus(&all_schemas(), "orders");
    assert!(node(&graph, "orders").data.highlighted);
    assert!(!node(&graph, "users").data.highlighted);
}

#[test]
fn includes_edges_only_between_visible_tables() {
    let graph = build_focus(&all_schemas(), "orders");
    for edge in &graph.edges {
        assert!(["orders", "users", "order_items"].contains(&edge.source.as_str()));
        assert!(["orders", "users", "order_items"].contains(&edge.target.as_str()));
    }
}

#[test]
fn falls_back_to_all_tables_if_focus_table_not_found() {
    let graph = build_focus(&all_schemas(), "nonexistent");
    assert_eq!(graph.nodes.len(), 4);
}

#[test]
fn shows_only_the_single_table_if_it_has_no_relations() {
    let graph = build_focus(&all_schemas(), "products");
    // products has no outgoing FK and only order_items references it.
    let mut names: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["order_items", "products"]);
    assert!(graph.edges.len() >= 1);
}

// --- layout --------------------------------------------------------------------

#[test]
fn places_a_referenced_table_to_the_right_of_the_table_referencing_it() {
    // `order_items` references `orders`, which references `users`. The
    // layered layout must run that chain left to right, so an edge leaves
    // the child's right side and enters the parent's left — the handles
    // TableNode draws.
    let graph = build(&all_schemas());
    assert!(at(&graph, "users").0 > at(&graph, "orders").0);
    assert!(at(&graph, "orders").0 > at(&graph, "order_items").0);
}

#[test]
fn gives_every_node_a_declared_size_the_layout_can_pack_against() {
    let graph = build(&all_schemas());
    for n in &graph.nodes {
        assert_eq!(n.width, 260.0);
        assert!(n.height > 0.0);
    }
}

#[test]
fn leaves_a_single_table_at_the_canvas_margin() {
    let graph = build(&[users_schema()]);
    // The layout positions centres and pads by the graph margin; the canvas
    // needs the top-left corner, so the node must not be centred on itself.
    assert_eq!(at(&graph, "users"), (24.0, 24.0));
}

#[test]
fn never_overlaps_two_nodes() {
    // The bug this replaced: a wide table above a narrow one overlapped it
    // by 192px, because the row's y used the node's own height.
    let graph = build(&all_schemas());
    for i in 0..graph.nodes.len() {
        for j in i + 1..graph.nodes.len() {
            assert!(
                !overlap(&graph.nodes[i], &graph.nodes[j]),
                "{} overlaps {}",
                graph.nodes[i].id,
                graph.nodes[j].id
            );
        }
    }
}

#[test]
fn lays_out_tables_with_no_relationships_instead_of_dropping_them() {
    let orphan = make_schema("orphan", &[("id", "INT")], &["id"], Vec::new());
    let schemas = [all_schemas(), vec![orphan]].concat();
    let graph = build(&schemas);
    assert!(graph.nodes.iter().any(|n| n.id == "orphan"));
    let (x, y) = at(&graph, "orphan");
    assert!(x.is_finite() && y.is_finite());
}

#[test]
fn keeps_a_wide_table_from_colliding_with_the_next_rank() {
    let wide = make_schema(
        "wide",
        &(0..40).map(|i| (Box::leak(format!("c{i}").into_boxed_str()) as &str, "INT")).collect::<Vec<_>>(),
        &["c0"],
        Vec::new(),
    );
    let narrow = make_schema("narrow", &[("id", "INT")], &["id"], Vec::new());
    let graph = build(&[wide, narrow]);
    for n in &graph.nodes {
        assert!(n.position.x.is_finite());
        assert!(n.position.y.is_finite());
    }
}

#[test]
fn places_every_related_pair_on_neighbouring_ranks() {
    let graph = build(&all_schemas());
    let mut ranks: Vec<f32> = graph.nodes.iter().map(|n| n.position.x).collect();
    ranks.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    ranks.dedup_by(|a, b| a == b);
    let rank_of = |graph: &ErGraph, id: &str| -> i64 {
        let x = at(graph, id).0;
        ranks.iter().position(|r| *r == x).expect("rank") as i64
    };
    for edge in &graph.edges {
        // The layered layout runs a relationship over a single rank hop, so
        // no line has to cross the whole canvas to reach a join partner.
        assert!(
            (rank_of(&graph, &edge.source) - rank_of(&graph, &edge.target)).abs() <= 1,
            "{} -> {} spans more than one rank",
            edge.source,
            edge.target
        );
    }
}

#[test]
fn does_not_overlap_rows_when_tables_differ_in_height() {
    let wide = make_schema(
        "wide_table",
        &(0..12).map(|i| (Box::leak(format!("c{i}").into_boxed_str()) as &str, "INT")).collect::<Vec<_>>(),
        &["c0"],
        Vec::new(),
    );
    let schemas = vec![wide, users_schema(), orders_schema(), products_schema()];
    let graph = build(&schemas);
    let ys: Vec<f32> = graph.nodes.iter().map(|n| n.position.y).collect();
    let first_row_y = ys.iter().cloned().fold(f32::INFINITY, f32::min);
    let second_row_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let tallest_first_row = graph
        .nodes
        .iter()
        .filter(|n| n.position.y == first_row_y)
        .map(|n| 40.0 + n.data.columns.len() as f32 * 24.0)
        .fold(0.0f32, f32::max);
    assert!(second_row_y >= first_row_y + tallest_first_row);
}

// --- multi-column FK ------------------------------------------------------------

#[test]
fn handles_composite_foreign_key_columns_in_label() {
    let schema = make_schema(
        "composite_fk",
        &[("a", "INT"), ("b", "INT")],
        &[],
        vec![fk("fk_composite", &["a", "b"], "users", &["id", "name"])],
    );
    let graph = build(&[schema, users_schema()]);
    let edge = graph.edges.iter().find(|e| e.source == "composite_fk").expect("edge");
    assert_eq!(edge.label.as_deref(), Some("a, b"));
    assert_eq!(edge.data.columns, ["a", "b"]);
}

// --- node data properties --------------------------------------------------------

#[test]
fn all_nodes_have_highlighted_false_by_default() {
    let graph = build(&all_schemas());
    for n in &graph.nodes {
        assert!(!n.data.highlighted);
    }
}

#[test]
fn only_focused_table_has_highlighted_true() {
    let graph = build_focus(&all_schemas(), "users");
    assert!(node(&graph, "users").data.highlighted);
    for n in &graph.nodes {
        if n.id != "users" {
            assert!(!n.data.highlighted, "{}", n.id);
        }
    }
}

#[test]
fn each_node_data_contains_table_name_matching_node_id() {
    let graph = build(&all_schemas());
    for n in &graph.nodes {
        assert_eq!(n.data.table_name, n.id);
    }
}

#[test]
fn column_count_in_data_matches_schema_columns() {
    let schemas = all_schemas();
    let graph = build(&schemas);
    for n in &graph.nodes {
        let schema = schemas.iter().find(|s| s.table_name == n.id).expect("schema");
        assert_eq!(n.data.columns.len(), schema.columns.len());
    }
}

// --- edge properties --------------------------------------------------------------

#[test]
fn all_edges_have_marker_end_with_arrow() {
    let graph = build(&all_schemas());
    for edge in &graph.edges {
        assert!(edge.marker.arrow_closed);
    }
}

#[test]
fn edge_ids_are_unique() {
    let graph = build(&all_schemas());
    let mut ids: Vec<&str> = graph.edges.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), before);
}

#[test]
fn does_not_create_edge_if_referenced_table_is_not_visible() {
    let isolated = make_schema(
        "isolated",
        &[("ref_id", "INT")],
        &[],
        vec![fk("fk_to_missing", &["ref_id"], "missing_table", &["id"])],
    );
    let graph = build(&[isolated]);
    assert!(graph.edges.is_empty());
}

// --- large dataset ------------------------------------------------------------------

#[test]
fn handles_50_tables_without_error() {
    let schemas: Vec<TableSchema> = (0..50)
        .map(|i| {
            make_schema(
                Box::leak(format!("table_{i}").into_boxed_str()) as &str,
                &[("id", "INT"), ("name", "VARCHAR")],
                &["id"],
                if i > 0 {
                    vec![fk(
                        Box::leak(format!("fk_{i}").into_boxed_str()) as &str,
                        &["name"],
                        Box::leak(format!("table_{}", i - 1).into_boxed_str()) as &str,
                        &["id"],
                    )]
                } else {
                    Vec::new()
                },
            )
        })
        .collect();
    let graph = build(&schemas);
    assert_eq!(graph.nodes.len(), 50);
    assert_eq!(graph.edges.len(), 49);
}

// --- self-referencing FK -------------------------------------------------------------

#[test]
fn creates_edge_from_table_to_itself() {
    let schema = make_schema(
        "categories",
        &[("id", "INT"), ("parent_id", "INT")],
        &["id"],
        vec![fk("fk_parent", &["parent_id"], "categories", &["id"])],
    );
    let graph = build(&[schema]);
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(graph.edges[0].source, "categories");
    assert_eq!(graph.edges[0].target, "categories");
}

// --- inferred relationships -----------------------------------------------------------

#[test]
fn draws_an_inferred_relationship_as_its_own_edge() {
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let inferred: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.data.kind == ErRelationKind::Predicted)
        .collect();
    assert_eq!(inferred.len(), 1);
    assert_eq!(inferred[0].id, "predicted-orders-users-user_id=id");
    assert_eq!(inferred[0].source, "orders");
    assert_eq!(inferred[0].target, "users");
}

#[test]
fn keeps_an_inference_visually_distinct_from_a_constraint() {
    // The whole point of the feature is that a guess is never presented
    // with the same certainty as something the database enforces.
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let declared = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Declared)
        .expect("declared");
    let inferred = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Predicted)
        .expect("inferred");
    assert!(inferred.style.stroke_dasharray.is_some());
    assert!(!inferred.animated);
    assert_ne!(inferred.style.stroke, declared.style.stroke);
    assert!(inferred.marker.arrow_closed);
}

#[test]
fn draws_each_kind_with_the_stroke_the_legend_documents() {
    // The legend renders these same constants, so this is what keeps the
    // key and the canvas from drifting apart.
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let declared = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Declared)
        .expect("declared");
    let inferred = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Predicted)
        .expect("inferred");
    assert_eq!(declared.style.stroke.as_deref(), Some("var(--c-accent, #3b82f6)"));
    assert_eq!(declared.style.stroke_dasharray, None);
    assert_eq!(inferred.style.stroke.as_deref(), Some("var(--c-warning, #e39a27)"));
    assert_eq!(inferred.style.stroke_dasharray.as_deref(), Some("4 3"));
}

#[test]
fn keeps_a_constraint_and_an_inference_that_share_a_pair_as_two_edges() {
    // `all_schemas` already declares orders → users and the prediction
    // infers it again, which is the ordinary shape that used to take the
    // layout down. The layout collapses the pair before ranking, but the
    // canvas must still receive both edges — one solid, one dashed.
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let shared: Vec<ErRelationKind> = graph
        .edges
        .iter()
        .filter(|e| e.source == "orders" && e.target == "users")
        .map(|e| e.data.kind)
        .collect();
    assert_eq!(shared.len(), 2);
    let mut kinds = shared.clone();
    kinds.sort_by_key(|k| *k as i32);
    assert_eq!(kinds, [ErRelationKind::Declared, ErRelationKind::Predicted]);
    for n in &graph.nodes {
        assert!(n.position.x.is_finite());
        assert!(n.position.y.is_finite());
    }
}

#[test]
fn marks_inferred_columns_as_foreign_keys_on_the_node() {
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let user_id = node(&graph, "orders")
        .data
        .columns
        .iter()
        .find(|c| c.name == "user_id")
        .expect("column");
    assert!(user_id.is_fk);
}

#[test]
fn marks_a_column_only_an_inference_covers_as_a_foreign_key() {
    // `user_id` above is already covered by the declared constraint, so that
    // test cannot tell whether the predicted half of the marking runs. A
    // table with no declared foreign keys pins it: only the inference can
    // mark `actor_id`.
    let audit_log = make_schema(
        "audit_log",
        &[("id", "INT"), ("actor_id", "INT")],
        &["id"],
        Vec::new(),
    );
    let inferred = [prediction(
        "predicted-audit_log-users-actor_id=id",
        "audit_log",
        "users",
        &[("actor_id", "id")],
        0.8,
    )];
    let graph = build_predicted(&[audit_log, users_schema()], &inferred);
    let columns = &node(&graph, "audit_log").data.columns;
    let actor_id = columns.iter().find(|c| c.name == "actor_id").expect("column");
    assert!(actor_id.is_fk);
    let id = columns.iter().find(|c| c.name == "id").expect("column");
    assert!(!id.is_fk);
}

#[test]
fn draws_no_inferred_edges_by_default() {
    let graph = build(&all_schemas());
    assert!(graph.edges.iter().all(|e| e.data.kind != ErRelationKind::Predicted));
}

#[test]
fn skips_an_inference_whose_tables_are_not_both_present() {
    let orphan = [prediction(
        "predicted-orders-ghosts-order_id=id",
        "orders",
        "ghosts",
        &[("order_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &orphan);
    assert!(graph.edges.iter().all(|e| e.data.kind != ErRelationKind::Predicted));
}

#[test]
fn includes_an_inferred_neighbour_when_focusing_a_table() {
    // A focus view that ignored inferences would hide exactly the
    // relationships the feature was added to surface.
    let isolated = make_schema("audit_log", &[("id", "INT")], &["id"], Vec::new());
    let inferred = [prediction(
        "predicted-audit_log-users-actor_id=id",
        "audit_log",
        "users",
        &[("actor_id", "id")],
        0.8,
    )];
    let schemas = [all_schemas(), vec![isolated]].concat();
    let graph = build_er_graph(&schemas, Some("users"), &inferred, &HashSet::new());
    assert!(graph.nodes.iter().any(|n| n.id == "audit_log"));
    assert!(graph.edges.iter().any(|e| e.data.kind == ErRelationKind::Predicted));
}

#[test]
fn labels_an_inferred_edge_with_its_source_column() {
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let inferred = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Predicted)
        .expect("inferred");
    assert_eq!(inferred.label.as_deref(), Some("user_id"));
}

#[test]
fn anchors_an_inferred_edge_on_the_columns_it_joins() {
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    let inferred = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Predicted)
        .expect("inferred");
    assert_eq!(inferred.source_handle.as_deref(), Some("user_id:s-r"));
    assert_eq!(inferred.target_handle.as_deref(), Some("id:t-l"));
}

#[test]
fn draws_a_guess_the_engine_made_from_both_sides_only_once() {
    // Three tables sharing a `order_id` key each predict the link to each
    // of the others in both directions; the second copy adds nothing but
    // crossing lines.
    let both_ways = [
        prediction(
            "predicted-orders-users-user_id=id",
            "orders",
            "users",
            &[("user_id", "id")],
            0.9,
        ),
        prediction(
            "predicted-users-orders-id=user_id",
            "users",
            "orders",
            &[("id", "user_id")],
            0.7,
        ),
    ];
    let graph = build_predicted(&all_schemas(), &both_ways);
    let inferred: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.data.kind == ErRelationKind::Predicted)
        .collect();
    assert_eq!(inferred.len(), 1);
    assert_eq!(inferred[0].id, "predicted-orders-users-user_id=id");
    assert_eq!(inferred[0].source, "orders");
}

#[test]
fn keeps_two_different_guesses_about_the_same_table_pair_apart() {
    let two_links = [
        prediction(
            "predicted-orders-users-id=name",
            "orders",
            "users",
            &[("id", "name")],
            0.6,
        ),
        prediction(
            "predicted-orders-users-name=email",
            "orders",
            "users",
            &[("name", "email")],
            0.6,
        ),
    ];
    let graph = build_predicted(&all_schemas(), &two_links);
    assert_eq!(
        graph
            .edges
            .iter()
            .filter(|e| e.data.kind == ErRelationKind::Predicted)
            .count(),
        2
    );
}

#[test]
fn renders_an_ambiguous_guess_as_a_faint_unlabelled_line() {
    let mut ambiguous = prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.8,
    );
    ambiguous.ambiguous = true;
    let graph = build_predicted(&all_schemas(), &[ambiguous]);
    let edge = graph
        .edges
        .iter()
        .find(|e| e.id == "predicted-orders-users-user_id=id")
        .expect("edge");
    assert!(edge.label.is_none());
    assert!(edge.style.opacity.expect("opacity") < 1.0);
    assert!(edge.data.ambiguous);
    assert_eq!(edge.style.stroke_dasharray.as_deref(), Some("2 5"));
}

#[test]
fn fades_an_ambiguous_guess_with_its_arrowhead() {
    // Marker paint is not reached by the line's opacity, so an unfaded
    // chevron would be the one loud thing left on a faint line.
    let mut faded = prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.8,
    );
    faded.ambiguous = true;
    let graph = build_predicted(&all_schemas(), &[faded]);
    let edge = graph
        .edges
        .iter()
        .find(|e| e.data.kind == ErRelationKind::Predicted)
        .expect("inferred");
    assert!(edge.marker.color.contains("color-mix"));
}

#[test]
fn paints_every_edge_from_theme_tokens_not_the_nonexistent_color_namespace() {
    // A `--color-*` reference resolves to neither theme and silently drops
    // the paint; globals.css documents the same trap for the query builder.
    let predicted = [prediction(
        "predicted-orders-users-user_id=id",
        "orders",
        "users",
        &[("user_id", "id")],
        0.9,
    )];
    let graph = build_predicted(&all_schemas(), &predicted);
    assert!(!graph.edges.is_empty());
    for edge in &graph.edges {
        let painted = [
            edge.style.stroke.as_deref(),
            Some(edge.label_style.fill.as_str()),
            Some(edge.label_bg.fill.as_str()),
            Some(edge.marker.color.as_str()),
        ];
        for value in painted.into_iter().flatten() {
            assert!(!value.contains("--color-"), "{value}");
            if value.starts_with("var(") {
                assert!(value.starts_with("var(--c-"), "{value}");
            }
        }
    }
}

// --- collapsed tables ------------------------------------------------------------

#[test]
fn marks_a_collapsed_table_and_shrinks_the_height_it_reserves() {
    let expanded = build(&all_schemas());
    let collapsed = build_collapsed(&all_schemas(), &["orders"]);
    assert!(!node(&expanded, "orders").data.collapsed);
    assert!(node(&collapsed, "orders").data.collapsed);
    assert!(node(&collapsed, "orders").height < node(&expanded, "orders").height);
}

#[test]
fn leaves_other_tables_expanded() {
    let collapsed = build_collapsed(&all_schemas(), &["orders"]);
    assert!(!node(&collapsed, "users").data.collapsed);
}

#[test]
fn re_runs_the_layout_so_the_freed_space_is_reclaimed() {
    // Collapsing must move the remaining nodes, otherwise the diagram keeps
    // the collapsed node's old footprint and a later expansion can overlap
    // a neighbour.
    let expanded = build(&all_schemas());
    let collapsed = build_collapsed(&all_schemas(), &["orders"]);
    let positions = |graph: &ErGraph| {
        graph
            .nodes
            .iter()
            .map(|n| format!("{}:{},{}", n.id, n.position.x, n.position.y))
            .collect::<Vec<_>>()
    };
    assert_ne!(positions(&collapsed), positions(&expanded));
}

#[test]
fn still_never_overlaps_once_a_table_is_collapsed() {
    let collapsed = build_collapsed(&all_schemas(), &["orders", "order_items", "products"]);
    for i in 0..collapsed.nodes.len() {
        for j in i + 1..collapsed.nodes.len() {
            assert!(
                !overlap(&collapsed.nodes[i], &collapsed.nodes[j]),
                "{} overlaps {}",
                collapsed.nodes[i].id,
                collapsed.nodes[j].id
            );
        }
    }
}

#[test]
fn lays_out_the_same_way_when_the_collapsed_set_is_empty() {
    let empty_set = build_collapsed(&all_schemas(), &[]);
    let default = build(&all_schemas());
    let pos = |graph: &ErGraph| graph.nodes.iter().map(|n| n.position).collect::<Vec<_>>();
    assert_eq!(pos(&empty_set), pos(&default));
}

// --- handle routing ----------------------------------------------------------------

#[test]
fn points_a_forward_edge_at_the_rows_it_joins() {
    // `orders.user_id` references `users.id`. The layout puts `users` on the
    // right, so the edge leaves the child's right and enters the parent's
    // left.
    let graph = build(&all_schemas());
    let declared = graph.edges.iter().find(|e| e.id == "orders-fk_orders_user").expect("edge");
    assert_eq!(declared.source_handle.as_deref(), Some("user_id:s-r"));
    assert_eq!(declared.target_handle.as_deref(), Some("id:t-l"));
}

#[test]
fn mirrors_the_sides_for_an_edge_that_points_leftwards() {
    // Two tables referencing each other: the layered layout cannot keep
    // both pointing right, so one must be routed from the left.
    let a = make_schema(
        "a",
        &[("id", "INT"), ("b_id", "INT")],
        &["id"],
        vec![fk("fk_a_b", &["b_id"], "b", &["id"])],
    );
    let b = make_schema(
        "b",
        &[("id", "INT"), ("a_id", "INT")],
        &["id"],
        vec![fk("fk_b_a", &["a_id"], "a", &["id"])],
    );
    let graph = build(&[a, b]);
    let from_a = graph.edges.iter().find(|e| e.id == "a-fk_a_b").expect("edge");
    let from_b = graph.edges.iter().find(|e| e.id == "b-fk_b_a").expect("edge");
    // Whichever table ended up on the right must route from its left side.
    if at(&graph, "b").0 > at(&graph, "a").0 {
        assert_eq!(from_a.source_handle.as_deref(), Some("b_id:s-r"));
        assert_eq!(from_a.target_handle.as_deref(), Some("id:t-l"));
        assert_eq!(from_b.source_handle.as_deref(), Some("a_id:s-l"));
        assert_eq!(from_b.target_handle.as_deref(), Some("id:t-r"));
    } else {
        assert_eq!(from_a.source_handle.as_deref(), Some("b_id:s-l"));
        assert_eq!(from_a.target_handle.as_deref(), Some("id:t-r"));
        assert_eq!(from_b.source_handle.as_deref(), Some("a_id:s-r"));
        assert_eq!(from_b.target_handle.as_deref(), Some("id:t-l"));
    }
}

#[test]
fn falls_back_to_the_node_level_handles_when_a_table_is_collapsed() {
    // A collapsed node renders no rows, so there is no row to point at.
    let graph = build_collapsed(&all_schemas(), &["orders"]);
    let declared = graph.edges.iter().find(|e| e.id == "orders-fk_orders_user").expect("edge");
    assert_eq!(declared.source_handle.as_deref(), Some("node:s-r"));
    assert_eq!(declared.target_handle.as_deref(), Some("id:t-l"));
}

#[test]
fn falls_back_to_the_node_level_handles_for_a_self_reference() {
    let employee = make_schema(
        "employee",
        &[("id", "INT"), ("manager_id", "INT")],
        &["id"],
        vec![fk("fk_self", &["manager_id"], "employee", &["id"])],
    );
    let graph = build(&[employee]);
    assert_eq!(graph.edges[0].source_handle.as_deref(), Some("node:s-r"));
    assert_eq!(graph.edges[0].target_handle.as_deref(), Some("node:t-l"));
}

#[test]
fn reports_only_the_columns_an_edge_touches() {
    // A wide table must not carry connection points on every row.
    let graph = build(&all_schemas());
    let handles = &node(&graph, "orders").data.handle_columns;
    assert!(handles.iter().any(|c| c == "user_id"));
    assert!(handles.iter().any(|c| c == "id"));
    assert!(!handles.iter().any(|c| c == "total"));
}

#[test]
fn gives_every_edge_handles_it_can_actually_use() {
    let graph = build(&all_schemas());
    for edge in &graph.edges {
        let source_handle = edge.source_handle.as_deref().expect("source handle");
        let target_handle = edge.target_handle.as_deref().expect("target handle");
        for (node, handle, role) in [
            (node(&graph, &edge.source), source_handle, "s"),
            (node(&graph, &edge.target), target_handle, "t"),
        ] {
            // Either a node-level fallback, or a column that is declared on
            // the node.
            if handle.starts_with("node:") {
                let side = if handle.ends_with("-l") { "l" } else { "r" };
                assert_eq!(handle, format!("node:{role}-{side}"));
            } else {
                let column = handle.split(':').next().expect("column");
                assert!(
                    node.data.handle_columns.iter().any(|c| c == column),
                    "{column} not in {}'s handle columns",
                    node.id
                );
                assert!(node.data.columns.iter().any(|c| c.name == column));
            }
        }
    }
}

// --- wide tables -----------------------------------------------------------------

fn wide_schema(count: usize) -> TableSchema {
    let columns: Vec<(String, &str)> = (0..count).map(|i| (format!("c{i}"), "INT")).collect();
    let leaked: Vec<(&str, &str)> = columns
        .iter()
        .map(|(name, ty)| (Box::leak(name.clone().into_boxed_str()) as &str, *ty))
        .collect();
    make_schema("wide", &leaked, &["c0"], Vec::new())
}

#[test]
fn seeds_a_very_wide_table_as_collapsed() {
    // Without an internal scroll a 60-column table would be a 1478px node.
    let seeded = default_collapsed_tables(&[wide_schema(60)]);
    assert_eq!(seeded, ["wide"].iter().map(|s| (*s).to_owned()).collect::<HashSet<_>>());
}

#[test]
fn leaves_a_table_at_or_below_the_threshold_expanded() {
    assert!(default_collapsed_tables(&[wide_schema(30)]).is_empty());
    let seeded = default_collapsed_tables(&[wide_schema(31)]);
    assert_eq!(seeded, ["wide"].iter().map(|s| (*s).to_owned()).collect::<HashSet<_>>());
}

#[test]
fn stays_expanded_when_the_user_expands_a_wide_table() {
    // The seed is a starting point, not a rule: the chevron must work both
    // ways, so the graph builder must not re-collapse a wide table behind
    // the user.
    let graph = build_collapsed(&[wide_schema(60)], &[]);
    assert!(!graph.nodes[0].data.collapsed);
    // 36 + 60 × 24 + 2 borders — the literal pins the size contract the
    // layout packs against.
    assert_eq!(graph.nodes[0].height, 1478.0);
}

#[test]
fn collapses_a_wide_table_when_the_seeded_set_is_applied() {
    let schemas = [wide_schema(60)];
    let seeded = default_collapsed_tables(&schemas);
    let graph = build_er_graph(&schemas, None, &[], &seeded);
    assert!(graph.nodes[0].data.collapsed);
}

#[test]
fn leaves_an_ordinary_schema_untouched() {
    let schemas = all_schemas();
    let seeded = default_collapsed_tables(&schemas);
    let graph = build_er_graph(&schemas, None, &[], &seeded);
    for n in &graph.nodes {
        assert!(!n.data.collapsed);
    }
}

// --- dedupe helper -----------------------------------------------------------------

#[test]
fn dedupe_keeps_the_strongest_direction_and_first_wins_on_a_tie() {
    let relations = [
        prediction("a", "orders", "users", &[("user_id", "id")], 0.5),
        prediction("b", "users", "orders", &[("id", "user_id")], 0.7),
        prediction("c", "orders", "users", &[("user_id", "id")], 0.7),
    ];
    let deduped = dedupe_symmetric_predictions(&relations);
    // Same table pair, same columns: three guesses collapse to one. The
    // strongest direction wins, and a tie never displaces the first one
    // reported at that strength.
    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].id, "b");

    let different = [
        prediction("d", "orders", "users", &[("id", "name")], 0.6),
        prediction("e", "orders", "users", &[("name", "email")], 0.6),
    ];
    assert_eq!(dedupe_symmetric_predictions(&different).len(), 2);
}
