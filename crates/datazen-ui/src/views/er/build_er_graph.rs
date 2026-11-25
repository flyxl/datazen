//! Port of `packages/frontend/src/windows/connection/er/buildErGraph.ts`.
//!
//! The ER canvas's graph builder: database schemas in, laid-out nodes and
//! routed edges out. Everything downstream (the layout, the hover
//! transforms, the handle routing) works on the graph this module hands
//! back, so it owns the three decisions that shape the whole diagram:
//!
//! - **What is drawn** — every table, or (in focus mode) one table plus its
//!   relations by constraint *or* by inference. A focus view that ignored
//!   inferences would hide exactly the relationships the prediction feature
//!   was added to surface.
//! - **What a line means** — a declared constraint is solid, animated and
//!   accent-blue; an inference is dashed and amber so a guess is never
//!   presented with the certainty of something the database enforces; an
//!   ambiguous guess is fainter still and carries no label.
//! - **How big a node is** — the fixed width and the measured height from
//!   `nodeMetrics.ts` (T568), so the layout packs against the size the node
//!   really renders at. Collapse is read from the caller's set and nothing
//!   else: folding a "too wide to show" rule in here would make the chevron
//!   one-way, because the rule would re-collapse a wide table the user had
//!   just expanded on every relayout.
//!
//! Positions come from the layered layout (T566), never from the node's
//! index, and handles from `er_handles.rs` (this file's split). The view's
//! own `node_view_model` (T372) predates the builder and still renders its
//! approximation; the canvas host that wires them together is a later task,
//! so every item here stays `#[allow(dead_code)]` at its use site.

use std::collections::{HashMap, HashSet};

use crate::relation_metadata::TableSchema;
use crate::views::er::er_handles::{with_handle_columns, with_handles};
use crate::views::er::graph::{
    ErEdge, ErEdgeData, ErEdgeStyle, ErGraph, ErLabelBg, ErLabelStyle, ErMarkerEnd, ErNode,
    ErRelationKind,
};
use crate::views::er::layout_er_graph::layout_er_graph_default;
use crate::views::er::node_metrics::{er_node_height, ER_AUTO_COLLAPSE_COLUMNS, ER_NODE_WIDTH};
use crate::views::er::relation_style::{
    ER_DECLARED_COLOR, ER_PREDICTED_COLOR, ER_PREDICTED_DASH,
};
use crate::views::er::table_node::{TableColumnVm, TableNodeVm};

/// How quiet an ambiguous inference sits: still visible, never as loud as a
/// constraint.
const AMBIGUOUS_OPACITY: f32 = 0.3;
/// The dash an ambiguous guess draws — tighter than an ordinary inference's.
const AMBIGUOUS_DASH: &str = "2 5";
/// An arrowhead is painted by a marker, which the line's own opacity does
/// not reach: leaving it fully saturated would put a bright chevron on a
/// faint line. React interpolates the percentage as `0.3 * 100`, which JS
/// renders as `30.000000000000004%`; CSS would read the same colour, and
/// the honest 30% is what the string says here.
const AMBIGUOUS_MARKER_COLOR: &str =
    "color-mix(in oklab, var(--c-warning, #e39a27) 30%, transparent)";
/// A label that lands on top of another line has to knock that line out
/// behind it. React Flow's default chip is white, which disappears into the
/// light theme's own whiteness and glares in the dark one, so it is taken
/// from the surface token.
const LABEL_BG_FILL: &str = "var(--c-surface, #ffffff)";
/// Declared-edge label ink: muted foreground, not the accent blue the line
/// itself draws.
const DECLARED_LABEL_FILL: &str = "var(--c-fg-muted, #888)";

/// One column pair of an inferred relationship.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub struct ErColumnPair {
    pub left: String,
    pub right: String,
}

/// A relationship the engine inferred rather than read from a constraint.
///
/// Structurally a subset of the engine's `RelationCandidate`, redeclared
/// here so the graph builder does not depend on the prediction module for
/// its input shape.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub struct ErPredictedRelation {
    /// Stable id from the engine, so a dismissal could survive re-prediction.
    pub id: String,
    pub from_table: String,
    pub to_table: String,
    pub column_pairs: Vec<ErColumnPair>,
    pub score: f64,
    /// Another table matched these columns just as well. The engine will
    /// not pick one automatically, and the diagram must not present its
    /// guess as a certain one.
    pub ambiguous: bool,
}

/// The tables a diagram starts collapsed on.
///
/// The column list has no internal scroll, so a table of sixty columns is a
/// 1478px node that would space its whole rank that far apart. Seeding the
/// collapsed set — rather than forcing collapse in the graph builder —
/// keeps the chevron working in both directions: the user can expand such a
/// table, and it stays expanded.
pub fn default_collapsed_tables(schemas: &[TableSchema]) -> HashSet<String> {
    schemas
        .iter()
        .filter(|schema| schema.columns.len() > ER_AUTO_COLLAPSE_COLUMNS)
        .map(|schema| schema.table_name.clone())
        .collect()
}

/// The column a line attaches to. Connection points live on rendered rows,
/// so a name the table does not report would drop the edge entirely: an
/// absent or unknown name falls back to the first column the table does
/// report.
fn handle_column(schema: Option<&TableSchema>, wanted: Option<&str>) -> String {
    let Some(schema) = schema else { return String::new() };
    if schema.columns.is_empty() {
        return String::new();
    }
    let needle = wanted.map(|name| name.to_lowercase());
    let found = needle
        .as_ref()
        .and_then(|needle| schema.columns.iter().find(|c| c.name.to_lowercase() == *needle));
    found.unwrap_or(&schema.columns[0]).name.clone()
}

/// Collapse a relationship the engine guessed in both directions into one
/// edge.
///
/// `a.user_id → b.user_id` and `b.user_id → a.user_id` are the same unknown
/// link seen twice; drawing both doubles the lines crossing the canvas
/// without adding information. The key is the table pair plus every column
/// the guess spans, so two genuinely different guesses about the same pair
/// stay two edges. The stronger-scoring direction wins, and a tie keeps the
/// first one the engine reported.
///
/// The output keeps first-occurrence order (the React `Map` semantics), and
/// the stats readout counts the lines that are actually drawn — hence the
/// export.
pub fn dedupe_symmetric_predictions(
    relations: &[ErPredictedRelation],
) -> Vec<ErPredictedRelation> {
    let mut order: Vec<String> = Vec::new();
    let mut best: HashMap<String, ErPredictedRelation> = HashMap::new();
    for relation in relations {
        let mut tables = [&relation.from_table, &relation.to_table];
        tables.sort_unstable();
        let mut columns: Vec<String> = relation
            .column_pairs
            .iter()
            .flat_map(|pair| [&pair.left, &pair.right])
            .map(|name| name.to_lowercase())
            .collect();
        columns.sort_unstable();
        let key = format!("{}\u{0}{}\u{0}{}", tables[0], tables[1], columns.join(","));
        match best.get_mut(&key) {
            Some(existing) if relation.score <= existing.score => {}
            Some(existing) => *existing = relation.clone(),
            None => {
                order.push(key.clone());
                best.insert(key, relation.clone());
            }
        }
    }
    order
        .into_iter()
        .filter_map(|key| best.remove(&key))
        .collect()
}

/// Build the ER diagram's nodes and edges.
///
/// `schemas` is every table in the database. `focus_table`, when set and
/// found, narrows the canvas to that table and its relations (an unknown
/// focus name falls back to every table rather than an empty canvas).
/// `predicted` are inferred relationships to draw in addition to the
/// declared ones; `collapsed_tables` is the collapse state, seeded by
/// [`default_collapsed_tables`] and owned by the caller from then on.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub fn build_er_graph(
    schemas: &[TableSchema],
    focus_table: Option<&str>,
    predicted: &[ErPredictedRelation],
    collapsed_tables: &HashSet<String>,
) -> ErGraph {
    // Both kinds mark their columns as foreign keys — a predicted one is a
    // foreign key in everything but the constraint.
    let mut fk_columns: HashSet<String> = HashSet::new();
    for schema in schemas {
        for fk in &schema.foreign_keys {
            for column in &fk.columns {
                fk_columns.insert(format!("{}.{}", schema.table_name, column));
            }
        }
    }
    for relation in predicted {
        for pair in &relation.column_pairs {
            fk_columns.insert(format!("{}.{}", relation.from_table, pair.left));
        }
    }

    /// Tables related to `table`, by constraint or by inference.
    fn relations_of(schemas: &[TableSchema], predicted: &[ErPredictedRelation], table: &str) -> Vec<String> {
        let mut related: HashSet<String> = HashSet::new();
        if let Some(schema) = schemas.iter().find(|s| s.table_name == table) {
            for fk in &schema.foreign_keys {
                related.insert(fk.referenced_table.clone());
            }
        }
        for relation in predicted {
            if relation.from_table == table {
                related.insert(relation.to_table.clone());
            } else if relation.to_table == table {
                related.insert(relation.from_table.clone());
            }
        }
        related.into_iter().collect()
    }

    let mut visible: Vec<&TableSchema> = schemas.iter().collect();
    if let Some(focus) = focus_table {
        if schemas.iter().any(|s| s.table_name == focus) {
            let mut related: HashSet<String> = HashSet::new();
            related.insert(focus.to_owned());
            for name in relations_of(schemas, predicted, focus) {
                related.insert(name);
            }
            for schema in schemas {
                if relations_of(schemas, predicted, &schema.table_name)
                    .iter()
                    .any(|name| name == focus)
                {
                    related.insert(schema.table_name.clone());
                }
            }
            visible.retain(|schema| related.contains(&schema.table_name));
        }
    }

    let schema_by_name: HashMap<&str, &TableSchema> = visible
        .iter()
        .map(|schema| (schema.table_name.as_str(), *schema))
        .collect();
    // The diagram draws the same guess only once even when the engine saw it
    // from both ends, and it never shows a table the focus filter has hidden.
    let predictions: Vec<ErPredictedRelation> = dedupe_symmetric_predictions(
        &predicted
            .iter()
            .filter(|relation| {
                schema_by_name.contains_key(relation.from_table.as_str())
                    && schema_by_name.contains_key(relation.to_table.as_str())
            })
            .cloned()
            .collect::<Vec<_>>(),
    );

    // Sizes are declared up front, exactly as `TableNode` renders them, and
    // the positions come from the layout below — never from the node's index.
    let nodes: Vec<ErNode> = visible
        .iter()
        .map(|schema| {
            let collapsed = collapsed_tables.contains(&schema.table_name);
            ErNode {
                id: schema.table_name.clone(),
                node_type: "tableNode",
                position: Default::default(),
                width: ER_NODE_WIDTH,
                height: er_node_height(schema.columns.len() as i32, collapsed),
                data: TableNodeVm {
                    table_name: schema.table_name.clone(),
                    columns: schema
                        .columns
                        .iter()
                        .map(|column| TableColumnVm {
                            name: column.name.clone(),
                            type_name: column.data_type.clone(),
                            is_pk: column.is_primary_key
                                || schema.primary_keys.iter().any(|key| key == &column.name),
                            is_fk: fk_columns
                                .contains(&format!("{}.{}", schema.table_name, column.name)),
                        })
                        .collect(),
                    handle_columns: Vec::new(),
                    highlighted: Some(schema.table_name.as_str()) == focus_table,
                    dimmed: false,
                    collapsed,
                },
            }
        })
        .collect();

    let mut edges: Vec<ErEdge> = Vec::new();

    for schema in &visible {
        for fk in &schema.foreign_keys {
            if !schema_by_name.contains_key(fk.referenced_table.as_str()) {
                continue;
            }
            // A composite key still draws as one line, anchored on its first
            // column and labelled with all of them. The edge meets the first
            // column the table actually reports (never a name it does not
            // have), while the label still lists them all.
            let from_column = handle_column(Some(schema), fk.columns.first().map(String::as_str));
            let to_column = handle_column(
                schema_by_name.get(fk.referenced_table.as_str()).copied(),
                fk.referenced_columns.first().map(String::as_str),
            );
            edges.push(ErEdge {
                id: format!("{}-{}", schema.table_name, fk.name),
                source: schema.table_name.clone(),
                target: fk.referenced_table.clone(),
                edge_type: "smoothstep",
                animated: true,
                style: ErEdgeStyle {
                    stroke: Some(ER_DECLARED_COLOR.to_owned()),
                    ..ErEdgeStyle::default()
                },
                marker: ErMarkerEnd {
                    arrow_closed: true,
                    color: ER_DECLARED_COLOR.to_owned(),
                },
                label_style: ErLabelStyle {
                    fill: DECLARED_LABEL_FILL.to_owned(),
                    ..ErLabelStyle::default()
                },
                label_bg: ErLabelBg {
                    fill: LABEL_BG_FILL.to_owned(),
                    ..ErLabelBg::default()
                },
                data: ErEdgeData {
                    kind: ErRelationKind::Declared,
                    source_column: non_empty(from_column),
                    target_column: non_empty(to_column),
                    columns: fk.columns.clone(),
                    ..ErEdgeData::default()
                },
                ..ErEdge::default()
            });
        }
    }

    for relation in &predictions {
        let ambiguous = relation.ambiguous;
        let from_column = handle_column(
            schema_by_name.get(relation.from_table.as_str()).copied(),
            relation.column_pairs.first().map(|pair| pair.left.as_str()),
        );
        let to_column = handle_column(
            schema_by_name.get(relation.to_table.as_str()).copied(),
            relation.column_pairs.first().map(|pair| pair.right.as_str()),
        );
        edges.push(ErEdge {
            // The engine's id already encodes both tables and every column
            // pair, so it is stable across re-prediction and unique among
            // sibling edges.
            id: relation.id.clone(),
            source: relation.from_table.clone(),
            target: relation.to_table.clone(),
            edge_type: "smoothstep",
            // Not animated and dashed: this relationship is inferred, and
            // the diagram must not present it with the same certainty as a
            // constraint.
            animated: false,
            style: ErEdgeStyle {
                stroke: Some(ER_PREDICTED_COLOR.to_owned()),
                stroke_dasharray: Some(
                    if ambiguous { AMBIGUOUS_DASH } else { ER_PREDICTED_DASH }.to_owned(),
                ),
                opacity: Some(if ambiguous { AMBIGUOUS_OPACITY } else { 1.0 }),
                ..ErEdgeStyle::default()
            },
            marker: ErMarkerEnd {
                arrow_closed: true,
                color: if ambiguous {
                    AMBIGUOUS_MARKER_COLOR.to_owned()
                } else {
                    ER_PREDICTED_COLOR.to_owned()
                },
            },
            label_style: ErLabelStyle {
                fill: ER_PREDICTED_COLOR.to_owned(),
                ..ErLabelStyle::default()
            },
            label_bg: ErLabelBg {
                fill: LABEL_BG_FILL.to_owned(),
                ..ErLabelBg::default()
            },
            data: ErEdgeData {
                kind: ErRelationKind::Predicted,
                score: Some(relation.score),
                ambiguous,
                source_column: non_empty(from_column),
                target_column: non_empty(to_column),
                columns: relation
                    .column_pairs
                    .iter()
                    .map(|pair| pair.left.clone())
                    .collect(),
            },
            ..ErEdge::default()
        });
    }

    // A layered layout keyed on the relationships, not on the order the
    // backend happened to return the tables in.
    let laid_out = layout_er_graph_default(&nodes, &edges);

    ErGraph {
        nodes: with_handle_columns(laid_out.clone(), &edges),
        edges: with_handles(edges, &laid_out),
    }
}

/// React's `fromColumn || undefined`: an empty string is "no column".
fn non_empty(column: String) -> Option<String> {
    if column.is_empty() {
        None
    } else {
        Some(column)
    }
}
