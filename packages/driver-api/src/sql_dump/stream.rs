//! Awaitable SQL dump rendering. Sources must use one retained resource for
//! metadata and row reads. No database-wide or table-wide row buffer is used.
use super::{
    drop_object_sql, is_dumpable_object, is_view_like, partition_dump_objects, qualified_ident,
    INSERT_BATCH_MAX_BYTES, INSERT_BATCH_MAX_ROWS,
};
use crate::{
    async_trait,
    resource::{ResourceError, ResultChunk, ResultSink, StatementSummary},
    *,
};

pub const BACKUP_COMMAND: &str = "backup_dump";
pub const BACKUP_CAPABILITY: &str = "retainedSqlBackupV1";
pub const BACKUP_CHUNK_BYTES: usize = 64 * 1024;

/// Optional provider implementation seam. The legacy String dump is never a
/// fallback for this contract. `rows` must await the sink while consuming rows.
#[async_trait]
pub trait RetainedBackupSource: Send + Sync {
    fn driver(&self) -> &dyn DatabaseDriver;
    async fn objects(&self) -> Result<Vec<TableInfo>, ResourceError>;
    async fn ddl(&self, object: &TableInfo) -> Result<String, ResourceError>;
    async fn columns(&self, object: &TableInfo) -> Result<Vec<String>, ResourceError>;
    async fn rows(&self, sql: &str, sink: &dyn ResultSink) -> Result<(), ResourceError>;
    async fn extras(&self, routines: bool, sink: &dyn ResultSink) -> Result<(), ResourceError>;
}

pub async fn text(sink: &dyn ResultSink, text: &str) -> Result<(), ResourceError> {
    let mut remaining = text;
    while !remaining.is_empty() {
        let mut end = remaining.len().min(BACKUP_CHUNK_BYTES);
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        sink.write_output(serde_json::json!({"kind":"backupChunk", "text": &remaining[..end]}))
            .await?;
        remaining = &remaining[end..];
    }
    Ok(())
}

/// Keep untrusted identifiers inside a single SQL comment line.
pub fn comment_label(name: &str) -> String {
    name.replace(['\r', '\n'], " ")
}

pub async fn dump(
    source: &dyn RetainedBackupSource,
    database: &str,
    opts: &BackupDumpOptions,
    sink: &dyn ResultSink,
) -> Result<(), ResourceError> {
    let driver = source.driver();
    let objects = source
        .objects()
        .await?
        .into_iter()
        .filter(is_dumpable_object)
        .collect();
    let (tables, views) = partition_dump_objects(objects);
    text(
        sink,
        &format!(
            "-- DataZen backup: {}\n-- Date: {}\n",
            comment_label(database),
            chrono::Utc::now().to_rfc3339()
        ),
    )
    .await?;
    let mut flags = Vec::new();
    for (enabled, name) in [
        (opts.schema_only, "schema-only"),
        (opts.data_only, "data-only"),
        (opts.clean, "clean"),
        (opts.create_database, "create"),
        (opts.no_owner, "no-owner"),
        (opts.single_transaction, "single-transaction"),
        (opts.routines, "routines"),
        (opts.triggers, "triggers"),
    ] {
        if enabled {
            flags.push(name);
        }
    }
    if !flags.is_empty() {
        text(sink, &format!("-- Options: {}\n", flags.join(", "))).await?;
    }
    if opts.no_owner {
        text(sink, "-- no-owner: OWNER clauses omitted\n").await?;
    }
    text(sink, "\n").await?;
    if opts.clean {
        for object in views.iter().chain(tables.iter()) {
            text(
                sink,
                &drop_object_sql(
                    &|n| driver.quote_ident(n),
                    &object.name,
                    &object.table_type,
                    object.schema.as_deref(),
                ),
            )
            .await?;
        }
        text(sink, "\n").await?;
    }
    let total = (tables.len() + views.len()) as u32;
    for (index, object) in tables.iter().chain(views.iter()).enumerate() {
        sink.write_output(serde_json::json!({"kind":"backupProgress", "progress": DumpProgress {current:index as u32+1,total,object_name:object.name.clone(),phase:DumpPhase::Object}})).await?;
        let view = is_view_like(&object.table_type);
        text(
            sink,
            &format!(
                "-- {}: {}\n",
                if view { "View" } else { "Table" },
                comment_label(&object.name)
            ),
        )
        .await?;
        if !opts.data_only {
            text(sink, &source.ddl(object).await?).await?;
            text(sink, "\n").await?;
        }
        if opts.schema_only || view {
            continue;
        }
        let columns = source.columns(object).await?;
        if columns.is_empty() {
            return Err(DriverError::QueryFailed("backup table has no columns".into()).into());
        }
        let columns = columns
            .iter()
            .map(|c| driver.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let relation = qualified_ident(
            &|n| driver.quote_ident(n),
            object.schema.as_deref(),
            &object.name,
        );
        let rows = InsertSink {
            driver,
            sink,
            prefix: format!("INSERT INTO {relation} ({columns}) VALUES "),
        };
        source
            .rows(&format!("SELECT {columns} FROM {relation}"), &rows)
            .await?;
        text(sink, "\n").await?;
    }
    if !opts.data_only {
        if opts.routines {
            text(sink, "-- Routines (functions / procedures)\n").await?;
            source.extras(true, sink).await?;
        }
        if opts.triggers {
            text(sink, "-- Triggers\n").await?;
            source.extras(false, sink).await?;
        }
    }
    Ok(())
}

struct InsertSink<'a> {
    driver: &'a dyn DatabaseDriver,
    sink: &'a dyn ResultSink,
    prefix: String,
}
#[async_trait]
impl ResultSink for InsertSink<'_> {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        let mut batch = String::new();
        let mut count = 0;
        for row in chunk.rows {
            let tuple = bounded_tuple(self.driver, &self.prefix, &row)?;
            if count > 0
                && (count >= INSERT_BATCH_MAX_ROWS
                    || self.prefix.len() + batch.len() + tuple.len() + 4 > INSERT_BATCH_MAX_BYTES)
            {
                text(self.sink, &self.prefix).await?;
                text(self.sink, &batch).await?;
                text(self.sink, ";\n").await?;
                batch.clear();
                count = 0;
            }
            if count > 0 {
                batch.push_str(", ");
            }
            batch.push_str(&tuple);
            count += 1;
        }
        if count > 0 {
            text(self.sink, &self.prefix).await?;
            text(self.sink, &batch).await?;
            text(self.sink, ";\n").await?;
        }
        Ok(())
    }
    async fn write_statement_summary(&self, _: StatementSummary) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        Err(DriverError::QueryFailed("backup row stream failed".into()).into())
    }
}

/// Render a streamed one-column DDL query without collecting all routines.
pub struct DdlSink<'a>(pub &'a dyn ResultSink);
#[async_trait]
impl ResultSink for DdlSink<'_> {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        for row in chunk.rows {
            let Some(Some(Value::String(ddl))) = row.first() else {
                return Err(DriverError::QueryFailed("missing backup DDL".into()).into());
            };
            text(self.0, ddl.trim_end().trim_end_matches(';')).await?;
            text(self.0, ";\n\n").await?;
        }
        Ok(())
    }
    async fn write_statement_summary(&self, _: StatementSummary) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        Err(DriverError::QueryFailed("backup DDL stream failed".into()).into())
    }
}

pub fn command_definition() -> DriverCommandDefinition {
    let mut metadata = DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Read);
    metadata.workflow = false;
    metadata.ui = false;
    DriverCommandDefinition {
        id: BACKUP_COMMAND.into(),
        name: "Retained SQL backup".into(),
        description: Some("Host-bound SQL backup stream on one Runtime resource".into()),
        input_schema: serde_json::json!({"type":"object","required":["database","options"],"properties":{"database":{"type":"string"},"options":{"type":"object"}}}),
        output_schema: None,
        permissions: vec![],
        metadata,
    }
}

fn bounded_tuple(
    driver: &dyn DatabaseDriver,
    prefix: &str,
    row: &[Option<Value>],
) -> Result<String, ResourceError> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            if self.0 > INSERT_BATCH_MAX_BYTES {
                return Err(std::io::Error::other("backup row exceeds INSERT limit"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let reject = || ResourceError::SinkRejected {
        reason: "backup row exceeds INSERT limit".into(),
    };
    let mut tuple = String::from("(");
    for (index, value) in row.iter().enumerate() {
        // Count without allocating an encoded copy before dialect formatting.
        serde_json::to_writer(Counter(0), value).map_err(|_| reject())?;
        let literal = driver.format_sql_literal(value);
        let separator = if index == 0 { "" } else { ", " };
        if prefix
            .len()
            .saturating_add(tuple.len())
            .saturating_add(separator.len())
            .saturating_add(literal.len())
            .saturating_add(3)
            > INSERT_BATCH_MAX_BYTES
        {
            return Err(reject());
        }
        tuple.push_str(separator);
        tuple.push_str(&literal);
    }
    tuple.push(')');
    Ok(tuple)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Sink(Mutex<Vec<serde_json::Value>>);
    #[async_trait]
    impl ResultSink for Sink {
        async fn write(&self, _: ResultChunk) -> Result<(), ResourceError> {
            unreachable!()
        }
        async fn write_output(&self, v: serde_json::Value) -> Result<(), ResourceError> {
            self.0.lock().unwrap().push(v);
            Ok(())
        }
        async fn complete(&self) -> Result<(), ResourceError> {
            Ok(())
        }
        async fn fail(&self, _: &str) -> Result<(), ResourceError> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn chunks_preserve_utf8_and_bound_transfer_size() {
        let sink = Sink::default();
        let input = "汉字'".repeat(40000);
        text(&sink, &input).await.unwrap();
        let output = sink.0.lock().unwrap();
        assert!(output.len() > 1);
        assert!(output
            .iter()
            .all(|v| v["text"].as_str().unwrap().len() <= BACKUP_CHUNK_BYTES));
        assert_eq!(
            output
                .iter()
                .map(|v| v["text"].as_str().unwrap())
                .collect::<String>(),
            input
        );
    }
    #[test]
    fn identifier_comments_cannot_escape_the_comment_line() {
        assert_eq!(
            comment_label("name\nDROP TABLE users;\r"),
            "name DROP TABLE users; "
        );
    }
    #[test]
    fn rejects_oversized_value_and_escaped_tuple() {
        let driver = crate::mock_driver::MockDriver::new("postgresql", Default::default());
        for value in [
            "x".repeat(INSERT_BATCH_MAX_BYTES + 1),
            "'".repeat(INSERT_BATCH_MAX_BYTES / 2),
        ] {
            assert!(bounded_tuple(
                &*driver,
                "INSERT INTO t (x) VALUES ",
                &[Some(Value::String(value))]
            )
            .is_err());
        }
        assert!(bounded_tuple(
            &*driver,
            "INSERT INTO t (x) VALUES ",
            &[Some(Value::String("ok".into()))]
        )
        .is_ok());
    }
    #[tokio::test]
    async fn insert_batches_never_exceed_existing_limit() {
        let driver = crate::mock_driver::MockDriver::new("postgresql", Default::default());
        let sink = Sink::default();
        let rows = InsertSink {
            driver: &*driver,
            sink: &sink,
            prefix: "INSERT INTO t (x) VALUES ".into(),
        };
        rows.write(ResultChunk {
            statement_index: 0,
            sql: String::new(),
            rows: vec![vec![Some(Value::String("x".repeat(4000)))]; 600],
            rows_affected: None,
        })
        .await
        .unwrap();
        let sql = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|v| v["text"].as_str().unwrap())
            .collect::<String>();
        assert!(sql
            .split_inclusive(";\n")
            .all(|s| s.len() <= INSERT_BATCH_MAX_BYTES));
        assert_eq!(sql.matches("INSERT INTO").count(), 5);
    }
}
