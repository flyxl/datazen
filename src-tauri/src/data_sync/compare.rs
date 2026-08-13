//! Streaming PK merge-compare. Host orchestration; no dialect SQL here.

use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::Value;

use super::error::DataSyncError;
use super::model::{optional_values_equal, Row, RowChange, SyncOptions, TableResult};

#[async_trait]
pub trait RowPageSource: Send {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError>;
}

pub fn cmp_values(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Bytes(a), Value::Bytes(b)) => a.cmp(b),
        (Value::Timestamp(a), Value::Timestamp(b)) => a.cmp(b),
        (Value::Json(a), Value::Json(b)) => a.to_string().cmp(&b.to_string()),
        (a, b) => value_rank(a).cmp(&value_rank(b)).then_with(|| {
            format!("{a:?}").cmp(&format!("{b:?}"))
        }),
    }
}

fn value_rank(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Integer(_) => 2,
        Value::Float(_) => 3,
        Value::String(_) => 4,
        Value::Bytes(_) => 5,
        Value::Timestamp(_) => 6,
        Value::Json(_) => 7,
    }
}

pub fn cmp_keys(left: &[Value], right: &[Value]) -> Ordering {
    for (a, b) in left.iter().zip(right.iter()) {
        let ord = cmp_values(a, b);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    left.len().cmp(&right.len())
}

pub fn extract_key(row: &[Option<Value>], pk_indexes: &[usize]) -> Result<Vec<Value>, DataSyncError> {
    let mut key = Vec::with_capacity(pk_indexes.len());
    for &idx in pk_indexes {
        let cell = row.get(idx).ok_or_else(|| {
            DataSyncError::validation(format!("primary key index {idx} out of row bounds"))
        })?;
        key.push(cell.clone().unwrap_or(Value::Null));
    }
    Ok(key)
}

pub fn diff_changed_columns(
    source: &[Option<Value>],
    target: &[Option<Value>],
    column_names: &[String],
    pk_indexes: &[usize],
) -> Vec<String> {
    let mut changed = Vec::new();
    let len = column_names.len().min(source.len()).min(target.len());
    for i in 0..len {
        if pk_indexes.contains(&i) {
            continue;
        }
        if !optional_values_equal(&source[i], &target[i]) {
            changed.push(column_names[i].clone());
        }
    }
    changed
}

pub fn compare_sorted_rows(
    source_rows: &[Row],
    target_rows: &[Row],
    pk_indexes: &[usize],
    column_names: &[String],
    options: &SyncOptions,
) -> Result<Vec<RowChange>, DataSyncError> {
    let mut changes = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < source_rows.len() || j < target_rows.len() {
        if i >= source_rows.len() {
            let key = extract_key(&target_rows[j], pk_indexes)?;
            changes.push(RowChange::delete(key, target_rows[j].clone(), options));
            j += 1;
            continue;
        }
        if j >= target_rows.len() {
            let key = extract_key(&source_rows[i], pk_indexes)?;
            changes.push(RowChange::insert(key, source_rows[i].clone(), options));
            i += 1;
            continue;
        }
        let src_key = extract_key(&source_rows[i], pk_indexes)?;
        let tgt_key = extract_key(&target_rows[j], pk_indexes)?;
        match cmp_keys(&src_key, &tgt_key) {
            Ordering::Less => {
                changes.push(RowChange::insert(
                    src_key,
                    source_rows[i].clone(),
                    options,
                ));
                i += 1;
            }
            Ordering::Greater => {
                changes.push(RowChange::delete(
                    tgt_key,
                    target_rows[j].clone(),
                    options,
                ));
                j += 1;
            }
            Ordering::Equal => {
                let changed_columns = diff_changed_columns(
                    &source_rows[i],
                    &target_rows[j],
                    column_names,
                    pk_indexes,
                );
                if changed_columns.is_empty() {
                    changes.push(RowChange::unchanged(
                        src_key,
                        source_rows[i].clone(),
                        target_rows[j].clone(),
                    ));
                } else {
                    changes.push(RowChange::update(
                        src_key,
                        source_rows[i].clone(),
                        target_rows[j].clone(),
                        changed_columns,
                        options,
                    ));
                }
                i += 1;
                j += 1;
            }
        }
    }
    Ok(changes)
}

pub async fn compare_table_pages<S, T>(
    source_table: &str,
    target_table: &str,
    pk_indexes: &[usize],
    column_names: &[String],
    options: &SyncOptions,
    source: &mut S,
    target: &mut T,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<TableResult, DataSyncError>
where
    S: RowPageSource,
    T: RowPageSource,
{
    let mut src_page = source.next_page(None, options.batch_size).await?;
    let mut tgt_page = target.next_page(None, options.batch_size).await?;
    let mut i = 0usize;
    let mut j = 0usize;
    let mut changes = Vec::new();

    loop {
        if cancelled
            .as_ref()
            .is_some_and(|c| c.load(AtomicOrdering::SeqCst))
        {
            return Err(DataSyncError::cancelled("compare cancelled"));
        }
        if i >= src_page.len() && !src_page.is_empty() {
            let after = extract_key(src_page.last().unwrap(), pk_indexes)?;
            src_page = source.next_page(Some(&after), options.batch_size).await?;
            i = 0;
        }
        if j >= tgt_page.len() && !tgt_page.is_empty() {
            let after = extract_key(tgt_page.last().unwrap(), pk_indexes)?;
            tgt_page = target.next_page(Some(&after), options.batch_size).await?;
            j = 0;
        }
        if src_page.is_empty() && tgt_page.is_empty() {
            break;
        }
        if src_page.is_empty() {
            let key = extract_key(&tgt_page[j], pk_indexes)?;
            changes.push(RowChange::delete(key, tgt_page[j].clone(), options));
            j += 1;
            continue;
        }
        if tgt_page.is_empty() {
            let key = extract_key(&src_page[i], pk_indexes)?;
            changes.push(RowChange::insert(key, src_page[i].clone(), options));
            i += 1;
            continue;
        }
        let src_key = extract_key(&src_page[i], pk_indexes)?;
        let tgt_key = extract_key(&tgt_page[j], pk_indexes)?;
        match cmp_keys(&src_key, &tgt_key) {
            Ordering::Less => {
                changes.push(RowChange::insert(src_key, src_page[i].clone(), options));
                i += 1;
            }
            Ordering::Greater => {
                changes.push(RowChange::delete(tgt_key, tgt_page[j].clone(), options));
                j += 1;
            }
            Ordering::Equal => {
                let changed_columns =
                    diff_changed_columns(&src_page[i], &tgt_page[j], column_names, pk_indexes);
                if changed_columns.is_empty() {
                    changes.push(RowChange::unchanged(
                        src_key,
                        src_page[i].clone(),
                        tgt_page[j].clone(),
                    ));
                } else {
                    changes.push(RowChange::update(
                        src_key,
                        src_page[i].clone(),
                        tgt_page[j].clone(),
                        changed_columns,
                        options,
                    ));
                }
                i += 1;
                j += 1;
            }
        }
    }

    Ok(TableResult::matched(source_table, target_table, changes))
}

/// In-memory sorted page source for tests and small fixtures.
pub struct SliceRowSource {
    rows: Vec<Row>,
    pk_indexes: Vec<usize>,
}

impl SliceRowSource {
    pub fn new(mut rows: Vec<Row>, pk_indexes: Vec<usize>) -> Result<Self, DataSyncError> {
        rows.sort_by(|a, b| {
            let ka = extract_key(a, &pk_indexes).unwrap_or_default();
            let kb = extract_key(b, &pk_indexes).unwrap_or_default();
            cmp_keys(&ka, &kb)
        });
        Ok(Self { rows, pk_indexes })
    }
}

#[async_trait]
impl RowPageSource for SliceRowSource {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        let limit = limit.max(1) as usize;
        let start = match after_key {
            None => 0,
            Some(after) => {
                let mut idx = 0;
                while idx < self.rows.len() {
                    let key = extract_key(&self.rows[idx], &self.pk_indexes)?;
                    if cmp_keys(&key, after) == Ordering::Greater {
                        break;
                    }
                    idx += 1;
                }
                idx
            }
        };
        let end = (start + limit).min(self.rows.len());
        Ok(self.rows[start..end].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync::model::ChangeOperation;

    fn i(n: i64) -> Option<Value> {
        Some(Value::Integer(n))
    }
    fn s(v: &str) -> Option<Value> {
        Some(Value::String(v.into()))
    }

    fn cols() -> Vec<String> {
        vec!["id".into(), "name".into(), "age".into()]
    }

    #[test]
    fn merge_insert_update_delete_unchanged() {
        let opts = SyncOptions::default();
        let source = vec![
            vec![i(1), s("a"), i(10)],
            vec![i(2), s("b"), i(20)],
            vec![i(4), s("d"), i(40)],
        ];
        let target = vec![
            vec![i(1), s("a"), i(10)],
            vec![i(2), s("b"), i(99)],
            vec![i(3), s("c"), i(30)],
        ];
        let changes = compare_sorted_rows(&source, &target, &[0], &cols(), &opts).unwrap();
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0].operation, ChangeOperation::Unchanged);
        assert_eq!(changes[1].operation, ChangeOperation::Update);
        assert_eq!(changes[1].changed_columns, vec!["age".to_string()]);
        assert_eq!(changes[2].operation, ChangeOperation::Delete);
        assert!(!changes[2].selected);
        assert_eq!(changes[3].operation, ChangeOperation::Insert);
        assert!(changes[3].selected);
    }

    #[test]
    fn composite_pk_and_type_strictness() {
        let opts = SyncOptions::default();
        let source = vec![vec![i(1), s("east"), i(0)], vec![i(1), s("west"), i(1)]];
        let target = vec![vec![i(1), s("east"), i(0)], vec![i(1), s("west"), s("1")]];
        let cols = vec!["tenant".into(), "region".into(), "n".into()];
        let changes = compare_sorted_rows(&source, &target, &[0, 1], &cols, &opts).unwrap();
        assert_eq!(changes[0].operation, ChangeOperation::Unchanged);
        assert_eq!(changes[1].operation, ChangeOperation::Update);
        assert_eq!(changes[1].changed_columns, vec!["n".to_string()]);
    }

    #[test]
    fn null_not_equal_empty_string() {
        let opts = SyncOptions::default();
        let source = vec![vec![i(1), None]];
        let target = vec![vec![i(1), s("")]];
        let cols = vec!["id".into(), "note".into()];
        let changes = compare_sorted_rows(&source, &target, &[0], &cols, &opts).unwrap();
        assert_eq!(changes[0].operation, ChangeOperation::Update);
    }

    #[test]
    fn cmp_values_orders_and_cross_type_rank() {
        assert_eq!(cmp_values(&Value::Null, &Value::Integer(1)), Ordering::Less);
        assert_eq!(
            cmp_values(&Value::Integer(1), &Value::Integer(2)),
            Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::String("a".into()), &Value::String("b".into())),
            Ordering::Less
        );
        assert_eq!(
            cmp_keys(&[Value::Integer(1)], &[Value::Integer(1), Value::Integer(2)]),
            Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::Bool(false), &Value::Bool(true)),
            Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::Float(1.0), &Value::Float(2.0)),
            Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::Bytes(vec![1]), &Value::Bytes(vec![2])),
            Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::Timestamp("a".into()), &Value::Timestamp("b".into())),
            Ordering::Less
        );
        assert_ne!(
            cmp_values(
                &Value::Json(serde_json::json!({"a": 1})),
                &Value::Json(serde_json::json!({"b": 2}))
            ),
            Ordering::Equal
        );
        assert_eq!(
            cmp_values(&Value::Integer(1), &Value::String("1".into())),
            Ordering::Less
        );
    }

    #[tokio::test]
    async fn paged_source_matches_full_merge() {
        let opts = SyncOptions {
            batch_size: 2,
            ..SyncOptions::default()
        };
        let source_data = vec![
            vec![i(1), s("a")],
            vec![i(2), s("b")],
            vec![i(3), s("c")],
            vec![i(5), s("e")],
        ];
        let target_data = vec![
            vec![i(1), s("a")],
            vec![i(2), s("B")],
            vec![i(4), s("d")],
        ];
        let cols = vec!["id".into(), "name".into()];
        let mut src = SliceRowSource::new(source_data.clone(), vec![0]).unwrap();
        let mut tgt = SliceRowSource::new(target_data.clone(), vec![0]).unwrap();
        let table = compare_table_pages(
            "users",
            "clients",
            &[0],
            &cols,
            &opts,
            &mut src,
            &mut tgt,
            None,
        )
        .await
        .unwrap();
        assert_eq!(table.source_table, "users");
        assert_eq!(table.target_table, "clients");
        assert_eq!(table.insert_count(), 2); // 3 and 5
        assert_eq!(table.update_count(), 1);
        assert_eq!(table.delete_count(), 1);
        assert_eq!(table.unchanged_row_count(), 1);
    }

    #[tokio::test]
    async fn cancel_stops_compare() {
        let opts = SyncOptions::default();
        let mut src = SliceRowSource::new(vec![vec![i(1), s("a")]], vec![0]).unwrap();
        let mut tgt = SliceRowSource::new(vec![vec![i(1), s("a")]], vec![0]).unwrap();
        let flag = Arc::new(AtomicBool::new(true));
        let err = compare_table_pages(
            "t",
            "t",
            &[0],
            &["id".into(), "n".into()],
            &opts,
            &mut src,
            &mut tgt,
            Some(flag),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DataSyncError::Cancelled(_)));
    }

    #[test]
    fn pk_index_out_of_bounds() {
        let opts = SyncOptions::default();
        let err = compare_sorted_rows(
            &[vec![i(1)]],
            &[vec![i(1)]],
            &[3],
            &["id".into()],
            &opts,
        )
        .unwrap_err();
        assert!(err.to_string().contains("out of row bounds"));
    }

    #[tokio::test]
    async fn empty_tables_are_matched_with_no_rows() {
        let opts = SyncOptions::default();
        let mut src = SliceRowSource::new(vec![], vec![0]).unwrap();
        let mut tgt = SliceRowSource::new(vec![], vec![0]).unwrap();
        let table = compare_table_pages(
            "t",
            "t",
            &[0],
            &["id".into()],
            &opts,
            &mut src,
            &mut tgt,
            None,
        )
        .await
        .unwrap();
        assert!(!table.has_row_differences());
        assert!(table.rows.is_empty());
    }
}
