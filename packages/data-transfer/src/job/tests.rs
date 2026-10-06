//! CM-46/47/48/49：有界管道、恢复裁决与 SQL 文件目标的单元覆盖。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_platform_api::dto::job::{Checkpoint, CommitBoundary};
use datazen_platform_api::id::StageId;

use crate::error::TransferError;
use crate::execute::ValueFormatter;
use crate::model::*;

use super::handler::{DataTransferHandler, InMemoryTransferCheckpoint, TransferEndpoints};
use super::pipeline::{
    execute_bounded_table, BoundedPipelineContext, PipelineBudget, PIPELINE_INITIAL_BYTES,
};
use super::plan::TransferFreezeBody;
use super::recovery::verify_checkpoint;
use datazen_runtime::job::JobHandler as _;

/// 把一次性查询结果拆成 §6.2 期望的流式事件序列（单语句、可分批、不截断）。
fn emit_stream(
    page: &QueryResult,
    sql: &str,
    batch_size: Option<u32>,
    callback: QueryStreamCallback,
) {
    callback(QueryStreamEvent::ExecutionStarted {
        execution_id: uuid::Uuid::new_v4().to_string(),
    });
    callback(QueryStreamEvent::StatementStart {
        index: 0,
        sql: sql.to_string(),
        columns: page.columns.clone(),
    });
    let batch = batch_size
        .map(|size| size as usize)
        .filter(|size| *size > 0)
        .unwrap_or(page.rows.len().max(1));
    for rows in page.rows.chunks(batch) {
        callback(QueryStreamEvent::Rows {
            index: 0,
            rows: rows.to_vec(),
        });
    }
    callback(QueryStreamEvent::StatementEnd {
        index: 0,
        rows_affected: page.rows_affected,
        execution_time_ms: 0,
        truncated: false,
    });
}

type Rows = Vec<Vec<Option<Value>>>;

// ------------------------------------------------------------------ fake DB

#[derive(Default)]
struct State {
    source_queries: Vec<(String, Vec<Value>)>,
    committed: Vec<Vec<Value>>,
    pending: Vec<Vec<Value>>,
    write_sqls: Vec<String>,
    execute_calls: usize,
}

/// 慢目标闸门：execute_with_params / commit 第一次命中时等待释放。
struct SlowGate {
    entered: tokio::sync::Notify,
    opened: tokio::sync::Notify,
    first: AtomicBool,
}

struct FakeDb {
    rows: Rows,
    schema: TableSchema,
    state: Mutex<State>,
    commit_error_after_effect: bool,
    slow: Option<Arc<SlowGate>>,
}

fn unsupported<T>() -> Result<T, DriverError> {
    Err(DriverError::Unsupported("unused in test".into()))
}

#[async_trait]
impl DatabaseDriver for FakeDb {
    async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
    // 有界分页恢复契约只承认已验证的分块驱动（postgresql / mysql），夹具必须如实申报。
    fn driver_type(&self) -> String {
        "postgresql".into()
    }
    fn quote_char(&self) -> char {
        '"'
    }
    fn parameter_placeholder(&self, _: usize, _: Option<&str>) -> Result<String, DriverError> {
        Ok("?".into())
    }
    async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        unsupported()
    }
    async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
    async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        unsupported()
    }
    async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        unsupported()
    }
    async fn get_tables(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        unsupported()
    }
    async fn get_table_schema(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Ok(self.schema.clone())
    }
    async fn query(&self, _: &ConnectionHandle, _: &str) -> Result<QueryResult, DriverError> {
        unsupported()
    }
    async fn query_multi(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        unsupported()
    }
    async fn query_with_params(
        &self,
        _: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        // 解析页参数：scope 无 WHERE，cursor = params[0]（exclusive > 语义）。
        let mut state = self.state.lock().unwrap();
        state
            .source_queries
            .push((sql.to_string(), params.to_vec()));
        let limit = parse_limit(sql);
        let cursor = params.first().and_then(|v| match v {
            Value::Integer(i) => Some(*i),
            Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        });
        let mut page: Vec<Vec<Option<Value>>> = self
            .rows
            .iter()
            .filter(|row| match (row.first(), cursor) {
                (Some(Some(Value::Integer(id))), Some(c)) => id > &c,
                _ => true,
            })
            .cloned()
            .collect();
        page.truncate(limit);
        Ok(QueryResult {
            columns: self
                .schema
                .columns
                .iter()
                .map(|c| ColumnInfo {
                    name: c.name.clone(),
                    data_type: c.data_type.clone(),
                    nullable: c.nullable,
                })
                .collect(),
            rows: page,
            rows_affected: None,
            execution_time_ms: 0,
        })
    }
    async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
        Ok(0)
    }
    // SQL 文件目标按 §6.2 走 spool 流式扫描，夹具如实申报流式能力。
    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        batch_size: Option<u32>,
        callback: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let page = self.query(handle, sql).await?;
        emit_stream(&page, sql, batch_size, callback);
        Ok(())
    }
    async fn query_stream_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        batch_size: Option<u32>,
        callback: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let page = self.query_with_params(handle, sql, params).await?;
        emit_stream(&page, sql, batch_size, callback);
        Ok(())
    }
    async fn begin_transaction(
        &self,
        _: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Ok(TransactionHandle {
            id: "tx".into(),
            connection_id: "conn".into(),
        })
    }
    async fn begin_read_snapshot(
        &self,
        _: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Ok(TransactionHandle {
            id: "snap".into(),
            connection_id: "conn".into(),
        })
    }
    async fn advance_transfer_identity_sequences(
        &self,
        _: &ConnectionHandle,
        _: Option<&str>,
        _: &str,
        _: &[String],
    ) -> Result<(), DriverError> {
        Ok(())
    }
    fn explicit_identity_insert_requires_session_toggle(&self) -> bool {
        false
    }
    async fn set_identity_insert(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<&str>,
        _: &str,
        _: bool,
    ) -> Result<(), DriverError> {
        Ok(())
    }
    async fn discard_connection(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
    fn transfer_explicit_identity_insert_clause(&self) -> Option<&'static str> {
        None
    }
    async fn execute_with_params(
        &self,
        _: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        {
            let mut state = self.state.lock().unwrap();
            state.execute_calls += 1;
            state.write_sqls.push(sql.to_string());
            state.pending.push(params.to_vec());
        }
        if let Some(slow) = &self.slow {
            if slow.first.swap(false, Ordering::SeqCst) {
                slow.entered.notify_one();
                slow.opened.notified().await;
            }
        }
        let affected = sql
            .split_once("VALUES ")
            .map(|(_, v)| v.matches('(').count() as u64)
            .unwrap_or(1);
        Ok(affected)
    }
    async fn commit(&self, _: TransactionHandle) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        if self.commit_error_after_effect {
            let pending = std::mem::take(&mut state.pending);
            state.committed.extend(pending);
            return Err(DriverError::TransactionError(
                "injected lost commit acknowledgement".into(),
            ));
        }
        let pending = std::mem::take(&mut state.pending);
        state.committed.extend(pending);
        Ok(())
    }
    async fn rollback(&self, _: TransactionHandle) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        state.pending.clear();
        Ok(())
    }
}

fn parse_limit(sql: &str) -> usize {
    sql.split("LIMIT ")
        .nth(1)
        .and_then(|tail| tail.trim().split(' ').next())
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(1000)
}

fn schema_with_snapshot(names: &[&str]) -> TableSchema {
    let mut schema = base_schema(names);
    // 分页恢复要求声明式主键索引顺序可核对，复合键顺序只在索引元数据里。
    schema.indexes = vec![IndexInfo {
        name: "PRIMARY".into(),
        columns: vec!["id".into()],
        is_unique: true,
        is_primary: true,
        index_type: "BTREE".into(),
    }];
    schema
}

/// SQL 文件目标未注册 IR 适配器时只允许基础建表；索引/外键 DDL 必须有 IR 渲染。
fn schema_without_objects(names: &[&str]) -> TableSchema {
    base_schema(names)
}

fn base_schema(names: &[&str]) -> TableSchema {
    TableSchema {
        table_name: "t".into(),
        columns: names
            .iter()
            .map(|name| ColumnSchema {
                name: (*name).into(),
                data_type: if *name == "id" {
                    "INTEGER".into()
                } else {
                    "TEXT".into()
                },
                nullable: *name != "id",
                default_value: None,
                comment: None,
                is_primary_key: *name == "id",
                is_auto_increment: false,
            })
            .collect(),
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: TableOptions {
            supports_consistent_snapshot: Some(true),
            ..Default::default()
        },
    }
}

fn src_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "src-1".into(),
        pool_id: "pool-s".into(),
    }
}

fn tgt_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "tgt-1".into(),
        pool_id: "pool-t".into(),
    }
}

fn columns(names: &[&str]) -> Vec<ColumnMapping> {
    names
        .iter()
        .map(|n| ColumnMapping {
            source_column: (*n).into(),
            target_column: (*n).into(),
            skip: false,
            target_native_type: None,
        })
        .collect()
}

fn job_for_pipeline(batch_size: u32) -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: "src-session".into(),
            database: "srcdb".into(),
            schema: None,
        },
        target: Some(Endpoint {
            db_session_id: "tgt-session".into(),
            database: "tgtdb".into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping {
            source_table: "t".into(),
            target_table: "t".into(),
            create_new: false,
            enabled: true,
            column_mappings: columns(&["id", "name"]),
            ddl_override: None,
            source_filter: None,
            recordset: None,
        }],
        options: TransferOptions {
            batch_size,
            stop_on_error: true,
            confirmed_destructive: false,
            use_target_default_collation: false,
        },
    }
}

fn inspected_for(names: &[&str]) -> TableInspectResult {
    TableInspectResult {
        source_table: "t".into(),
        target_table: "t".into(),
        status: TableMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: columns(names),
        source_columns: names.iter().map(|s| s.to_string()).collect(),
        source_primary_keys: vec!["id".into()],
        target_columns: names.iter().map(|s| s.to_string()).collect(),
        source_column_types: HashMap::new(),
        target_column_types: HashMap::new(),
        incompatible_reason: None,
        source_row_count: None,
        recordset: None,
    }
}

fn frozen_plan_for_apply() -> datazen_runtime::job::FrozenPlan {
    datazen_runtime::job::FrozenPlan {
        kind: "dataTransferApply".into(),
        is_apply: true,
        consumed_plan_id: Some("plan-1".into()),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: Some(1),
    }
}

// ------------------------------------------------------------------ CM-47/48

fn checkpoint_with(markers: &[&str], policies: &str, committed: Vec<CommitBoundary>) -> Checkpoint {
    Checkpoint {
        job_id: datazen_platform_api::id::JobId::new("job-1"),
        state_version: datazen_platform_api::id::JobStateVersion::new(1),
        stable_target_fingerprint: "sha256:target".into(),
        committed,
        verification_evidence: markers.iter().map(|m| m.to_string()).collect(),
        recovery_policy: policies.into(),
    }
}

fn boundary(rows: u64) -> CommitBoundary {
    CommitBoundary {
        stage_id: StageId::new("data"),
        stable_target_fingerprint: "sha256:target".into(),
        committed_at: datazen_platform_api::id::Timestamp::new("2026-01-01T00:00:00Z"),
        operation_id: None,
        batch_id: Some("t#b0".into()),
        payload_digest: Some("digest".into()),
        evidence: vec![format!("rows={rows}"), "confirmed=target-commit-ack".into()],
        verified_at: None,
    }
}

// 分文件承载 CM-46 / CM-47-48 / CM-49 / CANCEL_WATCH 集成，单文件规模保持在 800 行以内。
mod cm46_pipeline;
mod cm47_48_recovery;
mod cm49_sql_file;
mod kernel_cancel;
mod stage_shape;
