//! 管理库迁移表（expand/contract 纪律见 `super`）。
//!
//! 版本号只增；`expand` 是唯一被执行的 SQL。计划中的收缩步骤以
//! `contract` 注释记录，执行器拒绝跑它——等备份核验切片（WR-G3）落地。

/// 一个迁移单元。`contract` 是**计划**的收缩 SQL，当前执行器不执行。
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub expand: &'static str,
}

/// v1 基线：连接档案表（`ProfileRepository` 的服务端落地）。
///
/// 列语义与 `ProfileRecord` 一一对应：
/// - 复合主键 `(organization_id, connection_id)`：组织隔离在**存储层**就是
///   硬边界（CM-05），越组织的 id 查询天然落空。
/// - `secret_ref` 只存引用（`SecretRef` 字符串），明文永不入行——服务端
///   解析走 `SecretProvider`（WR-D4）。
/// - `initial_namespace_json` / `driver_options_json` 是投影列，读取时由
///   适配器反序列化回 domain 类型。
/// - `creation_key` 承载 `create` 的幂等（`{principalId}:{idempotencyKey}`，
///   与桌面 `platform_creation` 同款键形）；部分唯一索引让 NULL（非 create
///   写入的行）不参与唯一约束。
const V1_CONNECTION_PROFILE: Migration = Migration {
    version: 1,
    name: "connection_profile baseline",
    expand: r#"
CREATE TABLE connection_profile (
  organization_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  name TEXT NOT NULL,
  driver_id TEXT NOT NULL,
  config_revision INTEGER NOT NULL CHECK (config_revision > 0),
  credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
  network_route_revision INTEGER NOT NULL DEFAULT 1 CHECK (network_route_revision > 0),
  initial_namespace_json TEXT NOT NULL,
  driver_options_json TEXT NOT NULL DEFAULT '{}',
  secret_ref TEXT,
  network_route_ref TEXT,
  read_only INTEGER NOT NULL DEFAULT 0 CHECK (read_only IN (0, 1)),
  policy_ref TEXT,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  creation_key TEXT,
  PRIMARY KEY (organization_id, connection_id)
);
CREATE UNIQUE INDEX idx_connection_profile_creation_key
  ON connection_profile (organization_id, creation_key)
  WHERE creation_key IS NOT NULL;
CREATE INDEX idx_connection_profile_list
  ON connection_profile (organization_id, enabled, name);
"#,
};

/// 全部迁移，按版本升序。执行器假定有序且无空洞。
pub const MIGRATIONS: &[Migration] = &[
    V1_CONNECTION_PROFILE,
    V2_KNOWN_HOSTS,
    V3_JOBS,
    V4_ARTIFACTS,
];

/// v2：SSH known-hosts 表（`KnownHostsRepository` 的服务端落地）。
///
/// 端口是**同步**且**无鉴权维度**的（条目按 `host:port` 键，TOFU 语义）；
/// 组织隔离由宿主按「每组织一个仓储实例」实现——`organization_id` 列是
/// 实例的隔离边界，不是端口参数（形状差异见端口模块头）。
const V2_KNOWN_HOSTS: Migration = Migration {
    version: 2,
    name: "known_hosts baseline",
    expand: r#"
CREATE TABLE known_hosts (
  organization_id TEXT NOT NULL,
  host_key TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  algorithm TEXT NOT NULL,
  public_key TEXT NOT NULL,
  PRIMARY KEY (organization_id, host_key)
);
"#,
};

/// v3：任务受理与状态推进表（`JobRepository` 的服务端落地）。
///
/// 列是共享端口落盘词汇（`JobDefinition` / `JobView` / `JobClaim` /
/// `StageRecord` / `Checkpoint` / `CommitBoundary`）的 SQLite 投影，不变量
/// 镜像 [persistence-model §3.6](../../../../../docs/architecture/platform/persistence-model.md)
/// 的 DDL 约束（PostgreSQL 目标 schema 的 SQLite 等价形——选型差异已在
/// `storage/mod.rs` 登记为「首版 SQLite、生产化切片换 PostgreSQL」）：
///
/// - 复合主键 `(organization_id, job_id)`：组织隔离在存储层就是硬边界
///   （CM-05）；`JobDefinition.organization_id` 与行上的组织列在受理时
///   一起落库，读取按 `ctx` 的组织限定。
/// - `payload_json` 是冻结计划正文（§3.6 `jobs.plan`：目标、指纹、映射、
///   事务要求；敏感内容只允许 `SecretRef` 引用）。
/// - `consumed_plan_id` 建立部分唯一索引：同一计划在同一组织内只能被一个
///   apply Job 消费（§3.6 `uq_jobs_apply_plan`；失败也不释放消费资格）。
/// - `idem_scope_key`（`org|principal|key`）+ `accept_fingerprint` 落同一张
///   行上：幂等重放按键取回原 Job，同键不同指纹判 `IdempotencyConflict`。
/// - claim 三元组（worker / expires / generation）镜像 §3.6 的调度租约：
///   终态必须成对清空（`ck_jobs_claim`），running 必须有 worker
///   （`ck_jobs_claim_live`）；`claim_stage_id` / `claimed_at` 是端口
///   `JobClaim` 词汇的补列（PostgreSQL 目标 schema 无此二列，届时合同迁移补齐）。
/// - 阶段、checkpoint、提交边界各自成表：checkpoint 以
///   `(organization_id, job_id, checkpoint_version)` 为键（§3.6），版本即
///   写入时的 `Checkpoint.state_version`（与内核内存实现同口径）；边界按
///   产生顺序递增（§3.6 `job_checkpoints.committed_boundary` 的 durable 投影）。
const V3_JOBS: Migration = Migration {
    version: 3,
    name: "jobs baseline",
    expand: r#"
CREATE TABLE jobs (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('queued','running','succeeded','failed','cancelled')),
  state_version INTEGER NOT NULL CHECK (state_version > 0),
  stage TEXT,
  owner_json TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  execution_ids_json TEXT NOT NULL DEFAULT '[]',
  artifact_ids_json TEXT NOT NULL DEFAULT '[]',
  effect_outcome TEXT,
  cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
  pending_verification_reason TEXT,
  error TEXT,
  progress_json TEXT NOT NULL DEFAULT '{}',
  worker_id TEXT,
  claim_stage_id TEXT,
  claimed_at TEXT,
  claim_expires_at TEXT,
  claim_generation INTEGER NOT NULL DEFAULT 0 CHECK (claim_generation >= 0),
  consumed_plan_id TEXT,
  idem_scope_key TEXT,
  accept_fingerprint TEXT,
  PRIMARY KEY (organization_id, job_id),
  CONSTRAINT ck_jobs_claim_pair CHECK (
    (worker_id IS NULL) = (claim_expires_at IS NULL)
    AND (worker_id IS NULL) = (claim_stage_id IS NULL)
    AND (worker_id IS NULL) = (claimed_at IS NULL)
  ),
  CONSTRAINT ck_jobs_claim_live CHECK (state <> 'running' OR worker_id IS NOT NULL)
);
CREATE UNIQUE INDEX uq_jobs_idem
  ON jobs (organization_id, idem_scope_key)
  WHERE idem_scope_key IS NOT NULL;
CREATE UNIQUE INDEX uq_jobs_consumed_plan
  ON jobs (organization_id, consumed_plan_id)
  WHERE consumed_plan_id IS NOT NULL;
CREATE INDEX idx_jobs_list
  ON jobs (organization_id, created_at, job_id);

CREATE TABLE job_stages (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  stage_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  claimed_by TEXT,
  execution_ids_json TEXT NOT NULL DEFAULT '[]',
  started_at TEXT,
  finished_at TEXT,
  PRIMARY KEY (organization_id, job_id, stage_id)
);

CREATE TABLE job_checkpoints (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  checkpoint_version INTEGER NOT NULL CHECK (checkpoint_version > 0),
  stable_target_fingerprint TEXT NOT NULL,
  committed_json TEXT NOT NULL,
  verification_evidence_json TEXT NOT NULL,
  recovery_policy TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, job_id, checkpoint_version)
);

CREATE TABLE job_commit_boundaries (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  boundary_index INTEGER NOT NULL CHECK (boundary_index >= 0),
  boundary_json TEXT NOT NULL,
  PRIMARY KEY (organization_id, job_id, boundary_index)
);
"#,
};

/// v4：结果产物元数据表（`ArtifactRepository` 的管理库半边；字节半边是
/// 构造时给定的受控目录）。
///
/// 列是共享端口落盘词汇（`ArtifactSpec` / `ArtifactMetadata` / `Record` 的
/// 持久化字段）的 SQLite 投影。[persistence-model §3.8](../../../../../docs/architecture/platform/persistence-model.md)
/// 的 PostgreSQL 目标 schema 还带 `connection_id` / `job_id` / `content_digest`
/// / `visibility` 等服务端扩展列——本切片先落端口的合同词汇，扩展列随
/// PostgreSQL 生产化切片的合同迁移补齐（与 v3 的登记口径一致）：
///
/// - 复合主键 `(organization_id, artifact_id)`：组织隔离在存储层就是硬边界
///   （CM-05）；owner 绑定在 create 时冻结（ACL 随创建走）。
/// - `chunk_sizes_json` 是已发布块的字节长度序列（发布即追加，只增不减）——
///   `read_range` 的偏移换算与恢复核验都以它为准。
/// - `writer_lease` 是进程内 writer 句柄的落库投影：丢弃句柄由恢复扫描标记
///   truncated，writer 能力永不跨重启存活（与内核 `FileArtifactRepository`
///   的 `Record.writer` 同语义）。`ck_artifact_final` 把「尚未终结」
///   （pending）钉在 writing/revoked/expired 三个生命周期上——写入中被吊销
///   或到期仍是未终结产物，与 §3.8 的 `finalized_at IS NULL` 同口径。
/// - `expires_epoch` 是 TTL 到期的 Unix 秒；`expired_artifact` 墓碑表让
///   已删除产物在一小时宽限期内继续回答 `ArtifactExpired`（不是 `NotFound`），
///   与内核 `FileArtifactRepository` 的内存墓碑同语义。
/// - 字节与元数据之间的故障窗口由「先写块文件、后事务提交元数据」关闭：
///   崩溃留下的孤儿块由打开期恢复扫描回收（§3.8 验收口径）。
const V4_ARTIFACTS: Migration = Migration {
    version: 4,
    name: "artifact metadata baseline",
    expand: r#"
CREATE TABLE artifact_metadata (
  organization_id TEXT NOT NULL,
  artifact_id TEXT NOT NULL,
  owner_principal_id TEXT NOT NULL,
  execution_id TEXT NOT NULL,
  delegated_execution INTEGER NOT NULL DEFAULT 0 CHECK (delegated_execution IN (0, 1)),
  target_fingerprint TEXT NOT NULL,
  expires_epoch INTEGER NOT NULL CHECK (expires_epoch >= 0),
  lifecycle TEXT NOT NULL
    CHECK (lifecycle IN ('writing','complete','truncated','revoked','expired')),
  published_chunk_count INTEGER NOT NULL DEFAULT 0 CHECK (published_chunk_count >= 0),
  published_byte_size INTEGER NOT NULL DEFAULT 0 CHECK (published_byte_size >= 0),
  total_chunks INTEGER,
  result_completeness TEXT NOT NULL
    CHECK (result_completeness IN ('pending','complete','truncated')),
  truncation_reason TEXT,
  chunk_sizes_json TEXT NOT NULL DEFAULT '[]',
  writer_lease TEXT,
  created_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, artifact_id),
  CONSTRAINT ck_artifact_final CHECK (
    (result_completeness = 'pending') = (lifecycle IN ('writing','revoked','expired'))
  )
);
CREATE INDEX idx_artifact_expiry
  ON artifact_metadata (organization_id, expires_epoch);

CREATE TABLE expired_artifact (
  artifact_id TEXT PRIMARY KEY,
  organization_id TEXT NOT NULL,
  owner_principal_id TEXT NOT NULL,
  expires_epoch INTEGER NOT NULL
);
"#,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_ascending_without_gaps() {
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                migration.version,
                (index as i64) + 1,
                "migration versions must be 1..=n without gaps"
            );
            assert!(!migration.name.is_empty());
            assert!(!migration.expand.trim().is_empty());
        }
    }
}