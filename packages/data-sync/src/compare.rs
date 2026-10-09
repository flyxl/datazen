//! Streaming PK merge-compare. Host orchestration; no dialect SQL here.

use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::{SyncKeyValue, Value};

use super::error::DataSyncError;
use super::model::{optional_values_equal, Row, RowChange, SyncOptions, TableResult};

#[async_trait]
pub trait RowPageSource: Send {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError>;

    /// Normalize a raw primary-key tuple using the driver's equality/order
    /// contract.  The raw tuple remains available for seek parameters and
    /// writes; only comparison uses this canonical representation.
    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError>;
}

/// Receives only the rows that differ while the two keyset streams are
/// merged.  Implementations may persist each change immediately, keeping the
/// merge state bounded to the two driver pages and the current row.
#[async_trait]
pub trait RowChangeSink: Send {
    async fn push(&mut self, change: RowChange) -> Result<(), DataSyncError>;

    async fn unchanged(&mut self) -> Result<(), DataSyncError>;
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
        (a, b) => value_rank(a)
            .cmp(&value_rank(b))
            .then_with(|| format!("{a:?}").cmp(&format!("{b:?}"))),
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

pub fn extract_key(
    row: &[Option<Value>],
    pk_indexes: &[usize],
) -> Result<Vec<Value>, DataSyncError> {
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
                changes.push(RowChange::insert(src_key, source_rows[i].clone(), options));
                i += 1;
            }
            Ordering::Greater => {
                changes.push(RowChange::delete(tgt_key, target_rows[j].clone(), options));
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

// Fail closed before exposing partial comparisons. Driver ordering must match the merge order.
fn validate_page_with_source<S: RowPageSource>(
    source: &S,
    page: &[Row],
    pk_indexes: &[usize],
    columns: &[String],
    after: Option<&[Value]>,
) -> Result<(), DataSyncError> {
    if page.len() > 1000
        || serde_json::to_vec(page)
            .map_err(|e| DataSyncError::validation(e.to_string()))?
            .len()
            > 8 * 1024 * 1024
    {
        return Err(DataSyncError::validation(
            "comparison page exceeds 1000 rows / 8 MiB; reduce batch size or large column values",
        ));
    }
    let mut previous = after.map(|raw| source.normalize_key(raw)).transpose()?;
    for row in page {
        if row.len() != columns.len() {
            return Err(DataSyncError::validation("row width differs from canonical projection; compare again after checking the driver"));
        }
        let raw_key = extract_key(row, pk_indexes)?;
        if raw_key.is_empty() {
            return Err(DataSyncError::validation(
                "comparison requires primary key columns",
            ));
        }
        if raw_key.iter().any(|value| matches!(value, Value::Null)) {
            return Err(DataSyncError::validation(
                "comparison requires non-null primary keys",
            ));
        }
        let key = source.normalize_key(&raw_key)?;
        if previous
            .as_ref()
            .is_some_and(|p| key.cmp(p) != Ordering::Greater)
        {
            return Err(DataSyncError::validation("normalized primary key stream is not strictly increasing; check key collation, duplicate keys and driver ordering, then compare again"));
        }
        previous = Some(key);
    }
    Ok(())
}

struct RawPageSource;

#[async_trait]
impl RowPageSource for RawPageSource {
    async fn next_page(&mut self, _: Option<&[Value]>, _: u32) -> Result<Vec<Row>, DataSyncError> {
        Err(DataSyncError::validation(
            "raw page source cannot fetch pages",
        ))
    }

    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
        key.iter()
            .map(|value| SyncKeyValue::from_value(value).map_err(DataSyncError::validation))
            .collect()
    }
}

fn validate_page(
    page: &[Row],
    pk_indexes: &[usize],
    columns: &[String],
    after: Option<&[Value]>,
) -> Result<(), DataSyncError> {
    validate_page_with_source(&RawPageSource, page, pk_indexes, columns, after)
}

const MAX_DIFF_ROWS: usize = 10_000;
const MAX_RESULT_BYTES: usize = 32 * 1024 * 1024;

struct VecRowChangeSink {
    rows: Vec<RowChange>,
}

#[async_trait]
impl RowChangeSink for VecRowChangeSink {
    async fn push(&mut self, change: RowChange) -> Result<(), DataSyncError> {
        self.rows.push(change);
        Ok(())
    }

    async fn unchanged(&mut self) -> Result<(), DataSyncError> {
        Ok(())
    }
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
    let mut sink = VecRowChangeSink { rows: Vec::new() };
    let mut result = compare_table_pages_into(
        source_table,
        target_table,
        pk_indexes,
        column_names,
        options,
        source,
        target,
        cancelled,
        &mut sink,
        Some((MAX_DIFF_ROWS, MAX_RESULT_BYTES)),
    )
    .await?;
    result.rows = sink.rows;
    Ok(result)
}

/// Compare two ordered keyset streams and send each difference to `sink` as
/// soon as it is found.  The sink path deliberately has no aggregate result
/// limit: the durable comparison index owns the change rows and only the two
/// bounded driver pages remain in memory.
pub async fn compare_table_pages_to_sink<S, T, K>(
    source_table: &str,
    target_table: &str,
    pk_indexes: &[usize],
    column_names: &[String],
    options: &SyncOptions,
    source: &mut S,
    target: &mut T,
    cancelled: Option<Arc<AtomicBool>>,
    sink: &mut K,
) -> Result<TableResult, DataSyncError>
where
    S: RowPageSource,
    T: RowPageSource,
    K: RowChangeSink,
{
    compare_table_pages_into(
        source_table,
        target_table,
        pk_indexes,
        column_names,
        options,
        source,
        target,
        cancelled,
        sink,
        None,
    )
    .await
}

async fn compare_table_pages_into<S, T, K>(
    source_table: &str,
    target_table: &str,
    pk_indexes: &[usize],
    column_names: &[String],
    options: &SyncOptions,
    source: &mut S,
    target: &mut T,
    cancelled: Option<Arc<AtomicBool>>,
    sink: &mut K,
    result_limits: Option<(usize, usize)>,
) -> Result<TableResult, DataSyncError>
where
    S: RowPageSource,
    T: RowPageSource,
    K: RowChangeSink,
{
    options.validate()?;
    let mut src_page = source.next_page(None, options.batch_size).await?;
    let mut tgt_page = target.next_page(None, options.batch_size).await?;
    validate_page_with_source(source, &src_page, pk_indexes, column_names, None)?;
    validate_page_with_source(target, &tgt_page, pk_indexes, column_names, None)?;
    let mut unchanged_count = 0;
    let mut result_bytes = 0;
    let mut i = 0usize;
    let mut j = 0usize;
    let mut change_count = 0usize;

    loop {
        if let Some((max_rows, max_bytes)) = result_limits {
            if change_count > max_rows || result_bytes > max_bytes {
                return Err(DataSyncError::validation("comparison exceeds the current 10,000 difference / 32 MiB review limit; select fewer tables or a smaller dataset"));
            }
        }
        if cancelled
            .as_ref()
            .is_some_and(|c| c.load(AtomicOrdering::SeqCst))
        {
            return Err(DataSyncError::cancelled("compare cancelled"));
        }
        if i >= src_page.len() && !src_page.is_empty() {
            let after = extract_key(&src_page[src_page.len() - 1], pk_indexes)?;
            src_page = source.next_page(Some(&after), options.batch_size).await?;
            validate_page_with_source(source, &src_page, pk_indexes, column_names, Some(&after))?;
            i = 0;
        }
        if j >= tgt_page.len() && !tgt_page.is_empty() {
            let after = extract_key(&tgt_page[tgt_page.len() - 1], pk_indexes)?;
            tgt_page = target.next_page(Some(&after), options.batch_size).await?;
            validate_page_with_source(target, &tgt_page, pk_indexes, column_names, Some(&after))?;
            j = 0;
        }
        if src_page.is_empty() && tgt_page.is_empty() {
            break;
        }
        if src_page.is_empty() {
            let key = extract_key(&tgt_page[j], pk_indexes)?;
            let change = RowChange::delete(key, tgt_page[j].clone(), options);
            result_bytes = result_bytes.saturating_add(
                serde_json::to_vec(&change)
                    .map_err(|e| DataSyncError::validation(e.to_string()))?
                    .len(),
            );
            change_count = change_count
                .checked_add(1)
                .ok_or_else(|| DataSyncError::validation("comparison change count overflowed"))?;
            sink.push(change).await?;
            j += 1;
            continue;
        }
        if tgt_page.is_empty() {
            let key = extract_key(&src_page[i], pk_indexes)?;
            let change = RowChange::insert(key, src_page[i].clone(), options);
            result_bytes = result_bytes.saturating_add(
                serde_json::to_vec(&change)
                    .map_err(|e| DataSyncError::validation(e.to_string()))?
                    .len(),
            );
            change_count = change_count
                .checked_add(1)
                .ok_or_else(|| DataSyncError::validation("comparison change count overflowed"))?;
            sink.push(change).await?;
            i += 1;
            continue;
        }
        let src_raw_key = extract_key(&src_page[i], pk_indexes)?;
        let tgt_raw_key = extract_key(&tgt_page[j], pk_indexes)?;
        let src_key = source.normalize_key(&src_raw_key)?;
        let tgt_key = target.normalize_key(&tgt_raw_key)?;
        if src_key.len() != tgt_key.len()
            || src_key
                .iter()
                .zip(&tgt_key)
                .any(|(left, right)| std::mem::discriminant(left) != std::mem::discriminant(right))
        {
            return Err(DataSyncError::validation(
                "source and target normalized key contracts differ; compare again after checking driver capabilities and key types",
            ));
        }
        match src_key.cmp(&tgt_key) {
            Ordering::Less => {
                let change = RowChange::insert(src_raw_key, src_page[i].clone(), options);
                result_bytes = result_bytes.saturating_add(
                    serde_json::to_vec(&change)
                        .map_err(|e| DataSyncError::validation(e.to_string()))?
                        .len(),
                );
                change_count = change_count.checked_add(1).ok_or_else(|| {
                    DataSyncError::validation("comparison change count overflowed")
                })?;
                sink.push(change).await?;
                i += 1;
            }
            Ordering::Greater => {
                let change = RowChange::delete(tgt_raw_key, tgt_page[j].clone(), options);
                result_bytes = result_bytes.saturating_add(
                    serde_json::to_vec(&change)
                        .map_err(|e| DataSyncError::validation(e.to_string()))?
                        .len(),
                );
                change_count = change_count.checked_add(1).ok_or_else(|| {
                    DataSyncError::validation("comparison change count overflowed")
                })?;
                sink.push(change).await?;
                j += 1;
            }
            Ordering::Equal => {
                let changed_columns =
                    diff_changed_columns(&src_page[i], &tgt_page[j], column_names, pk_indexes);
                if changed_columns.is_empty() {
                    unchanged_count += 1;
                    sink.unchanged().await?;
                } else {
                    let change = RowChange::update(
                        src_raw_key,
                        src_page[i].clone(),
                        tgt_page[j].clone(),
                        changed_columns,
                        options,
                    );
                    result_bytes = result_bytes.saturating_add(
                        serde_json::to_vec(&change)
                            .map_err(|e| DataSyncError::validation(e.to_string()))?
                            .len(),
                    );
                    change_count = change_count.checked_add(1).ok_or_else(|| {
                        DataSyncError::validation("comparison change count overflowed")
                    })?;
                    sink.push(change).await?;
                }
                i += 1;
                j += 1;
            }
        }
    }

    let mut result = TableResult::matched(source_table, target_table, Vec::new());
    result.columns = column_names.to_vec();
    result.primary_keys = pk_indexes
        .iter()
        .map(|&i| column_names[i].clone())
        .collect();
    result.unchanged_count = unchanged_count;
    Ok(result)
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

    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
        key.iter()
            .map(|value| SyncKeyValue::from_value(value).map_err(DataSyncError::validation))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChangeOperation;

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
            cmp_keys(
                &[Value::Integer(1)],
                &[Value::Integer(1), Value::Integer(2)]
            ),
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
        let target_data = vec![vec![i(1), s("a")], vec![i(2), s("B")], vec![i(4), s("d")]];
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
        let err = compare_sorted_rows(&[vec![i(1)]], &[vec![i(1)]], &[3], &["id".into()], &opts)
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
    #[tokio::test]
    async fn unchanged_rows_are_counted_without_retaining_values() {
        let rows = (0..5000)
            .map(|n| vec![i(n), s("unchanged")])
            .collect::<Vec<_>>();
        let mut src = SliceRowSource::new(rows.clone(), vec![0]).unwrap();
        let mut tgt = SliceRowSource::new(rows, vec![0]).unwrap();
        let table = compare_table_pages(
            "t",
            "t",
            &[0],
            &["id".into(), "name".into()],
            &SyncOptions::default(),
            &mut src,
            &mut tgt,
            None,
        )
        .await
        .unwrap();
        assert_eq!(table.unchanged_row_count(), 5000);
        assert!(table.rows.is_empty());
        assert_eq!(table.columns, vec!["id", "name"]);
    }

    #[tokio::test]
    async fn difference_limit_returns_error_instead_of_partial_approval() {
        let rows = (0..=MAX_DIFF_ROWS as i64).map(|n| vec![i(n)]).collect();
        let mut src = SliceRowSource::new(rows, vec![0]).unwrap();
        let mut tgt = SliceRowSource::new(vec![], vec![0]).unwrap();
        let err = compare_table_pages(
            "t",
            "t",
            &[0],
            &["id".into()],
            &SyncOptions::default(),
            &mut src,
            &mut tgt,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("review limit"));
    }

    #[test]
    fn malformed_and_non_monotone_pages_are_rejected() {
        let columns = vec!["id".into()];
        assert!(validate_page(&[vec![i(1), i(2)]], &[0], &columns, None).is_err());
        assert!(validate_page(&[vec![None]], &[0], &columns, None).is_err());
        assert!(validate_page(&[vec![i(1)], vec![i(1)]], &[0], &columns, None).is_err());
        assert!(validate_page(&[vec![i(2)], vec![i(1)]], &[0], &columns, None).is_err());
        assert!(validate_page(
            &[vec![s("B")]],
            &[0],
            &columns,
            Some(&[Value::String("a".into())])
        )
        .is_err());
    }

    struct RepeatedPage;
    #[async_trait]
    impl RowPageSource for RepeatedPage {
        async fn next_page(
            &mut self,
            _: Option<&[Value]>,
            _: u32,
        ) -> Result<Vec<Row>, DataSyncError> {
            Ok(vec![vec![i(1)]])
        }

        fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
            key.iter()
                .map(|value| SyncKeyValue::from_value(value).map_err(DataSyncError::validation))
                .collect()
        }
    }

    struct ContractRows {
        rows: Vec<Row>,
        contracts: Vec<datazen_driver_api::SyncKeyContract>,
        served: bool,
    }

    #[async_trait]
    impl RowPageSource for ContractRows {
        async fn next_page(
            &mut self,
            _: Option<&[Value]>,
            _: u32,
        ) -> Result<Vec<Row>, DataSyncError> {
            if self.served {
                Ok(Vec::new())
            } else {
                self.served = true;
                Ok(self.rows.clone())
            }
        }

        fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
            if key.len() != self.contracts.len() {
                return Err(DataSyncError::validation("test key arity mismatch"));
            }
            key.iter()
                .zip(&self.contracts)
                .map(|(value, contract)| {
                    contract
                        .normalize(&Some(value.clone()))
                        .map_err(DataSyncError::validation)
                })
                .collect()
        }
    }

    fn contract_rows(
        rows: Vec<Row>,
        contracts: Vec<datazen_driver_api::SyncKeyContract>,
    ) -> ContractRows {
        ContractRows {
            rows,
            contracts,
            served: false,
        }
    }

    #[tokio::test]
    async fn normalized_text_decimal_timestamp_and_composite_keys_compare() {
        use datazen_driver_api::{SyncKeyCollation, SyncKeyContract, SyncKeyKind};
        let contracts = vec![
            SyncKeyContract::reject_nulls(SyncKeyKind::Text {
                collation: SyncKeyCollation::Binary,
            }),
            SyncKeyContract::reject_nulls(SyncKeyKind::Decimal { scale: None }),
            SyncKeyContract::reject_nulls(SyncKeyKind::Timestamp {
                with_timezone: true,
                precision: 6,
            }),
        ];
        let source_row = vec![
            s("tenant"),
            Some(Value::String("1.20".into())),
            Some(Value::String("2026-01-01T00:00:00+08:00".into())),
            s("source"),
        ];
        let target_row = vec![
            s("tenant"),
            Some(Value::String("1.2".into())),
            Some(Value::String("2025-12-31T16:00:00Z".into())),
            s("target"),
        ];
        let mut source = contract_rows(vec![source_row], contracts.clone());
        let mut target = contract_rows(vec![target_row], contracts);
        let result = compare_table_pages(
            "source",
            "target",
            &[0, 1, 2],
            &[
                "tenant".into(),
                "amount".into(),
                "created".into(),
                "value".into(),
            ],
            &SyncOptions::default(),
            &mut source,
            &mut target,
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].operation, ChangeOperation::Update);
        assert!(matches!(
            result.rows[0].key[0],
            Value::String(ref value) if value == "tenant"
        ));
    }

    #[tokio::test]
    async fn duplicate_normalized_keys_fail_closed_before_changes() {
        use datazen_driver_api::{SyncKeyCollation, SyncKeyContract, SyncKeyKind};
        let contract = SyncKeyContract::reject_nulls(SyncKeyKind::Text {
            collation: SyncKeyCollation::Binary,
        });
        let mut source = contract_rows(
            vec![vec![s("a"), s("one")], vec![s("a"), s("two")]],
            vec![contract.clone()],
        );
        let mut target = contract_rows(Vec::new(), vec![contract]);
        let err = compare_table_pages(
            "source",
            "target",
            &[0],
            &["id".into(), "value".into()],
            &SyncOptions::default(),
            &mut source,
            &mut target,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("strictly increasing"));
    }

    #[tokio::test]
    async fn source_and_target_key_domains_cannot_be_coerced() {
        use datazen_driver_api::{SyncKeyContract, SyncKeyKind};
        let integer = SyncKeyContract::reject_nulls(SyncKeyKind::Integer { unsigned: false });
        let text = SyncKeyContract::reject_nulls(SyncKeyKind::Text {
            collation: datazen_driver_api::SyncKeyCollation::Binary,
        });
        let mut source = contract_rows(vec![vec![i(1)]], vec![integer]);
        let mut target = contract_rows(vec![vec![s("1")]], vec![text]);
        let err = compare_table_pages(
            "source",
            "target",
            &[0],
            &["id".into()],
            &SyncOptions::default(),
            &mut source,
            &mut target,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("contracts differ"));
    }

    #[tokio::test]
    async fn repeated_page_is_not_silently_treated_as_eof() {
        let mut src = RepeatedPage;
        let mut tgt = SliceRowSource::new(vec![], vec![0]).unwrap();
        let err = compare_table_pages(
            "t",
            "t",
            &[0],
            &["id".into()],
            &SyncOptions::default(),
            &mut src,
            &mut tgt,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("not strictly increasing"));
    }
    #[tokio::test]
    async fn test_tester_composite_integer_keys_across_single_row_pages() {
        let rows = vec![
            vec![i(-1), i(99), s("a")],
            vec![i(0), i(-1), s("b")],
            vec![i(0), i(0), s("c")],
        ];
        let mut source = SliceRowSource::new(rows.clone(), vec![0, 1]).unwrap();
        let mut target =
            SliceRowSource::new(vec![rows[0].clone(), rows[2].clone()], vec![0, 1]).unwrap();
        let options = SyncOptions {
            batch_size: 1,
            ..SyncOptions::default()
        };
        let result = compare_table_pages(
            "source",
            "target",
            &[0, 1],
            &["k1".into(), "k2".into(), "value".into()],
            &options,
            &mut source,
            &mut target,
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.unchanged_count, 2);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            serde_json::to_value(&result.rows[0].key).unwrap(),
            serde_json::json!([0, -1])
        );
        assert_eq!(result.primary_keys, vec!["k1", "k2"]);
    }

    #[tokio::test]
    async fn sink_path_accepts_large_change_sets_without_review_buffer_limit() {
        #[derive(Default)]
        struct CountingSink {
            changes: usize,
        }

        #[async_trait::async_trait]
        impl RowChangeSink for CountingSink {
            async fn push(&mut self, _change: RowChange) -> Result<(), DataSyncError> {
                self.changes += 1;
                Ok(())
            }

            async fn unchanged(&mut self) -> Result<(), DataSyncError> {
                Ok(())
            }
        }

        let rows = (0..10_001)
            .map(|key| vec![Some(Value::Integer(key))])
            .collect();
        let mut source = SliceRowSource::new(rows, vec![0]).unwrap();
        let mut target = SliceRowSource::new(Vec::new(), vec![0]).unwrap();
        let mut sink = CountingSink::default();
        let result = compare_table_pages_to_sink(
            "users",
            "users",
            &[0],
            &["id".into()],
            &SyncOptions::default(),
            &mut source,
            &mut target,
            None,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.changes, 10_001);
        assert_eq!(result.unchanged_count, 0);
        assert!(result.rows.is_empty());
    }

    #[test]
    fn test_tester_review_page_byte_limit_and_explicit_null_are_rejected() {
        let columns = vec!["id".into(), "payload".into()];
        let oversized = vec![vec![i(1), Some(Value::String("x".repeat(8 * 1024 * 1024)))]];
        assert!(validate_page(&oversized, &[0], &columns, None)
            .unwrap_err()
            .to_string()
            .contains("8 MiB"));
        assert!(
            validate_page(&[vec![Some(Value::Null), s("v")]], &[0], &columns, None)
                .unwrap_err()
                .to_string()
                .contains("non-null")
        );
        assert!(
            validate_page(&vec![vec![i(1), s("v")]; 1001], &[0], &columns, None)
                .unwrap_err()
                .to_string()
                .contains("1000 rows")
        );
    }
}
