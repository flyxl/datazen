//! 结果/产物 TTL 清扫器（WR-D6）。
//!
//! [ArtifactRepository::delete_expired](datazen::execution::domain::artifact_repository::ArtifactRepository)
//! 是端口既有的 TTL 删除入口（`ArtifactExpired` 的生成侧）；本模块给它一个
//! **周期驱动 + 审计**的宿主面：到期产物被移除后读取必须答「已过期」，
//! 不得用缓存副本继续服务（§4.6）。
//!
//! 配置来源：与 `ServerConfig` 的既有模式一致——部署可调的覆盖项经构造
//! 参数注入（`config.rs` 是唯一配置入口，本模块不读环境变量）。新增键与
//! 默认值在 WR-D6 报告登记：
//!
//! | 环境变量（主线接线） | 类型 | 默认 | 语义 |
//! |---|---|---|---|
//! | `DATAZEN_SERVER_ARTIFACT_TTL_SWEEP_INTERVAL_SECS` | u64 | `300` | 清扫周期（秒） |
//!
//! 审计：每清理一个产物落一条 `artifact_ttl_swept`（`Succeeded`，service
//! 主体）。`AuditPayload` 白名单没有「产物 id」键（§2.4 四键封顶），产物
//! 明细进 `tracing` 日志——这是白名单纪律下的已登记取舍。

use std::sync::Arc;
use std::time::Duration;

use datazen::execution::domain::artifact_repository::ArtifactRepository;
use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind, AuditSink};
use datazen::shared::error::ApiError;
use datazen::shared::id::{OrganizationId, Timestamp};

/// 清扫配置（构造注入；默认值见模块文档的登记表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtlSweepConfig {
    /// 两次清扫的间隔（秒）。
    pub interval_seconds: u64,
}

impl Default for TtlSweepConfig {
    fn default() -> Self {
        Self {
            interval_seconds: DEFAULT_TTL_SWEEP_INTERVAL_SECS,
        }
    }
}

/// 清扫周期的首版建议值（部署可调；非协议常量）。
pub const DEFAULT_TTL_SWEEP_INTERVAL_SECS: u64 = 300;

/// 一次清扫的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtlSweepReport {
    /// 本轮移除的产物数。
    pub deleted_count: usize,
    /// 审计是否全部落库（任一失败即为 `false`，失败明细在日志）。
    pub audited: bool,
}

/// TTL 清扫器。`A` 是 [`ArtifactRepository`] 端口实现（服务端装配注入
/// `crate::storage::ServerArtifacts`）。
///
/// `audit_organization`：审计行的组织归属。端口缺口登记——
/// `delete_expired` 只回产物 id、不回归属组织，而 `audit_events` 无组织
/// 即无行；单组织 v1 里由装配层传 `ServerConfig::organization_id`
/// （多组织分账审计随端口扩展补）。
pub struct TtlSweeper<A: ArtifactRepository> {
    artifacts: Arc<A>,
    audit: Arc<dyn AuditSink>,
    audit_organization: OrganizationId,
    config: TtlSweepConfig,
}

impl<A: ArtifactRepository> TtlSweeper<A> {
    pub fn new(
        artifacts: Arc<A>,
        audit: Arc<dyn AuditSink>,
        audit_organization: OrganizationId,
        config: TtlSweepConfig,
    ) -> Self {
        Self {
            artifacts,
            audit,
            audit_organization,
            config,
        }
    }

    /// 单轮清扫：删除过期产物 + 审计。可由后台任务周期调用，也可由运维
    /// 探针手动触发（`readyz` 深检等）。
    pub async fn sweep_once(&self) -> Result<TtlSweepReport, ApiError> {
        let now = Timestamp::new(super::now_string());
        let deleted = self
            .artifacts
            .delete_expired(now)
            .await
            .map_err(|e| super::port_error_to_api_error(&e))?;
        let mut audited = true;
        for artifact_id in &deleted {
            let event = AuditEvent::new(
                self.audit_organization.clone(),
                AuditPrincipalKind::Service,
                "artifact_ttl_swept",
                AuditOutcome::Succeeded,
                Timestamp::new(super::now_string()),
            );
            if let Err(source) = self.audit.record(event) {
                audited = false;
                tracing::error!(
                    error = %source,
                    artifact = %artifact_id.as_str(),
                    "ttl sweep audit append failed"
                );
            } else {
                tracing::info!(artifact = %artifact_id.as_str(), "artifact expired and removed");
            }
        }
        Ok(TtlSweepReport {
            deleted_count: deleted.len(),
            audited,
        })
    }

    /// 周期清扫循环（装配层 `tokio::spawn` 并持有 JoinHandle 做停机）。
    /// 单轮失败不终止循环：TTL 清扫是尽力而为的周期任务，下一轮重试。
    pub async fn run_forever(&self) {
        let interval = Duration::from_secs(self.config.interval_seconds.max(1));
        loop {
            tokio::time::sleep(interval).await;
            if let Err(source) = self.sweep_once().await {
                tracing::warn!(error = %source, "ttl sweep round failed; retrying next cycle");
            }
        }
    }
}
