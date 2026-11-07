//! 迁移运行历史两段落账 + profile 引用校验（W8 刀3b-2）。
//!
//! 桌面对照 `datazen-desktop/src/commands/history.rs`：`start_migration_run`
//! / `finish_migration_run` / `validate_migration_profile_ref` 的服务端对应
//! 物。存储 = v12 `migration_run_history`（[`ServerMigrationRuns`]）；
//! profile 读取 = [`ServerSchemaDiffProfiles`]（同一事实源，组织隔离）。
//!
//! 落账语义与桌面一致：**best-effort**——存储失败只 `warn`，不阻断执行
//! 应答（部署编排先落账再执行，失败收尾用同 id 覆写终态行）。

use datazen::history::{MigrationProfileRef, MigrationRunRecord};
use datazen::migration::schema_diff::SchemaDiffProfileRepository as _;
use datazen::shared::scope::OrganizationScope;

use crate::storage::migration_runs::ServerMigrationRuns;
use crate::storage::schema_diff_profiles::ServerSchemaDiffProfiles;

/// 历史服务（composition root 注入）。
#[derive(Clone)]
pub struct MigrationHistory {
    runs: std::sync::Arc<ServerMigrationRuns>,
    profiles: std::sync::Arc<ServerSchemaDiffProfiles>,
}

impl MigrationHistory {
    pub fn new(
        runs: std::sync::Arc<ServerMigrationRuns>,
        profiles: std::sync::Arc<ServerSchemaDiffProfiles>,
    ) -> Self {
        Self { runs, profiles }
    }

    /// 开始一段运行：落 `running` 行（best-effort），返回可覆写的记录。
    pub async fn start_run(
        &self,
        organization_id: &str,
        operation: &str,
        profile: Option<&MigrationProfileRef>,
    ) -> MigrationRunRecord {
        let run = MigrationRunRecord {
            id: uuid::Uuid::new_v4().to_string(),
            operation: operation.into(),
            status: "running".into(),
            outcome: "pending".into(),
            phase: "execute".into(),
            profile_id: profile.map(|value| value.id.clone()),
            profile_revision: profile.map(|value| value.revision.clone()),
            source_connection_id: None,
            target_connection_id: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            selected_count: 0,
            committed_count: 0,
            failed_count: 0,
            conflict_count: 0,
            cancelled: false,
            rollback_outcome: "notRequired".into(),
            error_summary: None,
        };
        if let Err(error) = self.runs.save(organization_id, run.clone()).await {
            tracing::warn!(%error, operation, "failed to persist migration run start");
        }
        run
    }

    /// 收尾一段运行：覆写终态（best-effort）。
    #[allow(clippy::too_many_arguments)]
    pub async fn finish_run(
        &self,
        organization_id: &str,
        mut run: MigrationRunRecord,
        success: bool,
        cancelled: bool,
        committed: u64,
        failed: u64,
        conflicts: u64,
        rollback_outcome: &str,
    ) {
        run.status = if cancelled {
            "cancelled"
        } else if success {
            "completed"
        } else {
            "failed"
        }
        .into();
        run.outcome = if success {
            "success"
        } else if cancelled {
            "cancelled"
        } else {
            "failed"
        }
        .into();
        run.phase = "finished".into();
        run.finished_at = Some(chrono::Utc::now().to_rfc3339());
        run.committed_count = committed;
        run.failed_count = failed;
        run.conflict_count = conflicts;
        run.cancelled = cancelled;
        run.rollback_outcome = rollback_outcome.into();
        if !success && !cancelled {
            run.error_summary =
                Some("Execution failed; details are available in the application log".into());
        }
        if let Err(error) = self.runs.save(organization_id, run).await {
            tracing::warn!(%error, "failed to persist migration run completion");
        }
    }

    /// profile 引用校验（桌面 `validate_migration_profile_ref` 的 schemaDiff
    /// 臂）：id 缺席 → 拒绝；`updated_at`（RFC3339 解析等值）不匹配 →
    /// 拒绝（「计划评审时的 profile」与「执行时的 profile」必须是同一版）。
    pub async fn validate_profile_ref(
        &self,
        organization_id: &str,
        reference: Option<&MigrationProfileRef>,
    ) -> Result<(), String> {
        let Some(reference) = reference else {
            return Ok(());
        };
        let scope = OrganizationScope {
            organization_id: datazen::shared::id::OrganizationId::new(organization_id),
        };
        let profiles = self
            .profiles
            .list(&scope)
            .await
            .map_err(|error| error.to_string())?;
        let actual = profiles
            .iter()
            .find(|profile| profile.id == reference.id)
            .map(|profile| profile.updated_at.to_rfc3339());
        let revisions_match = |actual: &str, expected: &str| {
            chrono::DateTime::parse_from_rfc3339(actual).ok()
                == chrono::DateTime::parse_from_rfc3339(expected).ok()
        };
        match actual.as_deref() {
            Some(actual) if revisions_match(actual, reference.revision.as_str()) => Ok(()),
            Some(_) => Err("Migration profile changed; review and prepare the plan again".into()),
            None => Err("Migration profile is missing; execution was blocked".into()),
        }
    }
}
