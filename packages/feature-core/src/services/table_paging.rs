//! Whether a page can be read by seek, and where its boundary comes from.
//!
//! Keyset pagination replaces the `OFFSET` of a page with a **value
//! boundary** — the key values of the last row of the previous page. The
//! trade is that the boundary only means something when the order it is
//! taken in is a **total order**: two rows must never compare equal, or the
//! engine is free to return them in either order and the boundary stops
//! being a fact.
//!
//! So the order a read uses has to be decided **once**, here, and the
//! decision has to be a pure function — it is the part of the paging
//! contract that is easiest to get wrong and the part that is hardest to
//! test against a live database.
//!
//! # The rules
//!
//! 1. The order is the one that is written into the `ORDER BY`. It is not
//!    computed twice: [`OrderSpec`] is the single description of it, and
//!    everything downstream — the SQL, the boundary, the answer to the caller
//!    — reads it from there.
//! 2. A table without a key has no total order. The first column is *not* a
//!    substitute: two rows may share it, and then the boundary is a guess.
//! 3. When the order does not already contain the whole key, the missing key
//!    columns are **appended** (ascending). "The key is in the order" is what
//!    makes the tuple comparison a total order; assuming it is there is not
//!    enough.
//! 4. A column that can be `NULL` makes the order unusable for a seek:
//!    `NULL` does not take part in comparisons, so a seek across the `NULL`
//!    block would silently drop rows. There is no `IS NULL` branch in this
//!    version of the contract — it would need to know where the dialect puts
//!    `NULL`s, and no method says that.
//!
//! Everything here is a pure function over plain data: no driver, no
//! connection, no statement.

use datazen_driver_api::keyset::{CursorValue, KeysetUnavailableReason, OrderColumn};
use datazen_driver_api::{ColumnSchema, Value};

/// Where the order of a read came from.
///
/// The source is not decoration: "the user asked for this column" and "the
/// key was injected" are different promises, and the degradation reasons a
/// caller sees are phrased in terms of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSource {
    /// The user picked the column.
    ExplicitSort,
    /// The driver declared a default order.
    DriverDefault,
    /// The key was injected because the caller asked for no order.
    PrimaryKeyInjection,
    /// The table has no key, so the first column was used. **Not a total
    /// order** — two rows may share it.
    FirstColumnFallback,
    /// No order at all.
    None,
}

/// The order one read actually uses.
///
/// This is *the* description of the `ORDER BY` of a read. Nothing downstream
/// may compute an order of its own: a second computation is a second truth,
/// and the two drift the moment the rules change.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderSpec {
    /// The columns of the `ORDER BY`, in order, with their directions.
    pub columns: Vec<OrderColumn>,
    /// Where the order came from.
    pub source: OrderSource,
    /// The key of the table — the columns that identify a row.
    ///
    /// Today this is the primary key. It is the **only** thing that can make
    /// an order total, because it is the only thing that is unique.
    pub unique_key: Vec<String>,
}

/// What the host decided about reading this page by seek.
#[derive(Debug, Clone, PartialEq)]
pub struct KeysetDecision {
    /// The order the **statement** is written in.
    ///
    /// The tie-breaker columns that make an order total are **not** in
    /// here: they are a property of the seek, not of the read. A page that
    /// is not read by seek is written in the order the caller asked for —
    /// byte for byte the statement the read always sent. This is the whole
    /// point of the "a driver that did not opt in reads exactly as before"
    /// promise, and it is why the two orders are two fields and not one.
    pub order: OrderSpec,
    /// Whether the page can be read by seek, and in which total order.
    pub seek: KeysetSeek,
}

/// The seek half of a decision.
#[derive(Debug, Clone, PartialEq)]
pub enum KeysetSeek {
    /// The page can be read by seek, in this total order — the read order
    /// with the key columns appended.
    Eligible { order: OrderSpec },
    /// It cannot, for this reason.
    Unavailable { reason: KeysetUnavailableReason },
}

/// The order of a read, assembled from what the caller asked and what the
/// table's key is.
///
/// `order_by` is the user's explicit sort (at most one column — the browsing
/// surface sorts by one column), `default_order` is the order the driver
/// declared for a read without a sort, and `primary_keys` is the table's
/// key.
///
/// The three are mutually exclusive by construction: an explicit sort wins,
/// and the default order is only consulted when there is none.
pub fn resolve_order_spec(
    order_by: Option<(&str, bool)>,
    default_order: &[String],
    primary_keys: &[String],
) -> OrderSpec {
    if let Some((column, descending)) = order_by {
        return OrderSpec {
            columns: vec![OrderColumn {
                column: column.to_string(),
                descending,
            }],
            source: OrderSource::ExplicitSort,
            unique_key: primary_keys.to_vec(),
        };
    }

    if default_order.is_empty() {
        return OrderSpec {
            columns: Vec::new(),
            source: OrderSource::None,
            unique_key: primary_keys.to_vec(),
        };
    }

    let source = if !primary_keys.is_empty() && default_order == primary_keys {
        OrderSource::PrimaryKeyInjection
    } else if primary_keys.is_empty() && default_order.len() == 1 {
        OrderSource::FirstColumnFallback
    } else {
        OrderSource::DriverDefault
    };

    OrderSpec {
        columns: default_order
            .iter()
            .map(|column| OrderColumn {
                column: column.clone(),
                descending: false,
            })
            .collect(),
        source,
        unique_key: primary_keys.to_vec(),
    }
}

/// Decide whether the order in force is a total order, and make it one if
/// it can be made one.
///
/// The decision, in order:
///
/// 1. a table without a key has no total order — [`KeysetUnavailableReason::NoStableOrder`];
/// 2. the key columns that are missing from the order are **appended**,
///    ascending, in the order the key declares them;
/// 3. a column of the resulting order that can be `NULL` rules the page out
///    — [`KeysetUnavailableReason::NullableKeyColumn`].
///
/// A column whose nullability cannot be established (it is not in the
/// column list at all) counts as nullable: the rule is about what the
/// database can hold, and "unknown" is not "safe".
pub fn keyset_decision(order: OrderSpec, columns: &[ColumnSchema]) -> KeysetDecision {
    if order.unique_key.is_empty() {
        return KeysetDecision {
            order,
            seek: KeysetSeek::Unavailable {
                reason: KeysetUnavailableReason::NoStableOrder,
            },
        };
    }

    let mut columns_in_order = order.columns.clone();
    for key in &order.unique_key {
        if columns_in_order.iter().any(|column| &column.column == key) {
            continue;
        }
        // The appended key column is always ascending: it is a tie-breaker,
        // not a second opinion about the user's sort.
        columns_in_order.push(OrderColumn {
            column: key.clone(),
            descending: false,
        });
    }

    let nullable = columns_in_order.iter().any(|column| {
        columns
            .iter()
            .find(|schema| schema.name == column.column)
            .map(|schema| schema.nullable)
            .unwrap_or(true)
    });
    if nullable {
        // The order that is reported here is the one that is **used**: the
        // page falls back to `OFFSET`, and the statement must be the
        // statement the table read always sent. The appended key columns
        // are a property of the seek, not of the read.
        return KeysetDecision {
            order,
            seek: KeysetSeek::Unavailable {
                reason: KeysetUnavailableReason::NullableKeyColumn,
            },
        };
    }

    // The order the statement is written in is the one that was asked for;
    // the appended key columns belong to the seek and are carried by the
    // seek's own order.
    let read_order = order.clone();
    KeysetDecision {
        order: read_order,
        seek: KeysetSeek::Eligible {
            order: OrderSpec {
                columns: columns_in_order,
                ..order
            },
        },
    }
}

/// The key values of a row, in the order of the order columns.
///
/// This is the boundary of the **next** page: the values are taken from a row
/// that was really read, never computed from a page number.
///
/// `None` means the row cannot serve as a boundary:
///
/// * the row is shorter than the order expects;
/// * a key column is not in the column list, so its position is unknown;
/// * a key value is `NULL` — `NULL` does not take part in comparisons, so a
///   condition built from it would skip rows without saying so.
pub fn cursor_from_row(
    row: &[Option<Value>],
    columns: &[ColumnSchema],
    order_columns: &[OrderColumn],
) -> Option<Vec<CursorValue>> {
    let mut boundary = Vec::with_capacity(order_columns.len());
    for order_column in order_columns {
        let index = columns
            .iter()
            .position(|schema| schema.name == order_column.column)?;
        let cell = row.get(index)?;
        let value = cell.as_ref()?;
        if matches!(value, Value::Null) {
            return None;
        }
        boundary.push(CursorValue::from_value(value));
    }
    Some(boundary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, nullable: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    fn key_column(name: &str) -> ColumnSchema {
        ColumnSchema {
            is_primary_key: true,
            ..column(name, false)
        }
    }

    #[test]
    fn keyset_decision_uses_primary_keys_as_stable_total_order() {
        let order = OrderSpec {
            columns: Vec::new(),
            source: OrderSource::PrimaryKeyInjection,
            unique_key: vec!["id".to_string()],
        };
        let decision = keyset_decision(order, &[key_column("id")]);
        match decision.seek {
            KeysetSeek::Eligible { order } => {
                assert_eq!(order.columns.len(), 1);
                assert_eq!(order.columns[0].column, "id");
                assert!(!order.columns[0].descending);
            }
            other => panic!("expected the key to make the order total, got {other:?}"),
        }
    }

    #[test]
    fn keyset_decision_appends_missing_primary_key_columns() {
        let order = OrderSpec {
            columns: vec![OrderColumn {
                column: "created_at".to_string(),
                descending: true,
            }],
            source: OrderSource::ExplicitSort,
            unique_key: vec!["id".to_string()],
        };
        let decision = keyset_decision(order, &[key_column("id"), column("created_at", false)]);
        match decision.seek {
            KeysetSeek::Eligible { order } => {
                assert_eq!(order.columns.len(), 2);
                assert_eq!(order.columns[0].column, "created_at");
                assert!(order.columns[0].descending, "the user's direction is kept");
                assert_eq!(order.columns[1].column, "id");
                assert!(
                    !order.columns[1].descending,
                    "the appended key column is a tie-breaker and is ascending"
                );
            }
            other => panic!("expected the key to be appended, got {other:?}"),
        }
    }

    #[test]
    fn keyset_decision_rejects_table_without_primary_key() {
        let order = OrderSpec {
            columns: vec![OrderColumn {
                column: "name".to_string(),
                descending: false,
            }],
            source: OrderSource::FirstColumnFallback,
            unique_key: Vec::new(),
        };
        let decision = keyset_decision(order, &[column("name", false)]);
        match decision.seek {
            KeysetSeek::Unavailable { reason } => {
                assert_eq!(reason, KeysetUnavailableReason::NoStableOrder);
            }
            other => panic!("expected the first column not to be a total order, got {other:?}"),
        }
    }

    #[test]
    fn keyset_decision_rejects_nullable_sort_column() {
        let order = OrderSpec {
            columns: vec![OrderColumn {
                column: "created_at".to_string(),
                descending: false,
            }],
            source: OrderSource::ExplicitSort,
            unique_key: vec!["id".to_string()],
        };
        let decision = keyset_decision(order, &[key_column("id"), column("created_at", true)]);
        match decision.seek {
            KeysetSeek::Unavailable { reason } => {
                assert_eq!(reason, KeysetUnavailableReason::NullableKeyColumn);
            }
            other => panic!("expected a nullable column to rule the page out, got {other:?}"),
        }
    }

    #[test]
    fn keyset_decision_composite_primary_key_all_columns_appended() {
        let order = OrderSpec {
            columns: vec![OrderColumn {
                column: "quantity".to_string(),
                descending: false,
            }],
            source: OrderSource::ExplicitSort,
            unique_key: vec!["order_id".to_string(), "product_id".to_string()],
        };
        let decision = keyset_decision(
            order,
            &[
                key_column("order_id"),
                key_column("product_id"),
                column("quantity", false),
            ],
        );
        match decision.seek {
            KeysetSeek::Eligible { order } => {
                let names: Vec<&str> = order
                    .columns
                    .iter()
                    .map(|column| column.column.as_str())
                    .collect();
                assert_eq!(names, vec!["quantity", "order_id", "product_id"]);
            }
            other => panic!("expected the whole key to be appended, got {other:?}"),
        }
    }

    #[test]
    fn cursor_from_row_reads_last_row_in_order_column_order() {
        let columns = vec![column("created_at", false), column("id", false)];
        let order = vec![
            OrderColumn {
                column: "created_at".to_string(),
                descending: true,
            },
            OrderColumn {
                column: "id".to_string(),
                descending: false,
            },
        ];
        let row = vec![Some(Value::Integer(7)), Some(Value::Integer(11))];
        let boundary = cursor_from_row(&row, &columns, &order).expect("a boundary");
        assert_eq!(boundary.len(), 2);
        assert_eq!(boundary[0], CursorValue::Integer(7));
        assert_eq!(boundary[1], CursorValue::Integer(11));
    }

    #[test]
    fn cursor_from_row_refuses_a_null_boundary() {
        let columns = vec![column("id", false)];
        let order = vec![OrderColumn {
            column: "id".to_string(),
            descending: false,
        }];
        assert!(
            cursor_from_row(&[None], &columns, &order).is_none(),
            "a NULL key value cannot locate the next page"
        );
        assert!(
            cursor_from_row(&[Some(Value::Null)], &columns, &order).is_none(),
            "a NULL key value cannot locate the next page"
        );
    }
}
