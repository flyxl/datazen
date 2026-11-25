//! T405: the foreign-key jump wording — single-column targets, composite
//! targets, and the in-flight label.

use datazen_ui::components::data_table::fk_cell_text::{fk_jump_label, fk_resolving_label};

#[test]
fn a_single_column_target_interpolates_as_target() {
    assert_eq!(fk_jump_label("customers.id"), "跳转到 customers.id");
    // A label that merely contains a dot but no bracket is single-column.
    assert_eq!(fk_jump_label("sales.parents"), "跳转到 sales.parents");
}

#[test]
fn a_composite_target_names_table_and_columns() {
    // `table.(column, column)`: everything before the bracket is the table,
    // everything inside it the columns, closing bracket excluded.
    assert_eq!(fk_jump_label("sales.parents.(x, y)"), "跳转到 sales.parents（x, y）");
    assert_eq!(fk_jump_label("t.(a)"), "跳转到 t（a）");
    assert_eq!(
        fk_jump_label("a.b.c.(one, two, three)"),
        "跳转到 a.b.c（one, two, three）"
    );
}

#[test]
fn the_first_bracket_wins() {
    // indexOf('.(') finds the FIRST occurrence: a column name containing
    // the marker cannot redirect the split. The template's own closing
    // bracket rides after whatever the columns slice produced.
    assert_eq!(fk_jump_label("t.((x)"), "跳转到 t（(x）");
    // A label with no marker stays whole even if it has a closing bracket.
    assert_eq!(fk_jump_label("weird)label"), "跳转到 weird)label");
}

#[test]
fn the_resolving_label_is_its_own_word() {
    assert_eq!(fk_resolving_label(), "正在解析外键…");
}
