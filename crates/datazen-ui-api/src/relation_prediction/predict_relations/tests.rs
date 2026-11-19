//! Mirrored React suites for [`predict_relations`].
//!
//! - `relationPrediction/__tests__/predictRelations.test.ts`
//!   (`describe('predictRelations')`) — every `it()` is one `#[test]`,
//!   fixtures field-for-field. Its `describe('normalize')` cases were
//!   already carried by normalize.rs (T033), so they are not repeated here.
//! - `relationPrediction/__tests__/predictRelations.scale.test.ts` — the
//!   database-scale guard, same 500×15 generation shape (Rust compiles and
//!   runs this far faster than the JS engine it guards, so no reduction was
//!   needed; only the clock is swapped for `std::time::Instant`).

use std::time::{Duration, Instant};

use super::super::from_table_schema::{DeclaredForeignKey, PredictionColumn, PredictionTable};
use super::*;

/// React's `col` helper: `integer`, not nullable, not indexed.
fn col(name: &str) -> PredictionColumn {
    PredictionColumn {
        name: name.to_string(),
        data_type: "integer".to_string(),
        nullable: false,
        indexed: false,
    }
}

/// React's `col(name, dataType)` overload.
fn col_typed(name: &str, data_type: &str) -> PredictionColumn {
    PredictionColumn {
        data_type: data_type.to_string(),
        ..col(name)
    }
}

/// React's `col(name, dataType, { indexed })` overload.
fn col_indexed(name: &str, data_type: &str, indexed: bool) -> PredictionColumn {
    PredictionColumn {
        indexed,
        ..col_typed(name, data_type)
    }
}

/// React's `table` helper: id = name, schema `public`, primary key `id`, no
/// unique sets, no declared foreign keys. Tests override the rest of the
/// fields after the fact (React's `over` spread).
fn table(name: &str, columns: Vec<PredictionColumn>) -> PredictionTable {
    PredictionTable {
        id: name.to_string(),
        name: name.to_string(),
        schema: Some("public".to_string()),
        columns,
        primary_key: vec!["id".to_string()],
        unique_column_sets: Vec::new(),
        declared_foreign_keys: Vec::new(),
    }
}

fn users() -> PredictionTable {
    table("users", vec![col("id"), col_typed("name", "varchar")])
}

fn orders() -> PredictionTable {
    table(
        "orders",
        vec![col("id"), col("user_id"), col_typed("total", "numeric")],
    )
}

/// React's `found[0].evidence.map((e) => e.code)` — as enum values.
fn evidence_codes(candidate: &RelationCandidate) -> Vec<PredictionEvidenceCode> {
    candidate.evidence.iter().map(|evidence| evidence.code).collect()
}

fn default_options() -> PredictionOptions {
    PredictionOptions::default()
}

#[test]
fn infers_the_conventional_table_id_relationship() {
    let found = predict_relations(&[users(), orders()], &default_options());
    assert_eq!(found.len(), 1);
    // React's toMatchObject picks these fields out of the candidate.
    assert_eq!(found[0].from_table, "orders");
    assert_eq!(found[0].to_table, "users");
    assert_eq!(
        found[0].column_pairs,
        vec![ColumnPair {
            left: "user_id".to_string(),
            right: "id".to_string()
        }]
    );
    assert_eq!(found[0].tier, PredictionTier::High);
    assert!(!found[0].ambiguous);
}

#[test]
fn accepts_the_un_singularized_table_name_too() {
    let order_lines = table("order_lines", vec![col("id"), col("orders_id")]);
    let found = predict_relations(&[orders(), order_lines], &default_options());
    assert!(found.iter().any(|candidate| candidate
        .column_pairs
        .first()
        .is_some_and(|pair| pair.left == "orders_id")));
}

#[test]
fn matches_a_prefixed_table_name_from_an_unprefixed_column() {
    // `app_user` is referenced by `user_id`; the table carries a prefix the
    // column drops, which is how most ORM-managed schemas are named.
    let app_users = table("app_users", vec![col("id")]);
    let sessions = table("sessions", vec![col("id"), col("user_id")]);
    let found = predict_relations(&[app_users, sessions], &default_options());
    assert_eq!(found.len(), 1);
    assert!(evidence_codes(&found[0]).contains(&PredictionEvidenceCode::NameMatchesTableStem));
    // Weaker than an exact match, so the score stays below the exact-name
    // case.
    assert!(found[0].score < 0.9);
}

#[test]
fn flags_a_prefix_collision_as_ambiguous_rather_than_choosing() {
    // `order_status` and `user_status` both end in `status`, so `status_id`
    // names both equally well.
    let order_status = table("order_status", vec![col("id")]);
    let user_status = table("user_status", vec![col("id")]);
    let rows = table("rows", vec![col("id"), col("status_id")]);
    let found = predict_relations(&[order_status, user_status, rows], &default_options());
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|candidate| candidate.ambiguous));
    assert!(found.iter().all(|candidate| candidate.tier == PredictionTier::Medium));
}

#[test]
fn never_relates_two_tables_through_their_generic_id_column_alone() {
    // Every table's surrogate key is `id`; matching on it would relate
    // everything to everything. Only a column that *names* the target
    // counts.
    let plain_orders = table("orders", vec![col("id"), col_typed("total", "numeric")]);
    assert!(predict_relations(&[users(), plain_orders], &default_options()).is_empty());
}

#[test]
fn returns_nothing_for_a_single_table() {
    assert!(predict_relations(&[users()], &default_options()).is_empty());
}

#[test]
fn skips_columns_already_covered_by_a_declared_foreign_key() {
    let mut declared = table("orders", vec![col("id"), col("user_id")]);
    declared.declared_foreign_keys = vec![DeclaredForeignKey {
        columns: vec!["user_id".to_string()],
        referenced_table: "users".to_string(),
        referenced_columns: vec!["id".to_string()],
    }];
    assert!(predict_relations(&[users(), declared], &default_options()).is_empty());
}

#[test]
fn rejects_a_type_family_mismatch_however_good_the_name_is() {
    // MySQL would compare '1' = 1 through an implicit cast, so the name
    // alone must not be enough.
    let string_fk = table("orders", vec![col("id"), col_typed("user_id", "varchar(32)")]);
    assert!(predict_relations(&[users(), string_fk], &default_options()).is_empty());
}

#[test]
fn accepts_different_numeric_widths() {
    let big_users = table("users", vec![col_typed("id", "bigint")]);
    let found = predict_relations(&[big_users, orders()], &default_options());
    assert_eq!(found.len(), 1);
    assert!(evidence_codes(&found[0]).contains(&PredictionEvidenceCode::TypeCompatible));
    assert!(!evidence_codes(&found[0]).contains(&PredictionEvidenceCode::TypeExact));
}

#[test]
fn will_not_target_a_non_key_column() {
    // `orders.name` matches `users.name` by name, but `name` is not a key.
    let named = table("orders", vec![col("id"), col_typed("name", "varchar")]);
    assert!(predict_relations(&[users(), named], &default_options()).is_empty());
}

#[test]
fn targets_a_unique_index_that_is_not_the_primary_key() {
    let mut accounts = table("accounts", vec![col("id"), col_typed("account_no", "varchar")]);
    accounts.unique_column_sets = vec![vec!["account_no".to_string()]];
    let invoices = table("invoices", vec![col("id"), col_typed("account_no", "varchar")]);
    let found = predict_relations(&[accounts, invoices], &default_options());
    assert_eq!(found.len(), 1);
    assert!(evidence_codes(&found[0]).contains(&PredictionEvidenceCode::TargetIsUnique));
}

#[test]
fn folds_a_composite_key_into_one_relationship_with_both_pairs() {
    let mut parents = table("parents", vec![col("id"), col("no")]);
    parents.primary_key = vec!["id".to_string(), "no".to_string()];
    let children = table("children", vec![col("id"), col("parent_id"), col("parent_no")]);
    let found = predict_relations(&[parents, children], &default_options());
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].column_pairs,
        vec![
            ColumnPair {
                left: "parent_id".to_string(),
                right: "id".to_string()
            },
            ColumnPair {
                left: "parent_no".to_string(),
                right: "no".to_string()
            },
        ]
    );
}

#[test]
fn keeps_two_separate_references_to_one_table_apart() {
    // `billing_address_id` and `shipping_address_id` are two relationships
    // to one table; merging them would invent an AND that matches nothing.
    let addresses = table("addresses", vec![col("id")]);
    let orders_with_two = table(
        "orders",
        vec![
            col("id"),
            col("billing_address_id"),
            col("shipping_address_id"),
        ],
    );
    let found = predict_relations(&[addresses, orders_with_two], &default_options());
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|candidate| candidate.column_pairs.len() == 1));
}

#[test]
fn refuses_to_choose_when_two_tables_match_equally_well() {
    // A legacy `person` table and a newer `people` one both singularize to
    // `person`, so `person_id` names both equally well.
    let legacy = table("person", vec![col("id")]);
    let modern = table("people", vec![col("id")]);
    let rows = table("rows", vec![col("id"), col("person_id")]);
    let found = predict_relations(&[legacy, modern, rows], &default_options());
    assert!(!found.is_empty());
    // Neither may be applied automatically.
    assert!(found.iter().all(|candidate| candidate.ambiguous));
    assert!(found.iter().all(|candidate| candidate.tier == PredictionTier::Medium));
    assert!(evidence_codes(&found[0]).contains(&PredictionEvidenceCode::Ambiguous));
}

#[test]
fn does_not_cross_schemas_by_default() {
    let mut other_schema = table("users", vec![col("id")]);
    other_schema.id = "sales.users".to_string();
    other_schema.schema = Some("sales".to_string());
    assert!(predict_relations(&[other_schema.clone(), orders()], &default_options()).is_empty());
    let cross = PredictionOptions {
        allow_cross_schema: Some(true),
        ..PredictionOptions::default()
    };
    assert_eq!(
        predict_relations(&[other_schema, orders()], &cross).len(),
        1
    );
}

#[test]
fn does_not_relate_a_table_to_itself() {
    let employees = table("employees", vec![col("id"), col("employee_id")]);
    assert!(predict_relations(&[employees], &default_options()).is_empty());
}

#[test]
fn drops_a_partial_composite_match_rather_than_emitting_half_a_predicate() {
    // Only one of the two key columns is present, so the ON clause would be
    // incomplete.
    let mut parents = table("parents", vec![col("id"), col("no")]);
    parents.primary_key = vec!["id".to_string(), "no".to_string()];
    let children = table("children", vec![col("id"), col("parent_id")]);
    assert!(predict_relations(&[parents, children], &default_options()).is_empty());
}

#[test]
fn produces_a_deterministic_id_and_stable_ordering() {
    let first = predict_relations(&[users(), orders()], &default_options());
    let second = predict_relations(&[orders(), users()], &default_options());
    assert_eq!(second, first);
}

#[test]
fn raises_the_score_when_the_source_column_is_indexed() {
    let plain = predict_relations(&[users(), orders()], &default_options());
    let indexed_orders = table(
        "orders",
        vec![col("id"), col_indexed("user_id", "integer", true)],
    );
    let indexed = predict_relations(&[users(), indexed_orders], &default_options());
    // React's toBe: both sides sum the same weights in the same
    // left-to-right order, so the doubles are bit-identical and exact
    // equality is the faithful assertion.
    assert_eq!(indexed[0].score, plain[0].score + EVIDENCE_WEIGHTS.source_indexed);
}

#[test]
fn honours_a_custom_high_threshold() {
    // Pin the score rather than assuming it, so the test fails loudly if
    // the weights move. React's toBeCloseTo(…, 3): |diff| < 10⁻³/2.
    let score = predict_relations(&[users(), orders()], &default_options())[0].score;
    let expected = EVIDENCE_WEIGHTS.target_is_primary_key
        + EVIDENCE_WEIGHTS.name_matches_table
        + EVIDENCE_WEIGHTS.type_exact;
    assert!((score - expected).abs() < 0.0005);
    let raised_options = PredictionOptions {
        high_threshold: Some(score + 0.01),
        ..PredictionOptions::default()
    };
    let raised = predict_relations(&[users(), orders()], &raised_options);
    assert_eq!(raised[0].tier, PredictionTier::Medium);
}

#[test]
fn explains_every_candidate_with_the_evidence_that_produced_it() {
    let found = predict_relations(&[users(), orders()], &default_options());
    let codes = evidence_codes(&found[0]);
    assert!(codes.contains(&PredictionEvidenceCode::NameMatchesTableStem));
    assert!(codes.contains(&PredictionEvidenceCode::TargetIsPrimaryKey));
    assert!(codes.contains(&PredictionEvidenceCode::TypeExact));
    assert!(found[0].evidence.iter().all(|evidence| !evidence.detail.is_empty()));
}

#[test]
fn lets_a_keyless_table_be_a_source_but_never_a_target() {
    // Without a key there is nothing to point at, so it cannot be
    // referenced — but its own columns may still reference a keyed table.
    let mut no_pk = table("views_like", vec![col("id"), col("user_id")]);
    no_pk.primary_key = Vec::new();
    let found = predict_relations(&[users(), no_pk], &default_options());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].from_table, "views_like");
    assert_eq!(found[0].to_table, "users");
}

#[test]
fn does_not_treat_a_unique_index_over_the_primary_key_as_a_second_target() {
    let mut users_with_unique_pk = table("users", vec![col("id")]);
    users_with_unique_pk.unique_column_sets = vec![vec!["id".to_string()]];
    let found = predict_relations(&[users_with_unique_pk, orders()], &default_options());
    assert_eq!(found.len(), 1);
    assert!(evidence_codes(&found[0]).contains(&PredictionEvidenceCode::TargetIsPrimaryKey));
    assert!(!evidence_codes(&found[0]).contains(&PredictionEvidenceCode::TargetIsUnique));
}

// ── `predictRelations.scale.test.ts` ──

/// The React generator, unchanged: `entity_{t}` with an `id` primary key,
/// 15 `field_{c}` columns (a third of them varchar), and every third table
/// carrying `entity_{t-1}_id` — a third of the tables reference an earlier
/// one, like a real schema.
fn make_schema(table_count: usize, cols_per_table: usize) -> Vec<PredictionTable> {
    let mut tables = Vec::with_capacity(table_count);
    for t in 0..table_count {
        let name = format!("entity_{t}");
        let mut columns = vec![col("id")];
        for c in 0..cols_per_table {
            columns.push(col_typed(
                &format!("field_{c}"),
                if c % 3 == 0 { "varchar(64)" } else { "integer" },
            ));
        }
        if t > 0 && t % 3 == 0 {
            columns.push(col(&format!("entity_{}_id", t - 1)));
        }
        tables.push(table(&name, columns));
    }
    tables
}

#[test]
fn stays_well_clear_of_a_quadratic_scan_on_a_500_table_schema() {
    // Guards the shape of the search, not a stopwatch reading: a return to
    // the quadratic scan is two orders of magnitude slower than this bound,
    // while ordinary CI jitter cannot fail it (the React bound of 150×
    // headroom, kept verbatim).
    let tables = make_schema(500, 15);
    let started = Instant::now();
    let found = predict_relations(&tables, &default_options());
    let elapsed = started.elapsed();

    // The relationships must still be found — a fast wrong answer is no
    // answer.
    assert!(found.len() > 150, "found {} candidates", found.len());
    assert!(found.iter().all(|candidate| !candidate.column_pairs.is_empty()));
    assert!(
        elapsed < Duration::from_millis(2000),
        "prediction took {elapsed:?}"
    );
}
