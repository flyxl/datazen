use serde_json::{Map, Value};

use super::types::{
    ImportBatch, ImportChunk, ImportColumnPair, ImportCommitMode, ImportSqlDialect,
    PlanImportChunksOptions,
};

fn ident(name: &str, database_type: Option<&str>, dialect: &ImportSqlDialect) -> String {
    (dialect.escape_ident)(name, database_type)
}

fn column_list(target_columns: &[String], database_type: Option<&str>, dialect: &ImportSqlDialect) -> String {
    target_columns
        .iter()
        .map(|column| ident(column, database_type, dialect))
        .collect::<Vec<_>>()
        .join(", ")
}

fn tuple_for(
    row: &Map<String, Value>,
    target_columns: &[String],
    database_type: Option<&str>,
    dialect: &ImportSqlDialect,
) -> String {
    let values = target_columns
        .iter()
        .map(|column| {
            let value = row.get(column).unwrap_or(&Value::Null);
            (dialect.escape_value)(database_type, value)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("({values})")
}

fn insert_prefix(
    table: &str,
    columns: &str,
    database_type: Option<&str>,
    dialect: &ImportSqlDialect,
) -> String {
    format!("INSERT INTO {} ({columns}) VALUES\n  ", ident(table, database_type, dialect))
}

pub fn build_multi_row_insert_sql(
    table: &str,
    target_columns: &[String],
    database_type: Option<&str>,
    rows: &[Map<String, Value>],
    row_start: usize,
    row_count: usize,
    dialect: &ImportSqlDialect,
) -> String {
    let columns = column_list(target_columns, database_type, dialect);
    let tuples: Vec<String> = (row_start..row_start + row_count)
        .map(|index| {
            let empty = Map::new();
            let row = rows.get(index).unwrap_or(&empty);
            tuple_for(row, target_columns, database_type, dialect)
        })
        .collect();
    format!(
        "{}{};",
        insert_prefix(table, &columns, database_type, dialect),
        tuples.join(",\n  ")
    )
}

fn project_rows(rows: &[Map<String, Value>], column_pairs: &[ImportColumnPair]) -> Vec<Map<String, Value>> {
    rows.iter()
        .map(|row| {
            let mut out = Map::new();
            for pair in column_pairs {
                let value = row.get(&pair.source).cloned().unwrap_or(Value::Null);
                out.insert(pair.target.clone(), value);
            }
            out
        })
        .collect()
}

pub fn plan_import_chunks(options: &PlanImportChunksOptions, dialect: &ImportSqlDialect) -> Vec<ImportChunk> {
    let database_type = options.database_type.as_deref();
    let projected = project_rows(&options.rows, &options.column_pairs);
    let columns = column_list(&options.target_columns, database_type, dialect);
    let prefix_length = insert_prefix(&options.table, &columns, database_type, dialect).len();

    let mut batches: Vec<ImportBatch> = Vec::new();
    let mut batch_start = 0usize;
    while batch_start < projected.len() {
        let mut tuples: Vec<String> = Vec::new();
        let mut bytes = prefix_length;
        let mut row_count = 0usize;
        while batch_start + row_count < projected.len() {
            let row = &projected[batch_start + row_count];
            let tuple = tuple_for(row, &options.target_columns, database_type, dialect);
            let added_bytes = tuple.len() + if tuples.is_empty() { 0 } else { 2 };
            if row_count > 0
                && (row_count + 1 > options.max_rows_per_batch || bytes + added_bytes > options.max_sql_bytes)
            {
                break;
            }
            tuples.push(tuple);
            bytes += added_bytes;
            row_count += 1;
        }
        let sql = format!(
            "{}{};",
            insert_prefix(&options.table, &columns, database_type, dialect),
            tuples.join(",\n  ")
        );
        batches.push(ImportBatch { row_start: batch_start, row_count, sql });
        batch_start += row_count;
    }

    if options.commit_mode == ImportCommitMode::Autocommit {
        return batches
            .into_iter()
            .map(|batch| ImportChunk {
                sql: batch.sql.clone(),
                row_start: batch.row_start,
                row_count: batch.row_count,
                batches: vec![batch],
            })
            .collect();
    }

    let mut chunks = Vec::new();
    let mut index = 0usize;
    while index < batches.len() {
        let end = (index + options.max_batches_per_chunk).min(batches.len());
        let group: Vec<ImportBatch> = batches[index..end].to_vec();
        let sql = format!(
            "BEGIN;\n{}\nCOMMIT;",
            group.iter().map(|batch| batch.sql.as_str()).collect::<Vec<_>>().join("\n")
        );
        let row_start = group.first().map_or(index, |batch| batch.row_start);
        let row_count = group.iter().map(|batch| batch.row_count).sum();
        chunks.push(ImportChunk { batches: group, sql, row_start, row_count });
        index = end;
    }
    chunks
}
