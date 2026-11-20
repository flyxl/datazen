//! Which tables are related to the ones already in play.
//!
//! Ported from React
//! `packages/frontend/src/lib/relationPrediction/relatedTables.ts`
//! (migration tasklist T121; this segment's tracker assigns the port as
//! T119 — the same tracker/tasklist renumbering the engine's header
//! records for T081/T082). The editor's completion list is built from the
//! schema tree, so it already contains every candidate table. What it
//! cannot know is which of them the user is *likely* to want next — and
//! that is exactly what a relationship tells us. Boosting beats filtering
//! here: a related table rises to the top, while an unrelated one stays
//! reachable, so the ranking never removes a valid choice.
//!
//! Only two evidence sources are used, because both are cheap:
//!
//! - **Declared foreign keys.** A loaded relation names its targets
//!   outright, so the target needs no extra fetch to be boosted.
//! - **Predicted relationships among the loaded relations.** These are
//!   the tables the statement already mentions, so no metadata beyond
//!   what is loaded is required.
//!
//! Prediction over the *whole* schema is deliberately not attempted: it
//! would need every table's primary key and column types, turning a
//! keystroke into a database-wide metadata fetch.
//!
//! ## Port decisions
//!
//! - `origin: 'declared' | 'predicted'` becomes the enum
//!   [`RelatedTableOrigin`] whose `as_str` pins the React literal, the
//!   family's standing rule for string-literal unions.
//! - `boost: number` is `i64`, not `f64`: the only values ever written are
//!   the two integer constants, and the field is only compared and summed
//!   with the completion's base boost — the crate's count precedent
//!   (chart_types' `distinctCount`), not the double a fractional JS number
//!   would need.
//! - React deduplicates through a `Map` keyed by table name and then
//!   sorts `seen.values()`. The comparator is total — boost descending,
//!   then name ascending, and names are unique map keys — so the result
//!   never depends on JS Map insertion order, and a [`HashMap`] plus the
//!   same comparator is exactly as deterministic here.
//! - `relatedTableBoostMap`'s `fold: (name: string) => string` parameter
//!   becomes a closure parameter (`F: Fn(&str) -> String`); React shares
//!   the hint objects into the output map, the port clones them (four
//!   small fields, and `RelatedTableHint` stays plain-owned).
//! - serde: nothing here is serialized (the completion list is a UI-side
//!   render concern), so these are plain structs — the same call
//!   `from_table_schema.rs` and `predict_relations/types.rs` made for the
//!   prediction family.

use std::collections::HashMap;

use super::from_table_schema::PredictionTable;
use super::predict_relations::{PredictionOptions, predict_relations};

/// A table worth surfacing, and why (React `RelatedTableHint`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedTableHint {
    /// Table name as it should be matched against a completion label.
    pub name: String,
    /// Declared constraints outrank inferences.
    pub origin: RelatedTableOrigin,
    /// Added to the completion's base boost; higher sorts first.
    pub boost: i64,
    /// The loaded table that leads here, for the completion's detail text.
    pub via: String,
}

/// React's `origin: 'declared' | 'predicted'` union.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RelatedTableOrigin {
    Declared,
    Predicted,
}

impl RelatedTableOrigin {
    /// React's literal, pinned so the port stays auditable against the
    /// source.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Predicted => "predicted",
        }
    }
}

/// A declared foreign key target is near-certain; an inference is a nudge.
pub const DECLARED_RELATION_BOOST: i64 = 20;
pub const PREDICTED_RELATION_BOOST: i64 = 12;

/// Keep the strongest reason for each table; a declared constraint already
/// covers anything a prediction could add. React's `push` closure, spelled
/// as a function so both loops share it.
fn push(seen: &mut HashMap<String, RelatedTableHint>, hint: RelatedTableHint) {
    if let Some(existing) = seen.get(&hint.name) {
        if existing.boost >= hint.boost {
            return;
        }
    }
    let key = hint.name.clone();
    seen.insert(key, hint);
}

/// Hints for tables related to `loaded`, most confident first.
///
/// `loaded` must be the relations the editor already has metadata for —
/// normally the tables referenced in the statement.
pub fn related_table_hints(loaded: &[PredictionTable]) -> Vec<RelatedTableHint> {
    if loaded.is_empty() {
        return Vec::new();
    }

    let mut seen: HashMap<String, RelatedTableHint> = HashMap::new();

    for table in loaded {
        for fk in &table.declared_foreign_keys {
            if fk.referenced_table == table.name {
                continue;
            }
            push(
                &mut seen,
                RelatedTableHint {
                    name: fk.referenced_table.clone(),
                    origin: RelatedTableOrigin::Declared,
                    boost: DECLARED_RELATION_BOOST,
                    via: table.name.clone(),
                },
            );
        }
    }

    for candidate in predict_relations(loaded, &PredictionOptions::default()) {
        let Some(from) = loaded.iter().find(|table| table.id == candidate.from_table) else {
            continue;
        };
        let Some(to) = loaded.iter().find(|table| table.id == candidate.to_table) else {
            continue;
        };
        // An ambiguous candidate is a poor hint; offering it would push a
        // coin flip to the top of the list.
        if candidate.ambiguous {
            continue;
        }
        push(
            &mut seen,
            RelatedTableHint {
                name: to.name.clone(),
                origin: RelatedTableOrigin::Predicted,
                boost: PREDICTED_RELATION_BOOST,
                via: from.name.clone(),
            },
        );
    }

    // React sorts `b.boost - a.boost || name asc`; the map's keys are unique
    // names, so the comparator is total and HashMap iteration order cannot
    // leak into the output.
    let mut hints: Vec<RelatedTableHint> = seen.into_values().collect();
    hints.sort_by(|a, b| b.boost.cmp(&a.boost).then_with(|| a.name.cmp(&b.name)));
    hints
}

/// Collapse hints into a boost lookup keyed by folded table name.
///
/// Folded so `Orders` and `orders` resolve to one entry: the completion
/// label is quoted per dialect, and the caller folds both sides the same
/// way.
pub fn related_table_boost_map<F>(
    hints: &[RelatedTableHint],
    fold: F,
) -> HashMap<String, RelatedTableHint>
where
    F: Fn(&str) -> String,
{
    let mut map: HashMap<String, RelatedTableHint> = HashMap::new();
    for hint in hints {
        let key = fold(&hint.name);
        let replace = match map.get(&key) {
            Some(existing) => existing.boost < hint.boost,
            None => true,
        };
        if replace {
            map.insert(key, hint.clone());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::super::from_table_schema::{DeclaredForeignKey, PredictionColumn, PredictionTable};
    use super::*;

    // The React suite (`relationPrediction/__tests__/relatedTables.test.ts`)
    // is carried it() by it(): the seven under `describe('relatedTableHints')`
    // and the two under `describe('relatedTableBoostMap')`.

    /// React's `col` helper: `integer`, not nullable, not indexed.
    fn col(name: &str) -> PredictionColumn {
        PredictionColumn {
            name: name.to_string(),
            data_type: "integer".to_string(),
            nullable: false,
            indexed: false,
        }
    }

    /// React's `declaredForeignKeys` entry shape.
    fn declared_fk(
        columns: &[&str],
        referenced_table: &str,
        referenced_columns: &[&str],
    ) -> DeclaredForeignKey {
        DeclaredForeignKey {
            columns: columns.iter().map(|column| column.to_string()).collect(),
            referenced_table: referenced_table.to_string(),
            referenced_columns: referenced_columns
                .iter()
                .map(|column| column.to_string())
                .collect(),
        }
    }

    /// React's `table(over)` base: id/name `orders`, schema `public`, an
    /// `id` primary key and the `id` + `user_id` columns, nothing else.
    /// Overrides ride on struct-update (React's `...over` spread), so the
    /// helper is named `base_orders` — the React tests' locals are all
    /// called `orders`, and a helper of that name would shadow itself in
    /// `let orders = PredictionTable { ..orders() }`.
    fn base_orders() -> PredictionTable {
        PredictionTable {
            id: "orders".to_string(),
            name: "orders".to_string(),
            schema: Some("public".to_string()),
            columns: vec![col("id"), col("user_id")],
            primary_key: vec!["id".to_string()],
            unique_column_sets: Vec::new(),
            declared_foreign_keys: Vec::new(),
        }
    }

    /// React's `users` fixture: one `id` column, itself the primary key.
    fn users() -> PredictionTable {
        PredictionTable {
            id: "users".to_string(),
            name: "users".to_string(),
            columns: vec![col("id")],
            ..base_orders()
        }
    }

    fn hint(name: &str, origin: RelatedTableOrigin, boost: i64, via: &str) -> RelatedTableHint {
        RelatedTableHint {
            name: name.to_string(),
            origin,
            boost,
            via: via.to_string(),
        }
    }

    #[test]
    fn returns_nothing_for_no_loaded_tables() {
        assert_eq!(related_table_hints(&[]), Vec::new());
    }

    #[test]
    fn surfaces_a_declared_foreign_key_target() {
        let orders = PredictionTable {
            declared_foreign_keys: vec![declared_fk(&["user_id"], "users", &["id"])],
            ..base_orders()
        };
        assert_eq!(
            related_table_hints(&[orders, users()]),
            vec![hint("users", RelatedTableOrigin::Declared, DECLARED_RELATION_BOOST, "orders")]
        );
    }

    #[test]
    fn never_suggests_a_table_as_related_to_itself() {
        let self_ref = PredictionTable {
            declared_foreign_keys: vec![declared_fk(&["manager_id"], "orders", &["id"])],
            ..base_orders()
        };
        assert_eq!(related_table_hints(&[self_ref]), Vec::new());
    }

    #[test]
    fn nudges_towards_a_predicted_target() {
        // No constraint anywhere — only the naming convention links the two.
        assert_eq!(
            related_table_hints(&[base_orders(), users()]),
            vec![hint(
                "users",
                RelatedTableOrigin::Predicted,
                PREDICTED_RELATION_BOOST,
                "orders"
            )]
        );
    }

    #[test]
    fn prefers_the_declared_reason_when_both_apply() {
        let orders = PredictionTable {
            declared_foreign_keys: vec![declared_fk(&["user_id"], "users", &["id"])],
            ..base_orders()
        };
        let hints = related_table_hints(&[orders, users()]);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].origin, RelatedTableOrigin::Declared);
    }

    #[test]
    fn drops_an_ambiguous_prediction_instead_of_ranking_a_coin_flip_first() {
        // A legacy `person` and a newer `people` both answer to `person_id`.
        let legacy = PredictionTable {
            id: "person".to_string(),
            name: "person".to_string(),
            columns: vec![col("id")],
            ..base_orders()
        };
        let modern = PredictionTable {
            id: "people".to_string(),
            name: "people".to_string(),
            ..legacy.clone()
        };
        let rows = PredictionTable {
            id: "rows".to_string(),
            name: "rows".to_string(),
            columns: vec![col("id"), col("person_id")],
            ..base_orders()
        };
        assert_eq!(related_table_hints(&[legacy, modern, rows]), Vec::new());
    }

    #[test]
    fn orders_the_strongest_hint_first() {
        let orders = PredictionTable {
            declared_foreign_keys: vec![declared_fk(&["user_id"], "users", &["id"])],
            ..base_orders()
        };
        let invoices = PredictionTable {
            id: "invoices".to_string(),
            name: "invoices".to_string(),
            columns: vec![col("id"), col("order_id")],
            ..base_orders()
        };
        let hints = related_table_hints(&[orders, users(), invoices]);
        // `users` is declared and must outrank anything inferred.
        assert_eq!(
            hints[0],
            hint("users", RelatedTableOrigin::Declared, DECLARED_RELATION_BOOST, "orders")
        );
        // `invoices.order_id` names `orders`, so the inference points back
        // at the already-loaded `orders` — the caller decides whether that
        // is worth showing.
        assert!(hints.contains(&hint(
            "orders",
            RelatedTableOrigin::Predicted,
            PREDICTED_RELATION_BOOST,
            "invoices"
        )));
    }

    #[test]
    fn sorts_equal_boost_hints_by_name_ascending() {
        // [tester] The comparator's name tiebreak was unpinned: every React
        // it() that sorts has distinct boosts. Two declared targets at the
        // same boost, with the alphabetically-later one loaded (and pushed)
        // first — only `name asc` can produce this order.
        let w = PredictionTable {
            id: "w".to_string(),
            name: "w".to_string(),
            columns: vec![col("id"), col("zeta_id")],
            declared_foreign_keys: vec![declared_fk(&["zeta_id"], "zeta", &["id"])],
            ..base_orders()
        };
        let zeta = PredictionTable {
            id: "zeta".to_string(),
            name: "zeta".to_string(),
            columns: vec![col("id")],
            ..base_orders()
        };
        let m = PredictionTable {
            id: "m".to_string(),
            name: "m".to_string(),
            columns: vec![col("id"), col("beta_id")],
            declared_foreign_keys: vec![declared_fk(&["beta_id"], "beta", &["id"])],
            ..base_orders()
        };
        let beta = PredictionTable {
            id: "beta".to_string(),
            name: "beta".to_string(),
            columns: vec![col("id")],
            ..base_orders()
        };
        assert_eq!(
            related_table_hints(&[w, zeta, m, beta]),
            vec![
                hint("beta", RelatedTableOrigin::Declared, DECLARED_RELATION_BOOST, "m"),
                hint("zeta", RelatedTableOrigin::Declared, DECLARED_RELATION_BOOST, "w"),
            ]
        );
    }

    #[test]
    fn keeps_the_first_hint_when_an_equal_boost_hint_arrives_second() {
        // [tester] `push`'s `>=` was unpinned: within `related_table_hints`
        // declared (20) always precedes predicted (12), so no React it()
        // exercises an equal-boost collision. Two loaded tables declare the
        // same target at the same boost — React's `existing.boost >=
        // hint.boost` keeps the first `via` (`w`), not the last.
        let w = PredictionTable {
            id: "w".to_string(),
            name: "w".to_string(),
            columns: vec![col("id"), col("x_id")],
            declared_foreign_keys: vec![declared_fk(&["x_id"], "x", &["id"])],
            ..base_orders()
        };
        let m = PredictionTable {
            id: "m".to_string(),
            name: "m".to_string(),
            columns: vec![col("id"), col("x_id")],
            declared_foreign_keys: vec![declared_fk(&["x_id"], "x", &["id"])],
            ..base_orders()
        };
        let x = PredictionTable {
            id: "x".to_string(),
            name: "x".to_string(),
            columns: vec![col("id")],
            ..base_orders()
        };
        assert_eq!(
            related_table_hints(&[w, m, x]),
            vec![hint("x", RelatedTableOrigin::Declared, DECLARED_RELATION_BOOST, "w")]
        );
    }

    #[test]
    fn folds_names_so_quoting_differences_resolve_to_one_entry() {
        // React's fold argument: `(name) => name.toLowerCase()`.
        let map = related_table_boost_map(
            &[hint("Users", RelatedTableOrigin::Declared, 20, "orders")],
            |name| name.to_lowercase(),
        );
        assert_eq!(
            map.get("users"),
            Some(&hint("Users", RelatedTableOrigin::Declared, 20, "orders"))
        );
    }

    #[test]
    fn keeps_the_strongest_hint_per_table() {
        // React's fold argument: `(name) => name` (identity).
        let map = related_table_boost_map(
            &[
                hint("users", RelatedTableOrigin::Predicted, 12, "a"),
                hint("users", RelatedTableOrigin::Declared, 20, "b"),
                hint("users", RelatedTableOrigin::Predicted, 12, "c"),
            ],
            |name| name.to_string(),
        );
        assert_eq!(
            map.get("users"),
            Some(&hint("users", RelatedTableOrigin::Declared, 20, "b"))
        );
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn origin_as_str_pins_the_react_literals() {
        // [tester] The `origin: 'declared' | 'predicted'` union crosses to
        // consumers as these literals (`RelatedTableOrigin::as_str`); no
        // other test in the crate exercises them, so the mirror table was
        // unpinned.
        assert_eq!(RelatedTableOrigin::Declared.as_str(), "declared");
        assert_eq!(RelatedTableOrigin::Predicted.as_str(), "predicted");
    }

    #[test]
    fn the_boost_constants_are_the_react_literals() {
        // [tester] Supplementary: React's own suite asserts the constants
        // symbolically (`boost: DECLARED_RELATION_BOOST`) or through
        // hand-built literal hints, so swapping the two constants — or
        // drifting either literal — escapes its net; only the ordering
        // tests would notice. The literals are the public scoring contract
        // (query completions rank by them), so they are pinned at their
        // source, then once more end-to-end through the declared path.
        assert_eq!(DECLARED_RELATION_BOOST, 20);
        assert_eq!(PREDICTED_RELATION_BOOST, 12);

        let orders = PredictionTable {
            declared_foreign_keys: vec![declared_fk(&["user_id"], "users", &["id"])],
            ..base_orders()
        };
        assert_eq!(
            related_table_hints(&[orders, users()]),
            vec![hint("users", RelatedTableOrigin::Declared, 20, "orders")]
        );
    }
}
