//! T405 audit (tester): independent checks of the foreign-key jump wording.
//! Fresh vectors: three-part tables, empty-table and trailing-bracket edge
//! labels, and the resolving word.

use datazen_ui::components::data_table::fk_cell_text::{fk_jump_label, fk_resolving_label};

#[test]
fn audit_multi_part_tables_and_long_column_lists() {
    // The table is everything before the FIRST '.(' — dots included.
    assert_eq!(
        fk_jump_label("db.sales.parents.(order_id, sku_id, line_no)"),
        "跳转到 db.sales.parents（order_id, sku_id, line_no）"
    );
    // A three-part single-column target (no bracket at all).
    assert_eq!(fk_jump_label("db.sales.orders"), "跳转到 db.sales.orders");
}

#[test]
fn audit_the_empty_table_edge() {
    // compositeAt = 0: the table slice is empty; the label reads "跳转到
    // （x）" — the template brackets an empty table name, exactly as the
    // React interpolation would.
    assert_eq!(fk_jump_label(".(x)"), "跳转到 （x）");
}

#[test]
fn audit_trailing_brackets_after_the_columns() {
    // slice(compositeAt + 2, -1) drops exactly ONE trailing character: the
    // ')' before the template's own bracket is the intended closer, and a
    // second ')' after it survives inside the columns.
    assert_eq!(fk_jump_label("t.(a))"), "跳转到 t（a)）");
}

#[test]
fn audit_the_marker_needs_both_the_dot_and_the_bracket() {
    // '.(' requires BOTH characters adjacent: a dot followed by anything
    // else, or a bracket alone, leaves the label single-column.
    assert_eq!(fk_jump_label("sales(orders"), "跳转到 sales(orders");
    assert_eq!(fk_jump_label("sales.orders."), "跳转到 sales.orders.");
}

#[test]
fn audit_the_resolving_word_is_distinct_from_the_jump_words() {
    let resolving = fk_resolving_label();
    assert_eq!(resolving, "正在解析外键…");
    // And it is its own word, not composed of the jump templates.
    assert_ne!(resolving, fk_jump_label("any"));
}
