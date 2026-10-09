//! CM-46：产物元数据冻结「生效源过滤」与「相同行基线」。
//!
//! prepare 把实际用于源侧读取的过滤捕获进 Artifact（重验/预览必须复用
//! 同一过滤），并冻结无需写入的相同行数（review 面板的基线）。断言这两点
//! 都落在 `ChangeSetArtifact.table_meta` 上，且经受 serde 往返（落盘/回读不丢）。

#[macro_use]
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use datazen_data_sync::filter::{SyncFilterLogic, SyncSourceFilter};
use datazen_data_sync::job::{DataSyncHandler, DataSyncHost, PrepareSpec};
use datazen_data_sync::model::{Row, SyncOptions, TableMapping};
use datazen_driver_api::filters::{FilterCondition, FilterOperator};
use datazen_driver_api::Value;
use datazen_platform_api::id::StageId;
use datazen_runtime::job::{CancelToken, JobHandler, StageSpec, StageTerminal};

use support::host::{source_endpoint, target_endpoint, FakeHost};
use support::rows::{int, row, text, TABLE};

fn old_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("old-{n}"))])
}

fn new_row(n: i64) -> Row {
    row(vec![int(n), text(&format!("new-{n}"))])
}

fn source_filter() -> SyncSourceFilter {
    SyncSourceFilter::new(
        vec![FilterCondition {
            column: "name".into(),
            operator: FilterOperator::Eq,
            value: Value::String("old-1".into()),
        }],
        SyncFilterLogic::And,
    )
    .unwrap()
}

fn seeded_host() -> Arc<FakeHost> {
    let fake = FakeHost::new();
    fake.source_rows(TABLE, vec![new_row(1), new_row(2), new_row(3)]);
    fake.target_rows(TABLE, vec![old_row(1), old_row(2), old_row(3)]);
    Arc::new(fake)
}

fn stage(kind: &str) -> StageSpec {
    StageSpec {
        stage_id: StageId::new(kind),
        kind: kind.to_string(),
        depends_on: Vec::new(),
    }
}

#[tokio::test]
async fn prepare_freezes_effective_source_filter_and_unchanged_baseline() {
    let host = seeded_host();
    let dyn_host: Arc<dyn DataSyncHost> = Arc::clone(&host) as Arc<dyn DataSyncHost>;
    let filter = source_filter();
    let mut filters = HashMap::new();
    filters.insert(TABLE.to_string(), filter.clone());
    let handler = DataSyncHandler::for_prepare(
        PrepareSpec {
            source: source_endpoint(),
            target: target_endpoint(),
            mappings: vec![TableMapping::auto(TABLE)],
            options: SyncOptions::default(),
            filters,
            plan_id: Some("plan-46".into()),
        },
        dyn_host,
    );
    let outcome = handler
        .run_stage(&stage("prepare"), &CancelToken::new())
        .await
        .expect("prepare stage returns an outcome");
    assert_eq!(outcome.terminal, StageTerminal::Succeeded);

    let artifact = host
        .load_artifact("plan-46")
        .await
        .expect("artifact lookup succeeds")
        .expect("prepare stored the ChangeSet");
    assert_eq!(artifact.blocks.len(), 3, "全部 3 行两侧不一致，均为 UPDATE");
    assert_eq!(artifact.table_meta.len(), 1);
    let meta = &artifact.table_meta[0];
    assert_eq!(meta.relation.table, TABLE);
    // 生效过滤被冻结：重验/预览复现与 prepare 完全相同的源侧过滤。
    assert_eq!(meta.source_filter, Some(filter.clone()));
    // review 基线：无一相同 ⇒ 0。
    assert_eq!(meta.unchanged_count, 0);

    // serde 往返（落盘/回读）不丢这两个字段。
    let text = serde_json::to_string(&artifact).expect("artifact serializes");
    let revived = serde_json::from_str::<datazen_data_sync::job::ChangeSetArtifact>(&text)
        .expect("artifact deserializes");
    assert_eq!(revived.table_meta[0].source_filter, Some(filter));
    assert_eq!(revived.table_meta[0].unchanged_count, 0);
}
