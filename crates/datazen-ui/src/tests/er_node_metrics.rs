//! T568: `nodeMetrics.ts` + `relationStyle.ts` ports — the size contract the
//! layout packs against and the stroke contract the legend documents. Ported
//! from `er/__tests__/nodeMetrics.test.ts` plus the relation-style assertions
//! the React side pins inside `buildErGraph.test.ts` and
//! `ErRelationLegend.test.tsx`.

use crate::views::er::er_relation_legend::PREDICTED_DASH;
use crate::views::er::node_metrics::{
    er_column_row_center_y, er_node_body_height, er_node_height, ER_AUTO_COLLAPSE_COLUMNS,
    ER_COLLAPSED_FOOTER_HEIGHT, ER_COLUMN_ROW_HEIGHT, ER_HEADER_HEIGHT, ER_NODE_BORDER,
    ER_NODE_WIDTH,
};
use crate::views::er::relation_style::{
    ER_DECLARED_COLOR, ER_PREDICTED_COLOR, ER_PREDICTED_DASH,
};

/// These numbers are a contract with `TableNode`, which applies each one as an
/// explicit inline height. If they drift, the layout packs nodes against a
/// size that is not what renders.
#[test]
fn node_metrics_gives_a_fixed_width_the_layout_can_pack_against() {
    assert!(ER_NODE_WIDTH > 0.0);
    assert_eq!(ER_NODE_WIDTH, 260.0);
}

#[test]
fn node_metrics_sizes_a_node_from_header_rows_and_border() {
    let columns = 5;
    assert_eq!(
        er_node_height(columns, false),
        ER_HEADER_HEIGHT + columns as f32 * ER_COLUMN_ROW_HEIGHT + ER_NODE_BORDER * 2.0
    );
}

#[test]
fn node_metrics_grows_with_every_column_with_no_scroll_cap() {
    // An internal scroll would move the connection points with `scrollTop`.
    assert_eq!(er_node_body_height(40), 40.0 * ER_COLUMN_ROW_HEIGHT);
    assert_eq!(
        er_node_height(40, false),
        ER_HEADER_HEIGHT + 40.0 * ER_COLUMN_ROW_HEIGHT + ER_NODE_BORDER * 2.0
    );
}

#[test]
fn node_metrics_matches_the_body_height() {
    assert_eq!(er_node_body_height(3), 3.0 * ER_COLUMN_ROW_HEIGHT);
    assert_eq!(er_node_body_height(0), 0.0);
}

#[test]
fn node_metrics_centres_each_column_row_inside_the_node() {
    // The first row starts after the border and the header.
    assert_eq!(
        er_column_row_center_y(0),
        ER_NODE_BORDER + ER_HEADER_HEIGHT + ER_COLUMN_ROW_HEIGHT / 2.0
    );
    // Consecutive rows are exactly one row apart.
    let delta = er_column_row_center_y(3) - er_column_row_center_y(2);
    assert_eq!(delta, ER_COLUMN_ROW_HEIGHT);
    // And the centre must land inside the node's own height.
    assert!(er_column_row_center_y(9) < er_node_height(10, false));
}

#[test]
fn node_metrics_clamps_a_negative_column_index_to_the_first_row() {
    assert_eq!(er_column_row_center_y(-1), er_column_row_center_y(0));
}

#[test]
fn node_metrics_ignores_a_negative_column_count_rather_than_rising_above_the_header() {
    assert_eq!(er_node_body_height(-4), 0.0);
    assert_eq!(er_node_height(-4, false), ER_HEADER_HEIGHT + ER_NODE_BORDER * 2.0);
}

#[test]
fn node_metrics_makes_a_collapsed_node_shorter_than_the_same_node_expanded() {
    let expanded = er_node_height(20, false);
    let collapsed = er_node_height(20, true);
    assert_eq!(
        collapsed,
        ER_HEADER_HEIGHT + ER_COLLAPSED_FOOTER_HEIGHT + ER_NODE_BORDER * 2.0
    );
    assert!(collapsed < expanded);
}

#[test]
fn node_metrics_collapses_to_the_same_height_regardless_of_column_count() {
    // The column list is not rendered when collapsed, so its length is
    // irrelevant.
    assert_eq!(er_node_height(3, true), er_node_height(60, true));
}

#[test]
fn node_metrics_threshold_is_thirty_columns() {
    // `defaultCollapsedTables` (T562) seeds collapse strictly above this.
    assert_eq!(ER_AUTO_COLLAPSE_COLUMNS, 30);
}

#[test]
fn relation_style_paints_from_theme_tokens() {
    // The stroke a constraint declares and the stroke an inference gets, as
    // the legend and the canvas both read them.
    assert_eq!(ER_DECLARED_COLOR, "var(--c-accent, #3b82f6)");
    assert_eq!(ER_PREDICTED_COLOR, "var(--c-warning, #e39a27)");
    // Dashed, not only recoloured: the distinction survives monochrome
    // exports and colour-blind readers.
    assert_eq!(ER_PREDICTED_DASH, "4 3");
    // Every paint comes from the `--c-*` token namespace with a literal
    // fallback; a `--color-*` reference resolves to neither theme.
    for value in [ER_DECLARED_COLOR, ER_PREDICTED_COLOR] {
        assert!(value.starts_with("var(--c-"));
        assert!(value.contains('#'));
    }
}

#[test]
fn legend_dash_is_derived_from_the_relation_style_constant() {
    // The legend swatch must be literally the stroke drawn on an edge: the
    // tuple is parsed out of the same string the canvas edges are painted
    // with, so the two cannot drift.
    assert_eq!(PREDICTED_DASH, (4.0, 3.0));
}

#[test]
fn table_node_rendering_variants_stay_pinned_to_the_shared_contract() {
    // `table_node.rs` (T361) renders through `usize` wrappers that delegate
    // to this module; only the row-centre differs, because Rune draws the
    // border inside the box where React's CSS adds it to layout.
    use crate::views::er::table_node;
    assert_eq!(
        table_node::er_node_height(7, false),
        er_node_height(7, false)
    );
    assert_eq!(
        table_node::er_node_body_height(7),
        er_node_body_height(7)
    );
    assert_eq!(
        table_node::er_node_height(7, true),
        er_node_height(7, true)
    );
    // The rendering centre omits the border the contract-faithful one adds.
    assert_eq!(
        table_node::er_column_row_center_y(0),
        er_column_row_center_y(0) - ER_NODE_BORDER
    );
}
