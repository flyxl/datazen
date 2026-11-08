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

/// v5：认证与授权四表（WR-D3 第一刀；`LoginSessions` / `PolicyService` 的
/// 服务端落地）。
///
/// 列是 [persistence-model §3.10](../../../../../docs/architecture/platform/persistence-model.md)
/// PostgreSQL 目标 schema 的 SQLite 等价形（与 v3/v4 同款登记口径）：
///
/// - `TIMESTAMPTZ → TEXT`：全部时刻列落**定宽 RFC3339 UTC（毫秒）**字符串
///   （与 `shared::clock` 的单一形状同一口径）；适配器在边界处统一规范化。
///   定宽是字典序比较可靠的前提——`ck_login_window` 的 `idle <= absolute`
///   在 SQLite 里就是字符串比较。
/// - `BOOLEAN → INTEGER 0/1` + `CHECK IN (0,1)`（v1 同款）。
/// - 表顺序钉住 §3.10 的可执行裁决：`memberships` 先于 `login_sessions`
///   ——后者的复合外键引用 `(organization_id, principal_id)`；同一迁移
///   事务内后建的表不能作为外键目标（§5.5 fail-closed，起不来即暴露）。
/// - **没有 `roles` 表**：六个固定角色由 `ck_membership_role` 的 CHECK
///   表达（team-server §6.1：首版不可由管理员自定义；P9 引入自定义角色时
///   再补表与外键）。
/// - `acl_entries.resource_id` 用空串表示**组织级**条目（主键列不能为
///   NULL）；`ck_acl_resource` 不含 `wapp`（persistence-model §3.10 的
///   CHECK 权威——与 team-server §6.1 词表的一处登记差异，wapp 安装闸走
///   角色基线，ACL 行不可落 wapp）。
/// - `permission_versions` 每组织一行、只增不减、永不删除；授权变更与
///   版本递增同事务（team-server §6.3），由 `policy::version` 的事务
///   helper 保证，DDL 只兜底 `>= 1`。
/// - 部分索引照抄：`idx_memberships_role ... WHERE enabled`、
///   `idx_login_sessions_owner / _sweep ... WHERE revoked_at IS NULL`。
/// - P8 的 `auth_mode/native_secret_hash` 已随本迁移一次给全（§3.10 注记
///   允许 expand 提前）：cookie 会话 hash 为 NULL，native 仅存 secret 的
///   HMAC 摘要；本刀不启用 native 路径（推迟项，见模块报告）。
const V5_AUTHZ_BASELINE: Migration = Migration {
    version: 5,
    name: "authz baseline (memberships/login_sessions/acl_entries/permission_versions)",
    expand: r#"
CREATE TABLE memberships (
  organization_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  role_id TEXT NOT NULL
    CHECK (role_id IN ('orgAdmin','orgMember','connectionOwner','connectionViewer','jobOperator','auditor')),
  external_subject TEXT NOT NULL CHECK (external_subject <> ''),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, principal_id)
);
CREATE INDEX idx_memberships_role
  ON memberships (organization_id, role_id) WHERE enabled;

CREATE TABLE login_sessions (
  organization_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  auth_time TEXT NOT NULL,
  issued_at TEXT NOT NULL,
  absolute_expires_at TEXT NOT NULL,
  idle_expires_at TEXT NOT NULL,
  reauth_required INTEGER NOT NULL DEFAULT 0 CHECK (reauth_required IN (0, 1)),
  permission_version INTEGER NOT NULL CHECK (permission_version >= 1),
  client_binding TEXT NOT NULL,
  token_epoch INTEGER NOT NULL DEFAULT 1 CHECK (token_epoch >= 1),
  auth_mode TEXT NOT NULL DEFAULT 'cookie' CHECK (auth_mode IN ('cookie','native')),
  native_secret_hash TEXT,
  revoked_at TEXT,
  revoke_reason TEXT,
  PRIMARY KEY (organization_id, session_id),
  FOREIGN KEY (organization_id, principal_id)
    REFERENCES memberships (organization_id, principal_id),
  CONSTRAINT ck_login_native
    CHECK ((auth_mode = 'native') = (native_secret_hash IS NOT NULL)),
  CONSTRAINT ck_login_window CHECK (idle_expires_at <= absolute_expires_at),
  CONSTRAINT ck_login_revoke
    CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL)),
  CONSTRAINT ck_login_reason
    CHECK (revoke_reason IS NULL OR revoke_reason IN ('logout','permission_revoked','admin','credential_rotated'))
);
CREATE INDEX idx_login_sessions_owner
  ON login_sessions (organization_id, principal_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_login_sessions_sweep
  ON login_sessions (idle_expires_at) WHERE revoked_at IS NULL;

CREATE TABLE acl_entries (
  organization_id TEXT NOT NULL,
  resource_type TEXT NOT NULL
    CHECK (resource_type IN ('connection','session','execution','job','artifact','stream')),
  resource_id TEXT NOT NULL DEFAULT '',
  subject_type TEXT NOT NULL CHECK (subject_type IN ('user','serviceAccount','delegated')),
  subject_id TEXT NOT NULL,
  action TEXT NOT NULL
    CHECK (action IN ('read','execute','write','cancel','download','subscribe','admin')),
  deny INTEGER NOT NULL DEFAULT 0 CHECK (deny IN (0, 1)),
  created_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, resource_type, resource_id, subject_type, subject_id, action)
);

CREATE TABLE permission_versions (
  organization_id TEXT NOT NULL PRIMARY KEY,
  permission_version INTEGER NOT NULL DEFAULT 1 CHECK (permission_version >= 1),
  updated_at TEXT NOT NULL
);
"#,
};

/// v6：审计事件表（WR-D3 第一刀；`ServerAudit`（`AuditSink` 端口）的落库
/// 半边）。
///
/// [persistence-model §3.7](../../../../../docs/architecture/platform/persistence-model.md)
/// 的 SQLite 等价形：
///
/// - `event_id BIGINT GENERATED ALWAYS AS IDENTITY → INTEGER PRIMARY KEY
///   AUTOINCREMENT`：同为**全局单调、永不复用**的序列；`event_id` 全局唯一
///   时 §3.7 的 `(organization_id, event_id)` 复合主键自动成立，不另设。
///   语义钉子：它只用于排序与分页，**不是** `EventEnvelope.sequence`。
/// - `JSONB → TEXT` JSON 投影（v3/v4 同款登记）；`payload` 只接受
///   persistence-model §2.4 白名单键，键集合在 `shared::audit::AuditPayload`
///   的类型与钉子测试里固定，写入路径没有「对象序列化顺带落库」的通道。
/// - **append-only 由代码路径保证**（PostgreSQL 目标形态由应用角色授权
///   保证：只授 INSERT/SELECT）：本 crate 对该表只有 INSERT 与 SELECT，
///   没有任何 UPDATE/DELETE 语句（`storage::audit` 的源文本钉子测试把这条
///   纪律钉在 CI 上）。
const V6_AUDIT_EVENTS: Migration = Migration {
    version: 6,
    name: "audit_events baseline (append-only)",
    expand: r#"
CREATE TABLE audit_events (
  organization_id TEXT NOT NULL,
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  occurred_at TEXT NOT NULL,
  principal_id TEXT,
  principal_kind TEXT NOT NULL CHECK (principal_kind IN ('user','service','delegation')),
  delegation_id TEXT,
  client_instance_id TEXT,
  action TEXT NOT NULL,
  connection_id TEXT,
  config_revision INTEGER,
  target_projection_json TEXT,
  outcome TEXT NOT NULL CHECK (outcome IN ('succeeded','failed','denied','unknown')),
  error_code TEXT,
  request_id TEXT,
  payload_json TEXT NOT NULL DEFAULT '{}',
  CONSTRAINT ck_audit_action CHECK (action <> '')
);
CREATE INDEX idx_audit_time
  ON audit_events (organization_id, occurred_at DESC);
CREATE INDEX idx_audit_target
  ON audit_events (organization_id, connection_id, occurred_at DESC);
"#,
};

/// 全部迁移，按版本升序。执行器假定有序且无空洞。
pub const MIGRATIONS: &[Migration] = &[
    V1_CONNECTION_PROFILE,
    V2_KNOWN_HOSTS,
    V3_JOBS,
    V4_ARTIFACTS,
    V5_AUTHZ_BASELINE,
    V6_AUDIT_EVENTS,
    V7_EXECUTION_WIRING,
    V8_SERVER_SETTINGS,
    V9_NAMED_VIEWS,
    V10_ORG_MEMORIES,
    V11_QUERY_ASSETS,
    V12_SCHEMA_DIFF_RUNTIME,
    V13_WORKFLOW_RUNTIME,
    V14_DASHBOARD_RUNTIME,
    V15_SYNC_ASSETS,
];

/// v10：组织记忆与审批审计（T25）。版本化行 `(organization_id, id,
/// version)`；`status='revoked'` 后读取面与检索面必须立即排除。审计表
/// 只记录 who/org/memory/version/decision，不记录内容（与 `audit_events`
/// 的 payload 白名单纪律一致）。
const V10_ORG_MEMORIES: Migration = Migration {
    version: 10,
    name: "organization memory approvals and audit",
    expand: r#"
CREATE TABLE org_memories (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version >= 1),
  kind TEXT NOT NULL,
  subject TEXT NOT NULL,
  content TEXT NOT NULL,
  confidence REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
  sensitivity TEXT NOT NULL CHECK (sensitivity IN ('normal','internal')),
  status TEXT NOT NULL CHECK (status IN ('active','revoked')),
  created_by TEXT NOT NULL,
  approved_by TEXT NOT NULL,
  published_at TEXT NOT NULL,
  revoked_at TEXT,
  revoked_by TEXT,
  PRIMARY KEY (organization_id, id, version)
);
CREATE INDEX idx_org_memories_active
  ON org_memories (organization_id, status, id, version);
CREATE TABLE org_memory_audit (
  id TEXT PRIMARY KEY NOT NULL,
  organization_id TEXT NOT NULL,
  memory_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('publish','revoke','inject')),
  actor_principal_id TEXT NOT NULL,
  decision TEXT NOT NULL CHECK (decision IN ('approved','revoked','injected','suppressed')),
  detail_json TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_org_memory_audit_org
  ON org_memory_audit (organization_id, created_at DESC);
"#,
};

/// v11 编号裁决：v10 已被 T25 组织记忆（#205）先行占用——本迁移重编号让位，DDL 与词汇零变化。
/// v11：查询资产双表（W8 刀3a——收藏与查询历史的服务端存储；契约
/// §6.12 的存储形态裁决——web 形态把「桌面收藏 = 用户目录一个 .sql 文件」
/// 与「桌面历史 = 本机 history.sqlite」都改存服务端库，词汇一条（共享层
/// `datazen::history::{FavoriteQuery, QueryHistoryEntry}`）存储两形态。
///
/// DDL 口径（v5 起的登记纪律 + v9 先例）：
///
/// - **per (organization_id, principal_id) 隔离**：列在行上，路由的
///   scoped store 按认证链事实装配（INV-01：组织与主体不来自请求体）；
///   同组织两个主体各自持有自己的收藏/历史集合，越界读取天然落空
///   （CM-05：读越界空）。
/// - **id 服务端铸造**（uuid v4，CSPRNG，全局唯一）单列主键——收藏创建
///   与历史落账都由服务端铸 id，客户端不提供。
/// - **投影列显式命名**：`sql_text`（`sql` 不是保留字，但与共享词汇
///   `sql` 字段对应的列名避开歧义面）；`database_name` / `schema_name`
///   沿 named_views 命名口径。
/// - **历史保留策略**：追加后按 (org, principal) 截断到
///   `MAX_QUERY_HISTORY`（1000 行，共享层 scope.rs 常量；由存储实现
///   执行，DELETE 多于 1000 的旧行——与桌面 history_db 同一条业务规则）。
/// - **TIMESTAMPTZ → TEXT**（定宽 RFC3339 UTC 毫秒，v5 起口径；
///   `executed_at` 由服务端铸造时刻，`rows_affected` 可空）。
const V11_QUERY_ASSETS: Migration = Migration {
    version: 11,
    name: "query assets baseline (per-principal favorites + query history)",
    expand: r#"
CREATE TABLE query_favorites (
  id TEXT NOT NULL PRIMARY KEY,
  organization_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  connection_id TEXT NOT NULL CHECK (connection_id <> ''),
  title TEXT NOT NULL CHECK (title <> ''),
  sql_text TEXT NOT NULL,
  database_name TEXT CHECK (database_name IS NULL OR database_name <> ''),
  keyword TEXT CHECK (keyword IS NULL OR keyword <> ''),
  folder TEXT CHECK (folder IS NULL OR folder <> ''),
  created_at TEXT NOT NULL,
  updated_at TEXT CHECK (updated_at IS NULL OR updated_at <> '')
);
CREATE INDEX idx_query_favorites_owner
  ON query_favorites (organization_id, principal_id, connection_id);

CREATE TABLE query_history (
  id TEXT NOT NULL PRIMARY KEY,
  organization_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  connection_id TEXT NOT NULL CHECK (connection_id <> ''),
  database_name TEXT NOT NULL,
  schema_name TEXT CHECK (schema_name IS NULL),
  sql_text TEXT NOT NULL,
  executed_at TEXT NOT NULL,
  execution_time_ms INTEGER NOT NULL CHECK (execution_time_ms >= 0),
  rows_affected INTEGER CHECK (rows_affected IS NULL OR rows_affected >= 0),
  success INTEGER NOT NULL CHECK (success IN (0, 1)),
  error_message TEXT
);
CREATE INDEX idx_query_history_owner
  ON query_history (organization_id, principal_id, executed_at);
"#,
};

/// v8：服务端设置存储（`SettingsRepository` 的服务端落地；契约 §6.1 的
/// settings 读侧面）。
///
/// 形状裁决（storage/settings 模块头登记的取舍，此处只记 DDL 口径）：
///
/// - **per-section 行**：每个组织每节一行（`section` = `SharedSettings`
///   序列化后的顶层 wire 键：`preferences` / `execution` / `monitor` /
///   `onboarding` / `rowCountMode`），`settings_json` 是该节的 JSON 投影。
///   整档覆盖写（last-write-wins，契约 §6.1：不要求幂等令牌）按节重写，
///   `updated_at` 让「哪节何时被谁改过」可观测（审计面随 WR-G 切片接入）。
/// - 复合主键 `(organization_id, section)`：设置是**组织级**事实（CM-05
///   同款硬边界）；越组织读取天然落空。
/// - 缺行 ≠ 错误：读取按节补缺省（缺省文档在 `storage::settings` 里物化，
///   镜像前端 `DEFAULT_SETTINGS` 的共享投影——fresh install 语义，与桌面
///   `AppSettings::default_for_first_run` 同位）。
/// - `TIMESTAMPTZ → TEXT`（定宽 RFC3339 UTC 毫秒，v5 起的登记口径）。
const V8_SERVER_SETTINGS: Migration = Migration {
    version: 8,
    name: "server_settings baseline (per-section settings rows)",
    expand: r#"
CREATE TABLE server_settings (
  organization_id TEXT NOT NULL,
  section TEXT NOT NULL CHECK (section <> ''),
  settings_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, section)
);
CREATE INDEX idx_server_settings_org
  ON server_settings (organization_id, updated_at);
"#,
};

/// v9：命名视图表（`NamedViewRepository` 的服务端落地；契约 §6.9 的存储
/// 形态裁决——web 形态把「桌面存本机 named_views.json」改存服务端库，
/// 端口形状零改动）。
///
/// DDL 口径（v5 起的登记纪律）：
///
/// - **端口是 whole-collection**（load/save 整集合替换），组织与主体隔离
///   由适配器按「每 (organization_id, principal_id) 一个实例」实现（
///   settings/known-hosts 同款口径）：列在行上，端口参数不带。§6.9 的
///   归属语义是 **per principal + scope**——同一组织的两个主体各自持有
///   自己的视图集合，越界读取天然落空（CM-05：读越界空/None）。
/// - `id TEXT PRIMARY KEY` 单列主键：id 由服务端铸造（uuid v4，CSPRNG，
///   全局唯一），跨主体的主键碰撞即适配器的**写越界拒绝**信号（INSERT
///   撞键 → `NamedViewError::Storage`，绝不静默改写他人行）。
/// - **scope 投影列**（`connection_id` / `database_name` / `schema_name` /
///   `table_name`）镜像 `NamedViewScope`：`connection_id` NOT NULL——
///   scope 绑定**持久 connectionId**（§6.9 明文：`dbSessionId` 是运行时
///   状态，永不入 scope，本表没有也不允许有会话列）；`database` /
///   `schema` 可空（引擎无该层级时为 NULL），空串与缺失同义禁止（CHECK）。
///   投影列只服务可观测与查询，**权威是 `view_json`**（整条 `NamedView`
///   的 JSON 投影，v3/v4/v7 同款 JSONB→TEXT 口径）。
/// - `TIMESTAMPTZ → TEXT`（定宽 RFC3339 UTC 毫秒；`updated_at` 可空——
///   与 `NamedView.updated_at: Option` 同形，从未改过的记录为 NULL）。
const V9_NAMED_VIEWS: Migration = Migration {
    version: 9,
    name: "named_views baseline (per-principal saved views)",
    expand: r#"
CREATE TABLE named_views (
  id TEXT NOT NULL PRIMARY KEY,
  organization_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  connection_id TEXT NOT NULL CHECK (connection_id <> ''),
  database_name TEXT CHECK (database_name IS NULL OR database_name <> ''),
  schema_name TEXT CHECK (schema_name IS NULL OR schema_name <> ''),
  table_name TEXT NOT NULL CHECK (table_name <> ''),
  view_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT CHECK (updated_at IS NULL OR updated_at <> '')
);
CREATE INDEX idx_named_views_scope
  ON named_views (organization_id, principal_id, connection_id, table_name);
"#,
};

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

/// v12：Schema Diff 运行时三伴表 + 迁移历史 + Schema Diff profile（W8
/// 刀3b——schema_diff 域服务端化的存储底座）。
///
/// 桌面对照（`datazen-desktop/src/store/app_db.rs` 的 `JOB_SCHEMA_V2`）
/// 与逐表口径：
///
/// - **job 运行时三伴表**：桌面 `job_result_details` / `job_domain_results`
///   / `job_artifact_refs` 的服务端投影——主键全部升为
///   `(organization_id, …)` 复合键（组织隔离在存储层就是硬边界，CM-05；
///   v3 jobs 家族同款）。桌面 `job_idempotency_receipts` **不移植**：
///   服务端受理行自持幂等（v3 jobs 表的 `idem_scope_key` 部分唯一索引
///   `uq_jobs_idem`），`receipt_for` 直接查受理行，不再开表。
/// - **migration_run_history**：桌面 `history_db/schema.rs` 同名表的服务端
///   投影（PK 升 `(organization_id, id)`）——共享词汇
///   `datazen::history::domain::migration_run::MigrationRunRecord` 的 18 列
///   一一对应；`started_at DESC` 与 `profile_id` 两索引随行。
/// - **schema_diff_profiles**：桌面是加密 JSON 文件
///   （`schema_diff_profiles.enc`，单租户），web 形态改存服务端库行
///   （查询资产 v11 同款裁决：文件形的服务端对偶）——共享词汇
///   `datazen::migration::schema_diff::profile::SchemaDiffProfile`；
///   per organization 隔离（profile 是「连接对」级别的组织资产，桌面
///   单租户全体共享，服务端按组织收敛）；集合列落 JSON 投影
///   （`tables_json` 等，v1 connection_profile 同款投影列口径）。
/// - **TIMESTAMPTZ → TEXT**（定宽 RFC3339 UTC，v5 起口径）。
const V12_SCHEMA_DIFF_RUNTIME: Migration = Migration {
    version: 12,
    name: "schema diff runtime (job details/results/artifacts + migration run history + profiles)",
    expand: r#"
CREATE TABLE job_result_details (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  plan_id TEXT,
  plan_digest TEXT,
  selection_revision INTEGER,
  recovery_json TEXT,
  PRIMARY KEY (organization_id, job_id)
);

CREATE TABLE job_domain_results (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  stage_id TEXT NOT NULL,
  result_json TEXT NOT NULL,
  PRIMARY KEY (organization_id, job_id, stage_id)
);

CREATE TABLE job_artifact_refs (
  organization_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  artifact_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, job_id, artifact_id)
);
CREATE INDEX idx_job_artifact_expiry
  ON job_artifact_refs (organization_id, expires_at);

CREATE TABLE migration_run_history (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK (operation <> ''),
  status TEXT NOT NULL CHECK (status <> ''),
  outcome TEXT NOT NULL CHECK (outcome <> ''),
  phase TEXT NOT NULL CHECK (phase <> ''),
  profile_id TEXT,
  profile_revision TEXT,
  source_connection_id TEXT,
  target_connection_id TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  selected_count INTEGER NOT NULL DEFAULT 0 CHECK (selected_count >= 0),
  committed_count INTEGER NOT NULL DEFAULT 0 CHECK (committed_count >= 0),
  failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
  conflict_count INTEGER NOT NULL DEFAULT 0 CHECK (conflict_count >= 0),
  cancelled INTEGER NOT NULL DEFAULT 0 CHECK (cancelled IN (0, 1)),
  rollback_outcome TEXT NOT NULL DEFAULT 'notRequired',
  error_summary TEXT,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_migration_run_started
  ON migration_run_history (organization_id, started_at);
CREATE INDEX idx_migration_run_profile
  ON migration_run_history (organization_id, profile_id);

CREATE TABLE schema_diff_profiles (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version >= 1),
  name TEXT NOT NULL CHECK (name <> ''),
  source_connection_id TEXT NOT NULL CHECK (source_connection_id <> ''),
  target_connection_id TEXT NOT NULL CHECK (target_connection_id <> ''),
  source_database TEXT NOT NULL CHECK (source_database <> ''),
  target_database TEXT NOT NULL CHECK (target_database <> ''),
  source_schema TEXT,
  target_schema TEXT,
  target_only_tables_json TEXT NOT NULL DEFAULT '[]',
  tables_json TEXT NOT NULL DEFAULT '[]',
  source_objects_json TEXT NOT NULL DEFAULT '[]',
  target_objects_json TEXT NOT NULL DEFAULT '[]',
  allow_destructive INTEGER NOT NULL DEFAULT 0 CHECK (allow_destructive IN (0, 1)),
  include_indexes INTEGER NOT NULL DEFAULT 1 CHECK (include_indexes IN (0, 1)),
  require_rollback INTEGER NOT NULL DEFAULT 0 CHECK (require_rollback IN (0, 1)),
  type_overrides_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
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

/// v7：WR-D2 收尾（连接目录 / 隧道定义）与 WR-D4 执行接线（秘密材料 /
/// 服务身份）四表。expansion-only 追加，v1–v6 不动。
///
/// DDL 风格沿 §3.10（复合主键 `organization_id` 打头、CHECK 约束、部分索引、
/// expand-only）；PostgreSQL 等价形登记沿 v3/v4/v5 口径
/// （[persistence-model §3](../../../../../docs/architecture/platform/persistence-model.md)
/// 未覆盖这四张表，差异已在本刀模块报告登记）：
///
/// - `TIMESTAMPTZ → TEXT`（定宽 RFC3339 UTC 毫秒，`shared::clock` 同一口径）；
///   `BOOLEAN → INTEGER 0/1` + `CHECK IN (0,1)`（v1 同款）。
/// - **`stored_secret` / `stored_secret_version`**（`SecretProvider` 的服务端
///   落地）：行内只有**密文**（HMAC-SHA256 KDF 派生密钥的 sealed box：
///   nonce + 密文 + 完整性 tag 三列，全部 base64），明文永不入库——与
///   v1 `secret_ref` 的「只存引用」纪律一致，材料这半边由本迁移补齐。
///   版本行按 `(organization_id, secret_ref, revision)` 建键；
///   `read_versioned` 的历史回读（team-server §10.2）由旧版本行承接，
///   `rotate` 删除全部旧版本行实现「旧版本材料立即失效」。
/// - **`connection_catalog` / `connection_catalog_group`**（`ConnectionRepository`
///   的服务端落地）：表单词汇（`ConnectionConfig`）的**净化投影**落
///   `config_json`——敏感字段（`password` / `passphrase` / `authToken`，
///   含 ssh `jump` 嵌套）在序列化前置 NULL，明文进 `stored_secret` 的
///   sealed box，行内只留 `secret_ref` 引用（与 §3.3 `connections` 的
///   `secret_ref` 列语义同形；§3.3 是 `ProfileRepository` 的目标投影，
///   表单词汇的回显存储是另一张表，聚合边界见模块报告）。
///   `sort_order` 承载 `reorder` 的用户排序；`tunnel_id` 投影列让
///   `usage` 反向投影成为一次索引查询而不是 JSON 扫面；`group_name`
///   投影列同理（§4.1 桌面 `groups.json` 的服务端等价物）。
/// - **`tunnel_definition`**（`TunnelRepository` 的服务端落地）：`kind`
///   的 CHECK 取值即 driver-api `TunnelKind` 的三个持久化字面量
///   （`none` 不是持久化形态，SavedTunnel 契约排除它）；`config_json`
///   同款净化投影 + `secret_ref` 引用。
/// - **`execution_service_identity`**（`IdentityResolver` 委托路径的服务端
///   落地）：`workflow grant` 的 `service_principal` 词汇
///   （`crates/datazen/src/workflow/domain/grant.rs`）按
///   `(organization_id, delegation_id)` 登记，`scope` / `target_connection_id`
///   / `expires_at` 是 CM-06 的最小作用域事实；`uq_..._principal` 部分唯一
///   索引保证**一个服务身份至多一份活跃授权**（重发即换 delegation_id，
///   旧行保留 revoked=1 供审计）。
const V7_EXECUTION_WIRING: Migration = Migration {
    version: 7,
    name: "execution wiring baseline (stored_secret/connection_catalog/tunnel_definition/service_identity)",
    expand: r#"
CREATE TABLE stored_secret (
  organization_id TEXT NOT NULL,
  secret_ref TEXT NOT NULL,
  current_revision INTEGER NOT NULL CHECK (current_revision >= 1),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, secret_ref)
);

CREATE TABLE stored_secret_version (
  organization_id TEXT NOT NULL,
  secret_ref TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 1),
  nonce_b64 TEXT NOT NULL,
  ciphertext_b64 TEXT NOT NULL,
  tag_b64 TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, secret_ref, revision),
  FOREIGN KEY (organization_id, secret_ref)
    REFERENCES stored_secret (organization_id, secret_ref) ON DELETE CASCADE
);

CREATE TABLE connection_catalog (
  organization_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  name TEXT NOT NULL,
  group_name TEXT,
  tunnel_id TEXT,
  secret_ref TEXT,
  config_json TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
  last_connected_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, connection_id)
);
CREATE INDEX idx_connection_catalog_order
  ON connection_catalog (organization_id, sort_order, connection_id);
CREATE INDEX idx_connection_catalog_tunnel
  ON connection_catalog (organization_id, tunnel_id) WHERE tunnel_id IS NOT NULL;

CREATE TABLE connection_catalog_group (
  organization_id TEXT NOT NULL,
  group_name TEXT NOT NULL CHECK (group_name <> ''),
  created_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, group_name)
);

CREATE TABLE tunnel_definition (
  organization_id TEXT NOT NULL,
  tunnel_id TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('ssh','httpProxy','websocket')),
  secret_ref TEXT,
  config_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, tunnel_id)
);

CREATE TABLE execution_service_identity (
  organization_id TEXT NOT NULL,
  delegation_id TEXT NOT NULL,
  service_principal TEXT NOT NULL CHECK (service_principal <> ''),
  scope TEXT NOT NULL,
  target_connection_id TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  revoked INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0, 1)),
  approved_by TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, delegation_id)
);
CREATE UNIQUE INDEX uq_execution_service_identity_principal
  ON execution_service_identity (organization_id, service_principal)
  WHERE revoked = 0;
"#,
};

/// v13：Workflow 运行时双表（W8 刀3c-1 存储底座）。
///
/// 桌面 `AppDb.workflows` / `HistoryDb.workflow_history` 同名表的组织键投影
/// （桌面是单租户本地库，本表把 `organization_id` 提进复合主键——v9 起
/// 的既定口径）：
///
/// - **`workflows`**：定义整档以 YAML 文本落库（与桌面同形——「保存的
///   工作流就是用户在编辑器里写的那份文档」；解析/校验/列表投影的规则
///   在共享层 `workflow::registry`，本表只存行）。`visibility` 双态：
///   `user`（用户可见）与 `dashboardHidden`（仪表板私有的隐藏工作流，
///   列表不出现）。服务端**不播内置样例**（桌面的三个 builtin 是单机
///   onboarding 供给；团队服务器的组织自建，形态差异在台账登记）。
/// - **`workflow_run_history`**：运行记录整档 JSON（结果不投影成列——
///   步骤产物形状随执行器生长，投影就要跟着迁移；桌面同款裁决）。
///   `result_json` 反序列化失败按行损坏处理（读取面跳过并 warn，不炸
///   整个列表）。保留策略：追加后按组织截断到 `MAX_WORKFLOW_HISTORY`
///   （100，共享层常量；同一条业务规则的存储侧执行）。
/// - **删除语义**：`workflows` 行删除**不受** dashboard 引用闸约束——
///   服务端 dashboard 域（刀3d）落库时补 `workflow_id` 引用核对（桌面
///   `WorkflowInUse` 的对偶）；当前服务端无 dashboard 存储，无引用可核。
const V13_WORKFLOW_RUNTIME: Migration = Migration {
    version: 13,
    name: "workflow runtime (definitions + run history)",
    expand: r#"
CREATE TABLE workflows (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  name TEXT NOT NULL CHECK (name <> ''),
  description TEXT NOT NULL DEFAULT '',
  visibility TEXT NOT NULL DEFAULT 'user'
    CHECK (visibility IN ('user', 'dashboardHidden')),
  definition_yaml TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_workflows_visibility
  ON workflows (organization_id, visibility);

CREATE TABLE workflow_run_history (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  workflow_id TEXT NOT NULL CHECK (workflow_id <> ''),
  workflow_name TEXT NOT NULL,
  variables_json TEXT NOT NULL,
  result_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_workflow_run_history_flow
  ON workflow_run_history (organization_id, workflow_id, created_at DESC);
"#,
};

/// v14：Dashboard 运行时（W8 刀3d-1）。桌面 `AppDb` 五表的组织键投影
/// （桌面单租户无组织列，服务端把 `organization_id` 提进复合主键——
/// v11 查询资产/v13 workflow 同款裁决）：
/// - `dashboards` / `widgets`：行词汇 = 共享层 `dashboard::records`
///   （`DashboardRecord` / `WidgetRecord`）；
/// - `widget_runs` + `widget_latest_run`：运行历史与保留策略的伴表
///   （latest 让 days 清理不误删最新一条——桌面同款）；
/// - `monitor_schedule_grants`：monitor 授权单元数据（整档 JSON，桌面
///   v3 同形）。
const V14_DASHBOARD_RUNTIME: Migration = Migration {
    version: 14,
    name: "dashboard runtime (dashboards, widgets, widget runs, monitor grants)",
    expand: r#"
CREATE TABLE dashboards (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  name TEXT NOT NULL CHECK (name <> ''),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  layout_cols INTEGER NOT NULL DEFAULT 12,
  layout_row_height INTEGER NOT NULL DEFAULT 80,
  enabled INTEGER NOT NULL DEFAULT 1,
  refresh_paused INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (organization_id, id)
);

CREATE TABLE widgets (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  dashboard_id TEXT NOT NULL CHECK (dashboard_id <> ''),
  title TEXT NOT NULL,
  workflow_id TEXT NOT NULL CHECK (workflow_id <> ''),
  view_mode TEXT NOT NULL CHECK (view_mode IN ('chart', 'table')),
  chart_config_json TEXT,
  layout_x INTEGER NOT NULL,
  layout_y INTEGER NOT NULL,
  layout_w INTEGER NOT NULL,
  layout_h INTEGER NOT NULL,
  refresh_mode TEXT NOT NULL
    CHECK (refresh_mode IN ('manual', 'onOpen', 'interval')),
  refresh_sec INTEGER,
  alert_json TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_widgets_dashboard
  ON widgets (organization_id, dashboard_id, sort_order);
CREATE INDEX idx_widgets_workflow
  ON widgets (organization_id, workflow_id);

CREATE TABLE widget_runs (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  dashboard_id TEXT NOT NULL,
  widget_id TEXT NOT NULL,
  workflow_id TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('ok', 'error', 'timeout')),
  error TEXT,
  row_count INTEGER NOT NULL DEFAULT 0,
  columns_json TEXT NOT NULL,
  rows_json TEXT NOT NULL,
  variables_json TEXT,
  alert_fired INTEGER,
  alert_value REAL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_widget_runs_widget_started
  ON widget_runs (organization_id, widget_id, started_at DESC);

CREATE TABLE widget_latest_run (
  organization_id TEXT NOT NULL,
  widget_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  started_at TEXT NOT NULL,
  status TEXT NOT NULL,
  PRIMARY KEY (organization_id, widget_id)
);

CREATE TABLE monitor_schedule_grants (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  dashboard_id TEXT NOT NULL,
  widget_id TEXT NOT NULL,
  metadata_json TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_monitor_grant_widget
  ON monitor_schedule_grants (organization_id, dashboard_id, widget_id);
"#,
};

/// v15：Data Sync 双表（W8 刀3e-2——同步任务断点与复用档案的服务端
/// 存储；契约 §6.16 的存储底座）。桌面形是两个 JSON 文件
/// （`sync_tasks.json` / `sync_profiles.json`，单租户全体共享），web 形态
/// 改存服务端库行（v11 查询资产「文件形的服务端对偶」同款裁决）。
///
/// DDL 口径（v5 起登记纪律 + v11/v12 先例）：
///
/// - **per (organization_id, id) 复合主键**：任务与档案都是组织级资产
///   （桌面单租户不区分归属；服务端按组织收敛，CM-05：越界读空、写只落
///   本边界）。隔离键由调用方按认证链事实装配（INV-01）。
/// - **词汇单家**：`sync_profiles` 行 = 共享层 `SyncProfile`
///   （`migration::data_sync::profile`）；`sync_tasks` 行 = 共享层
///   `StoredSyncTask`（`migration::data_sync::task_checkpoint`，W8 刀3e-2
///   从桌面 `store/models.rs` 上移——桌面文件形与服务端表形共用一条
///   词汇，FavoriteQuery/QueryHistoryEntry 同款先例）。
/// - **深度结构 JSON 投影**：`tables_json` / `options_json`（档案的表
///   映射含结构化源过滤器/记录集，展开列不现实）；`payload_json`
///   （任务整档——断点字段是文档形状态，展开列无查询收益）。坏行按
///   存储故障上抛（v12 `schema_diff_profiles` 同款 fail-closed）。
/// - **任务投影列不加 CHECK**：`status` / `strategy` 在共享词汇里是
///   自由串（serde 缺省 + `normalize_legacy_state` 重写），存储层不得比
///   单家词汇更严——否则桌面接受的档案在服务端被拒，违反双形态一致性。
///   投影列只为续航查找（连接对）与观测（状态/时间）服务。
/// - **TIMESTAMPTZ → TEXT**（定宽 RFC3339 UTC，v5 起口径）。
const V15_SYNC_ASSETS: Migration = Migration {
    version: 15,
    name: "data sync assets (task checkpoints + reusable profiles)",
    expand: r#"
CREATE TABLE sync_profiles (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  version INTEGER NOT NULL CHECK (version > 0),
  name TEXT NOT NULL CHECK (name <> ''),
  source_connection_id TEXT NOT NULL CHECK (source_connection_id <> ''),
  target_connection_id TEXT NOT NULL CHECK (target_connection_id <> ''),
  source_database TEXT CHECK (source_database IS NULL OR source_database <> ''),
  target_database TEXT CHECK (target_database IS NULL OR target_database <> ''),
  source_schema TEXT CHECK (source_schema IS NULL OR source_schema <> ''),
  target_schema TEXT CHECK (target_schema IS NULL OR target_schema <> ''),
  tables_json TEXT NOT NULL,
  options_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_sync_profiles_connections
  ON sync_profiles (organization_id, source_connection_id, target_connection_id);

CREATE TABLE sync_tasks (
  organization_id TEXT NOT NULL,
  id TEXT NOT NULL CHECK (id <> ''),
  source_connection_id TEXT NOT NULL CHECK (source_connection_id <> ''),
  target_connection_id TEXT NOT NULL CHECK (target_connection_id <> ''),
  status TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (organization_id, id)
);
CREATE INDEX idx_sync_tasks_status
  ON sync_tasks (organization_id, status);
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
