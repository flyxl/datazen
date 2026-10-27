//! The constraint lookup and the uniqueness rule.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// §3.2 / §3.4 · the constraint lookup口径 is the same on both sides
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_constraint_lookup_prefers_the_name_and_falls_back_to_the_ordered_columns() {
    let foreign_keys = vec![
        foreign_key("fk_composite", &["a", "b"], "parents", &["x", "y"]),
        foreign_key("fk_single", &["a"], "parents", &["x"]),
    ];

    // The name wins.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("fk_composite")).map(|fk| fk.name.as_str()),
        Some("fk_composite")
    );
    // An unknown name falls back to the ordered columns.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("does_not_exist"))
            .map(|fk| fk.name.as_str()),
        Some("fk_single")
    );
    // No name at all: the ordered columns decide.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], None).map(|fk| fk.name.as_str()),
        Some("fk_single")
    );
    // A different order is a different key.
    assert!(find_fk(&foreign_keys, &["b".to_string(), "a".to_string()], None).is_none());
    // A blank name is no name.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("   ")).map(|fk| fk.name.as_str()),
        Some("fk_single")
    );
}

#[test]
fn uniqueness_needs_an_exactly_covering_index() {
    let wide = schema(
        "parents",
        vec![
            column("x", "integer", false),
            column("y", "text", false),
            column("z", "text", false),
        ],
        Vec::new(),
        vec![index("parents_x_y_z", &["x", "y", "z"], true)],
        Vec::new(),
    );
    assert!(!referenced_columns_are_unique(
        &wide,
        &["x".to_string(), "y".to_string()]
    ));
    assert!(referenced_columns_are_unique(
        &wide,
        &["x".to_string(), "y".to_string(), "z".to_string()]
    ));
    let keyed = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    assert!(referenced_columns_are_unique(&keyed, &["id".to_string()]));
    assert!(!referenced_columns_are_unique(&keyed, &[]));
}
