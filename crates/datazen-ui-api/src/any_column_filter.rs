//! The Any Column filter (DBR-04) — ported from React
//! `packages/frontend/src/lib/anyColumnFilter.ts` (task T291, 82 lines).
//!
//! One value is matched across every text-searchable column. React's key
//! decision is that the expansion is a **generator of structured
//! conditions** — one `contains` condition per searchable column, joined
//! with `or` — and not a second query dialect. Everything downstream
//! (the operator whitelist, the SQL rendering, the preview, the count, the
//! saved views) already speaks `FilterCondition`; reusing that channel is
//! what keeps the Any Column filter from drifting from the structured one.
//!
//! Which columns are searchable follows the value search's declaration
//! (DBR-03): text / enum / unknown always, plus the type groups the driver
//! declared its `LIKE` can text-match. A column whose type is not in the
//! set is **skipped and reported**, never silently cast into the match — a
//! type-cast failure must not masquerade as "no match".
//!
//! ## Mirror rules applied
//!
//! - React's `options.groups.length === 0 ? 'text' : …` scope ladder is
//!   preserved exactly, including its quirk that a non-empty group list
//!   with no `number` maps to `text+datetime` (["number"] and
//!   ["datetime"] both being the only other legal groups).
//! - `value.trim()` is Unicode-aware [`str::trim`]; an empty trimmed value
//!   expands to **no conditions** (the caller's cue to clear the filter)
//!   while the searchable/skipped classification is still reported.
//! - Every emitted condition is `operator: contains`, `connector: 'or'`
//!   except the first (which has none), and its operand is the trimmed
//!   string — never a cast, never a driver-specific expression. This is a
//!   soft "match any column" question, not an irreversible hard filter.
//! - `FilterCondition`, `ColumnSchema`, `Value`, `CellTypeHints` and the
//!   scope/group vocabulary are reused from this crate's existing ports;
//!   the small type-resolution projection this file needs lives in
//!   [`cell_types`] because `cellTypes.ts`/`valueSearch.ts` have not landed
//!   in this crate yet (see that submodule's ownership note).
//! - No production `unwrap`/`expect`: the lookup is an exhaustive `match`
//!   on the closed scope enum, and all collection walks are plain loops.

mod cell_types;

use crate::connection_types::{ColumnSchema, Value};
use crate::filter_types::{FilterCondition, FilterConnector, FilterOperator};
pub use cell_types::{
    resolve_cell_type, CellType, CellTypeHints, CellTypeMatch, CellTypeRule, ValueSearchTypeGroup,
    ValueSearchTypeScope, DEFAULT_CELL_TYPE_RULES,
};

/// What the expansion produced: the conditions, and what it left out.
// `FilterCondition` carries a `Value` (serde_json::Value, not `Eq` because
// of its f64 arm), so the expansion is `PartialEq` only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnyColumnExpansion {
    /// One `contains` condition per searchable column, OR-joined.
    pub conditions: Vec<FilterCondition>,
    /// The columns the filter will actually search, in schema order.
    pub searchable_columns: Vec<String>,
    /// The columns skipped because their type is not text-searchable,
    /// in schema order. Reported so the UI can tell the user instead of
    /// silently dropping them.
    pub skipped_columns: Vec<String>,
}

/// React `expandAnyColumnFilter`'s `options` argument: the driver-declared
/// extra type groups and the (optional) dialect type hints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnyColumnFilterOptions {
    pub groups: Vec<ValueSearchTypeGroup>,
    pub hints: Option<CellTypeHints>,
}

/// The React scope ladder over `options.groups`. Kept as a function so each
/// arm is independently testable and the empty-list default is pinned.
fn scope_for_groups(groups: &[ValueSearchTypeGroup]) -> ValueSearchTypeScope {
    if groups.is_empty() {
        ValueSearchTypeScope::Text
    } else if groups.contains(&ValueSearchTypeGroup::Number)
        && groups.contains(&ValueSearchTypeGroup::Datetime)
    {
        ValueSearchTypeScope::TextNumberDatetime
    } else if groups.contains(&ValueSearchTypeGroup::Number) {
        ValueSearchTypeScope::TextNumber
    } else {
        ValueSearchTypeScope::TextDatetime
    }
}

/// Expand a value into the structured conditions of the Any Column filter.
///
/// The value is matched as a `contains` (the shared filter layer escapes the
/// user's `%`), and an empty value expands to no conditions — the caller's
/// signal to clear the filter rather than ask a question that matches
/// nothing. Non-searchable columns are reported in `skipped_columns`, not
/// folded into a cast.
pub fn expand_any_column_filter(
    columns: &[ColumnSchema],
    value: &str,
    options: &AnyColumnFilterOptions,
) -> AnyColumnExpansion {
    let searchable = cell_types::cell_types_for_scope(scope_for_groups(&options.groups));
    let trimmed = value.trim();
    let mut conditions = Vec::new();
    let mut searchable_columns = Vec::new();
    let mut skipped_columns = Vec::new();

    for column in columns {
        let cell_type = resolve_cell_type(Some(&column.data_type), options.hints.as_ref());
        if !searchable.contains(&cell_type) {
            skipped_columns.push(column.name.clone());
            continue;
        }
        searchable_columns.push(column.name.clone());
        if trimmed.is_empty() {
            continue;
        }
        conditions.push(FilterCondition {
            column: column.name.clone(),
            operator: FilterOperator::Contains,
            value: Some(Value::String(trimmed.to_owned())),
            // Every condition joins the previous one with OR: the Any
            // Column question is "does the value appear in **any** column".
            connector: if conditions.is_empty() {
                None
            } else {
                Some(FilterConnector::Or)
            },
        });
    }

    AnyColumnExpansion { conditions, searchable_columns, skipped_columns }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, data_type: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.to_owned(),
            data_type: data_type.to_owned(),
            nullable: false,
            default_value: None,
            is_primary_key: None,
            is_auto_increment: None,
            comment: None,
        }
    }

    fn sample_columns() -> Vec<ColumnSchema> {
        vec![
            column("full_name", "VARCHAR(64)"),
            column("status", "enum('a','b')"),
            column("mystery", "USER-DEFINED"),
            column("amount", "INT"),
            column("created", "TIMESTAMP"),
            column("active", "BOOLEAN"),
            column("payload", "JSONB"),
            column("id", "UUID"),
            column("doc", "BLOB"),
        ]
    }

    fn contains(column: &str, value: &str, connector: Option<FilterConnector>) -> FilterCondition {
        FilterCondition {
            column: column.to_owned(),
            operator: FilterOperator::Contains,
            value: Some(Value::String(value.to_owned())),
            connector,
        }
    }

    // The default (no groups) scope: text/enum/unknown searchable, everything
    // else skipped and reported. Conditions are OR-joined in schema order.
    #[test]
    fn default_scope_searches_text_enum_unknown_and_reports_the_skipped_columns() {
        let expanded = expand_any_column_filter(
            &sample_columns(),
            "alice",
            &AnyColumnFilterOptions::default(),
        );
        assert_eq!(
            expanded.searchable_columns,
            vec!["full_name", "status", "mystery"]
        );
        assert_eq!(
            expanded.skipped_columns,
            vec!["amount", "created", "active", "payload", "id", "doc"]
        );
        assert_eq!(
            expanded.conditions,
            vec![
                contains("full_name", "alice", None),
                contains("status", "alice", Some(FilterConnector::Or)),
                contains("mystery", "alice", Some(FilterConnector::Or)),
            ]
        );
        assert!(expanded
            .conditions
            .iter()
            .all(|condition| condition.operator == FilterOperator::Contains));
    }

    #[test]
    fn every_emitted_connector_is_or_never_and_and_first_has_none() {
        let expanded = expand_any_column_filter(
            &sample_columns(),
            "x",
            &AnyColumnFilterOptions::default(),
        );
        assert_eq!(expanded.conditions[0].connector, None);
        assert_eq!(expanded.conditions.len(), 3);
        for condition in expanded.conditions.iter().skip(1) {
            assert_eq!(condition.connector, Some(FilterConnector::Or));
        }
    }

    // An empty value clears the filter (no conditions) but the classification
    // is still reported so the pane can show its "N searchable / M skipped" hint.
    #[test]
    fn empty_and_whitespace_values_emit_no_conditions_but_keep_classification() {
        for value in ["", "   ", "\t\n", "\u{3000}"] {
            let expanded = expand_any_column_filter(
                &sample_columns(),
                value,
                &AnyColumnFilterOptions::default(),
            );
            assert!(expanded.conditions.is_empty(), "{value:?}");
            assert_eq!(expanded.searchable_columns.len(), 3);
            assert_eq!(expanded.skipped_columns.len(), 6);
        }
    }

    #[test]
    fn the_trimmed_value_is_the_condition_operand() {
        for (input, expected) in [
            ("  alice  ", "alice"),
            ("\u{3000}alice\u{00A0}", "alice"),
            ("a b", "a b"),
        ] {
            let expanded = expand_any_column_filter(
                &[column("name", "text")],
                input,
                &AnyColumnFilterOptions::default(),
            );
            assert_eq!(
                expanded.conditions,
                vec![contains("name", expected, None)],
                "{input:?}"
            );
        }
    }

    #[test]
    fn a_number_group_adds_numeric_columns_but_not_datetimes() {
        let expanded = expand_any_column_filter(
            &sample_columns(),
            "x",
            &AnyColumnFilterOptions {
                groups: vec![ValueSearchTypeGroup::Number],
                hints: None,
            },
        );
        assert_eq!(
            expanded.searchable_columns,
            vec!["full_name", "status", "mystery", "amount"]
        );
        assert_eq!(
            expanded.skipped_columns,
            vec!["created", "active", "payload", "id", "doc"]
        );
    }

    #[test]
    fn a_datetime_group_adds_date_time_and_datetime_columns() {
        let expanded = expand_any_column_filter(
            &sample_columns(),
            "x",
            &AnyColumnFilterOptions {
                groups: vec![ValueSearchTypeGroup::Datetime],
                hints: None,
            },
        );
        assert_eq!(
            expanded.searchable_columns,
            vec!["full_name", "status", "mystery", "created"]
        );
        assert_eq!(
            expanded.skipped_columns,
            vec!["amount", "active", "payload", "id", "doc"]
        );
        // Date/time/datetime all join when the group is declared.
        let mixed = expand_any_column_filter(
            &[column("d", "date"), column("t", "time"), column("dt", "datetime")],
            "x",
            &AnyColumnFilterOptions {
                groups: vec![ValueSearchTypeGroup::Datetime],
                hints: None,
            },
        );
        assert_eq!(mixed.searchable_columns, vec!["d", "t", "dt"]);
        assert!(mixed.skipped_columns.is_empty());
    }

    #[test]
    fn both_groups_add_every_declared_family() {
        let expanded = expand_any_column_filter(
            &sample_columns(),
            "x",
            &AnyColumnFilterOptions {
                groups: vec![
                    ValueSearchTypeGroup::Number,
                    ValueSearchTypeGroup::Datetime,
                ],
                hints: None,
            },
        );
        assert_eq!(
            expanded.searchable_columns,
            vec!["full_name", "status", "mystery", "amount", "created"]
        );
        assert_eq!(expanded.skipped_columns, vec!["active", "payload", "id", "doc"]);
        // The scope ladder is order-independent for a two-group set.
        let reversed = expand_any_column_filter(
            &sample_columns(),
            "x",
            &AnyColumnFilterOptions {
                groups: vec![
                    ValueSearchTypeGroup::Datetime,
                    ValueSearchTypeGroup::Number,
                ],
                hints: None,
            },
        );
        assert_eq!(expanded, reversed);
    }

    #[test]
    fn skipped_columns_are_reported_not_silently_cast_into_a_condition() {
        let columns = [column("name", "text"), column("flag", "bool"), column("blob", "bytea")];
        let expanded = expand_any_column_filter(
            &columns,
            "needle",
            &AnyColumnFilterOptions::default(),
        );
        // The soft question is "any searchable column", not a hard filter on
        // the whole row: non-searchable columns stay visible in the report.
        assert_eq!(expanded.skipped_columns, vec!["flag", "blob"]);
        assert_eq!(expanded.conditions.len(), 1);
        assert_eq!(expanded.conditions[0].column, "name");
        assert_eq!(expanded.conditions[0].value, Some(Value::String("needle".into())));
    }

    #[test]
    fn driver_hints_change_searchability_without_a_cast() {
        let options = AnyColumnFilterOptions {
            groups: vec![],
            hints: Some(CellTypeHints {
                overrides: vec![
                    CellTypeRule {
                        match_kind: CellTypeMatch::Token,
                        pattern: "timestamp",
                        cell_type: CellType::Bytes,
                    },
                    CellTypeRule {
                        match_kind: CellTypeMatch::Substring,
                        pattern: "bool",
                        cell_type: CellType::Text,
                    },
                ],
                enum_types: vec![String::from("mystery")],
                ..CellTypeHints::default()
            }),
        };
        let expanded = expand_any_column_filter(
            &[
                column("created", "TIMESTAMP"),
                column("active", "BOOLEAN"),
                column("weird", "MYSTERY"),
            ],
            "x",
            &options,
        );
        // A dialect trap that calls `timestamp` a row-version removes that
        // column; a driver can also promote a type into the text set.
        assert_eq!(expanded.searchable_columns, vec!["active", "weird"]);
        assert_eq!(expanded.skipped_columns, vec!["created"]);
    }

    #[test]
    fn unknown_and_blank_type_names_stay_in_the_text_set() {
        let expanded = expand_any_column_filter(
            &[
                column("weird", ""),
                column("dirty", "   "),
                column("aliased", "SOMETHING-NEW"),
            ],
            "x",
            &AnyColumnFilterOptions::default(),
        );
        assert_eq!(expanded.searchable_columns, vec!["weird", "dirty", "aliased"]);
        assert!(expanded.skipped_columns.is_empty());
    }

    #[test]
    fn duplicate_column_names_are_kept_in_schema_order() {
        let expanded = expand_any_column_filter(
            &[column("dup", "text"), column("dup", "int")],
            "x",
            &AnyColumnFilterOptions::default(),
        );
        assert_eq!(expanded.searchable_columns, vec!["dup"]);
        assert_eq!(expanded.skipped_columns, vec!["dup"]);
        assert_eq!(expanded.conditions, vec![contains("dup", "x", None)]);
    }

    // The scope ladder is the React expression verbatim: an empty group list
    // is text-only; number/datetime select their scopes; "non-empty without
    // number" falls through to text+datetime.
    #[test]
    fn scope_ladder_matches_the_react_expression() {
        assert_eq!(scope_for_groups(&[]), ValueSearchTypeScope::Text);
        assert_eq!(
            scope_for_groups(&[ValueSearchTypeGroup::Number]),
            ValueSearchTypeScope::TextNumber
        );
        assert_eq!(
            scope_for_groups(&[ValueSearchTypeGroup::Datetime]),
            ValueSearchTypeScope::TextDatetime
        );
        assert_eq!(
            scope_for_groups(&[
                ValueSearchTypeGroup::Number,
                ValueSearchTypeGroup::Datetime,
            ]),
            ValueSearchTypeScope::TextNumberDatetime
        );
    }

    #[test]
    fn the_react_surface_is_reexported_at_the_crate_root() {
        use crate::{
            expand_any_column_filter, AnyColumnExpansion, AnyColumnFilterOptions,
            ValueSearchTypeGroup,
        };
        let expansion: AnyColumnExpansion = expand_any_column_filter(
            &[column("n", "text")],
            "v",
            &AnyColumnFilterOptions {
                groups: vec![ValueSearchTypeGroup::Number],
                hints: None,
            },
        );
        assert_eq!(expansion.conditions.len(), 1);
    }
}
