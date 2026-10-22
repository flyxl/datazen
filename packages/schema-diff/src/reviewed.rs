//! Short-lived, one-shot backend plans. Client SQL and risk labels never authorize writes.
//!
//! # 规则留包，存储走端口
//!
//! 本模块保留**评审计划的规则**：身份指纹、目标快照复核、依赖目录比对、计划有效期。
//! **存储**由 [`ReviewedPlanRepository`] 端口抽象，实例由宿主注入。
//!
//! 包内**不再持有进程级全局可变状态**：计划不再寄居在某个 `static` 里，
//! 而是寄居在宿主注入的存储实例中，并按组织作用域隔离——多租户宿主里
//! A 组织的计划不会被 B 组织读到、顶掉或提前消费掉。
//!
//! 端口方法一律带 `scope: &OrganizationScope`；**越组织的读取按「不存在」处理**
//! （`get` → `None`、`remove` → `false`），**不得**返回「无权限」错误——那会泄漏
//! 「存在但无权」（CM-05）。**越组织的写入结果由实现形态决定**（本包的内存实现按
//! 组织分桶落库，宿主的单租户实现直接丢弃），端口契约要求每个实现写明自己的
//! 语义。作用域类型由 `datazen-platform-api` 提供，本包不自造。
//!
//! # 时间基准
//!
//! 冻结时刻用**墙上时钟**（Unix 毫秒）而不是单调钟：存储实例可能被换成落盘实现，
//! 而 `Instant` 不可持久化；有效期规则（[`REVIEWED_PLAN_TTL`]）必须由规则层与
//! 存储层读同一份时刻。代价是**系统时钟异常会影响判定**，两个方向都按**失效关闭**
//! 处理（详见 [`ReviewedPlan::is_expired`]）：
//!
//! * **回拨**（`now < created_at_ms`）：判**已过期**，不让本该过期的计划复活。
//! * **前跳**（超过 TTL）：刚冻结的计划立刻过期，用户重新比对即可恢复。
//!
//! # 存储实现
//!
//! 本包随端口一起提供 [`InMemoryReviewedPlanRepository`]：形态中立、无副作用、无传输的
//! 进程内原语（先例：平台文档 §4 的 `ports::budget::pool_ledger`）。落盘实现由宿主
//! 另给；桌面在其之上叠加**单租户**收窄。
use super::{
    object_identity::{SchemaObjectDependencySnapshot, SchemaObjectIdentity},
    objects::SchemaObjectSnapshot,
    types::SchemaDiffPlan,
};
use async_trait::async_trait;
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, TableSchema};
use datazen_platform_api::ports::scope::OrganizationScope;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[path = "reviewed_catalog.rs"]
mod catalog;
pub use catalog::{validate_object_dependency_catalog, validate_table_identity_catalog};

/// 评审计划的有效期。超过即失效，必须重新比对后再部署。
pub const REVIEWED_PLAN_TTL: Duration = Duration::from_secs(1800);

/// 单个存储实例内保留的评审计划上限。超出时裁剪最旧的一条。
pub const REVIEWED_PLAN_MAX_ENTRIES: usize = 128;

/// 评审计划存储端口的错误。
///
/// 只表达「存储不可用」；**不**表达「越权」——越组织访问一律按「不存在」处理。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewedPlanRepositoryError {
    #[error("reviewed plan store unavailable: {0}")]
    Unavailable(String),
}

/// 已评审计划的存储端口。
///
/// 行类型是本包的词汇 [`ReviewedPlan`]——端口不另造一份计划结构。
///
/// * `put` 冻结一份计划；`get` 读取但不消费；`remove` 消费（取回并删除）。
/// * 缺失**与**越组织一律返回 `None` / `false`——**不**返回「无权限」错误，
///   否则会泄漏「存在但无权」（CM-05）。
/// * `remove` 返回 `false` 表示该计划已被消费，据此保证「一次部署只消费一次」。
#[async_trait]
pub trait ReviewedPlanRepository: Send + Sync + 'static {
    /// 冻结（写入）一份已评审计划。
    ///
    /// `put` 以 `scope` 为**归属键**；越组织写入是**丢弃**还是**落入别组织命名空间**，
    /// 由实现形态决定，但实现必须在自己的文档里写明。
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanRepositoryError>;

    /// 读取一份已评审计划（不消费）。缺失或越组织一律返回 `None`。
    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanRepositoryError>;

    /// 消费一份已评审计划（取回并删除）。已被消费返回 `false`。
    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanRepositoryError>;
}

/// 评审计划在存储里的键：**组织 + 计划 id**。
///
/// 组织维度进键，是「不同组织的计划互不可见」这条语义的**唯一**实现点：
/// 键里少了组织，跨组织就串味了。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlanKey {
    organization_id: String,
    plan_id: String,
}

/// 进程内存的评审计划存储：按**组织**分桶。
///
/// 形态中立、无副作用、无传输的进程内原语，因此随端口一起提供（先例：平台文档 §4
/// 的 `ports::budget::pool_ledger`）。落盘实现由宿主另给。
///
/// * 条目按 `(organization_id, plan_id)` 分桶：**不同组织的计划互不可见**——
///   越组织**读取**按「不存在」处理（`get` → `None`、`remove` → `false`），
///   **不**返回「无权限」错误（CM-05）。
/// * 越组织**写入**：本实现**按调用方 `scope` 的分桶落库**（键里带组织维度，
///   所以不会顶掉别组织的同名计划）；**丢弃**还是**落进别组织命名空间**是实现
///   形态的自由度，宿主形态的取舍见各自文档。
/// * 写入时裁剪**已失效**的条目（判定用规则层同一个 [`ReviewedPlan::is_expired`]），
///   并把条目数压到 [`REVIEWED_PLAN_MAX_ENTRIES`] 以内（超出时裁剪最旧的一条）。
///   上限是**整个实例**的（跨组织合计），不是每组织各一份——内存必须有界。
/// * 读取**不**按有效期过滤：过期判定属于规则层（[`validate`] 由
///   `consume` 调用），存储层只负责存取。
#[derive(Default)]
pub struct InMemoryReviewedPlanRepository {
    plans: Mutex<HashMap<PlanKey, ReviewedPlan>>,
}

impl InMemoryReviewedPlanRepository {
    /// 新建一个空存储。
    pub fn new() -> Self {
        Self::default()
    }

    fn key(scope: &OrganizationScope, plan_id: &str) -> PlanKey {
        PlanKey {
            organization_id: scope.organization_id().as_str().to_owned(),
            plan_id: plan_id.to_owned(),
        }
    }

    /// 当前存储的条目总数（跨组织合计）。
    pub async fn len(&self) -> usize {
        self.plans.lock().await.len()
    }

    /// 当前存储是否为空。
    pub async fn is_empty(&self) -> bool {
        self.plans.lock().await.is_empty()
    }
}

#[async_trait]
impl ReviewedPlanRepository for InMemoryReviewedPlanRepository {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanRepositoryError> {
        let mut plans = self.plans.lock().await;
        // 裁剪**已失效**的条目（判定与规则层同一个 `is_expired`）：失效条目对
        // 规则层来说本来就不可用，留着只是占位。按「年龄」裁剪会在**系统时钟
        // 回拨**时失效——回拨条目的年龄恒为 0，裁不掉；而且此时刚冻结的、仍然
        // 有效的计划 `created_at_ms` 反而**更小**，淘汰时会被**先**淘汰掉。
        plans.retain(|_, plan| !plan.is_expired());
        // Keep memory bounded even when a window repeatedly prepares a plan.
        if plans.len() >= REVIEWED_PLAN_MAX_ENTRIES {
            if let Some(oldest) = plans
                .iter()
                .min_by_key(|(_, plan)| plan.created_at_ms)
                .map(|(key, _)| key.clone())
            {
                plans.remove(&oldest);
            }
        }
        plans.insert(Self::key(scope, plan_id), plan);
        Ok(())
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanRepositoryError> {
        Ok(self
            .plans
            .lock()
            .await
            .get(&Self::key(scope, plan_id))
            .cloned())
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanRepositoryError> {
        Ok(self
            .plans
            .lock()
            .await
            .remove(&Self::key(scope, plan_id))
            .is_some())
    }
}

#[derive(Debug, Clone)]
pub struct ReviewedPlan {
    pub plan: SchemaDiffPlan,
    pub target_session: String,
    pub target_pool: String,
    pub target_identity: serde_json::Value,
    pub target_database_scope: Option<String>,
    pub target_schema_scope: Option<String>,
    pub snapshots: Vec<(String, TableSchema)>,
    pub has_complete_target_dependency_catalog: bool,
    pub target_dependency_schema_scope: Option<String>,
    pub object_snapshots: Vec<SchemaObjectSnapshot>,
    pub object_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
    pub target_table_identity_catalog: Vec<SchemaObjectIdentity>,
    pub has_complete_target_object_catalog: bool,
    pub target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
    pub source_snapshot: Option<ReviewedSourceSnapshot>,
    /// 冻结时刻（Unix 毫秒）。
    ///
    /// 用可持久化的墙上时钟而不是单调钟：存储实例可能被换成落盘实现，
    /// 而有效期规则（[`REVIEWED_PLAN_TTL`]）必须由规则层与存储层读同一份时刻。
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ReviewedSourceSnapshot {
    pub session: String,
    pub pool: String,
    pub identity: serde_json::Value,
    pub database_scope: Option<String>,
    pub schema_scope: Option<String>,
    pub table_snapshots: Vec<(String, TableSchema)>,
    pub object_snapshots: Vec<SchemaObjectSnapshot>,
    pub table_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
}

impl ReviewedPlan {
    /// 距冻结时刻过去了多久（墙上时钟差）。
    ///
    /// 系统时钟**回拨**时该差值会归零（`saturating_sub`）：此时「现在」早于冻结
    /// 时刻，真实经过时间**不可知**。因此**不要**只用 `age()` 判断有效性——
    /// 有效性用 [`Self::is_expired`]。
    ///
    /// 存储层的**裁剪**按 [`Self::is_expired`] 做（规则层同一个判定），因此回拨
    /// 造成的「冻结时刻在未来」的条目**会被裁掉**，不会占位。`age()` 本身只作为
    /// 「距今多久」的参考值，当前没有生产调用点。
    pub fn age(&self) -> Duration {
        Duration::from_millis(now_unix_millis().saturating_sub(self.created_at_ms))
    }

    /// 计划是否已过期（**失效关闭**）。
    ///
    /// * 正常路径：经过时间达到 [`REVIEWED_PLAN_TTL`] ⇒ 过期。
    /// * **系统时钟回拨**（`now < created_at_ms`）：时间基准不可信 ⇒ 判**已过期**。
    ///   冻结时刻落在「现在」之后，在单调世界里不可能发生，只能说明系统时钟被回拨
    ///   过。若不这样判，回拨会让本该过期的计划**复活**（**失效开放**）；判过期的
    ///   代价只是用户重新比对一次。
    ///
    /// 时钟**前跳**的方向天然是失效关闭：经过时间直接变大，刚冻结的计划立刻过期。
    pub fn is_expired(&self) -> bool {
        let now = now_unix_millis();
        if now < self.created_at_ms {
            return true;
        }
        Duration::from_millis(now - self.created_at_ms) >= REVIEWED_PLAN_TTL
    }
}

/// 当前时刻（Unix 毫秒）。系统时钟早于纪元时按 0 处理，不 panic。
fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

pub fn identity(config: &ConnectionConfig) -> serde_json::Value {
    serde_json::json!({"driver":config.database_type,"host":config.host,"port":config.port,
        "database":config.database,"schema":config.schema,"user":config.username,
        "options":config.options,"tunnel":config.ssh_tunnel})
}

pub fn validate_source_connection_snapshot(
    snapshot: &ReviewedSourceSnapshot,
    session: &str,
    pool: &str,
    config: &ConnectionConfig,
) -> Result<(), String> {
    if snapshot.session != session
        || snapshot.pool != pool
        || snapshot.identity != identity(config)
        || snapshot.database_scope != config.database
    {
        return Err("Source connection changed after review; compare again".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalDatabaseScope {
    Same,
    Different,
    Unknown,
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
        target_database_scope,
        target_schema_scope,
        false,
        None,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_dependency_catalog(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    dependency_schema_scope: Option<String>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
        target_database_scope,
        target_schema_scope,
        true,
        dependency_schema_scope,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_objects(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
) -> Result<(), String> {
    freeze_with_objects_and_mysql_view_context(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        object_snapshots,
        target_database_scope,
        target_schema_scope,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_objects_and_mysql_view_context(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        object_snapshots,
        Vec::new(),
        Vec::new(),
        false,
        target_database_scope,
        target_schema_scope,
        false,
        None,
        None,
        target_mysql_view_scope_context,
    )
    .await
}

/// Freeze one unified plan together with its full target object dependency catalog.
/// Deploy re-reads this catalog before executing any statement.
#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_unified_catalog(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    object_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
    target_table_identity_catalog: Vec<SchemaObjectIdentity>,
    has_complete_target_dependency_catalog: bool,
    has_complete_target_object_catalog: bool,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    dependency_schema_scope: Option<String>,
    source_snapshot: ReviewedSourceSnapshot,
    target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        object_snapshots,
        object_dependency_catalog,
        target_table_identity_catalog,
        has_complete_target_object_catalog,
        target_database_scope,
        target_schema_scope,
        has_complete_target_dependency_catalog,
        dependency_schema_scope,
        Some(source_snapshot),
        target_mysql_view_scope_context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn freeze_internal(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    object_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
    target_table_identity_catalog: Vec<SchemaObjectIdentity>,
    has_complete_target_object_catalog: bool,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    has_complete_target_dependency_catalog: bool,
    target_dependency_schema_scope: Option<String>,
    source_snapshot: Option<ReviewedSourceSnapshot>,
    target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().to_string();
    plan.plan_id = Some(id.clone());
    let frozen = ReviewedPlan {
        plan: plan.clone(),
        target_session: session,
        target_pool: handle.pool_id.clone(),
        target_identity: identity(config),
        target_database_scope,
        target_schema_scope,
        snapshots,
        has_complete_target_dependency_catalog,
        target_dependency_schema_scope,
        object_snapshots,
        object_dependency_catalog,
        target_table_identity_catalog,
        has_complete_target_object_catalog,
        target_mysql_view_scope_context,
        source_snapshot,
        created_at_ms: now_unix_millis(),
    };
    store
        .put(scope, &id, frozen)
        .await
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn consume(
    store: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    submitted: &SchemaDiffPlan,
    session: &str,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    target_database_scope: Option<&str>,
    target_schema_scope: Option<&str>,
) -> Result<ReviewedPlan, String> {
    let id = submitted
        .plan_id
        .as_ref()
        .ok_or("Prepare and review a new plan before deploying")?;
    let frozen = store
        .get(scope, id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Plan expired or already executed; compare again")?;
    validate(
        &frozen,
        submitted,
        session,
        handle,
        config,
        target_database_scope,
        target_schema_scope,
    )?;
    // 校验通过才消费：复核失败**不**烧掉计划，用户改回正确目标后仍能部署。
    // 并发部署时只有一个 `remove` 会成功，另一个据此判定「已被执行」。
    if !store
        .remove(scope, id)
        .await
        .map_err(|error| error.to_string())?
    {
        return Err("Plan already executed".into());
    }
    Ok(frozen)
}

fn validate(
    frozen: &ReviewedPlan,
    submitted: &SchemaDiffPlan,
    session: &str,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    target_database_scope: Option<&str>,
    target_schema_scope: Option<&str>,
) -> Result<(), String> {
    if frozen.is_expired() {
        return Err("Plan expired; compare again".into());
    }
    if frozen.plan != *submitted {
        return Err("Reviewed plan was modified; prepare again".into());
    }
    if frozen.target_session != session
        || frozen.target_pool != handle.pool_id
        || frozen.target_identity != identity(config)
    {
        return Err("Target identity changed after review; compare again".into());
    }
    if config.read_only {
        return Err("Target connection is read-only".into());
    }
    if normalized_scope(target_database_scope)
        != normalized_scope(frozen.target_database_scope.as_deref())
        || normalized_scope(target_schema_scope)
            != normalized_scope(frozen.target_schema_scope.as_deref())
    {
        return Err("Target database or schema scope changed after review; compare again".into());
    }
    Ok(())
}

fn normalized_scope(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Configuration identity is a conservative local check; driver-provided physical
/// identity is required to recognize aliases and distinct tunnels to one server.
pub fn same_endpoint(source: &ConnectionConfig, target: &ConnectionConfig) -> bool {
    let source_dialect = crate::types::normalize_dialect(&source.database_type);
    let target_dialect = crate::types::normalize_dialect(&target.database_type);
    if source_dialect != target_dialect {
        return false;
    }
    let mut a = identity(source);
    let mut b = identity(target);
    if let Some(v) = a.as_object_mut() {
        v.remove("user");
        v.insert("driver".into(), serde_json::Value::String(source_dialect));
    }
    if let Some(v) = b.as_object_mut() {
        v.remove("user");
        v.insert("driver".into(), serde_json::Value::String(target_dialect));
    }
    a == b
}

/// Detect aliases that reach the same database through different connection
/// settings. Database and schema names are only used as proof of distinct
/// scopes when the dialect's namespace rules make that conclusion reliable;
/// MySQL database names may be case-folded and SQLite paths may be hard links.
pub fn physical_database_scope(
    source: &ConnectionConfig,
    target: &ConnectionConfig,
    source_identity: Option<&str>,
    target_identity: Option<&str>,
    source_schema_scope: Option<&str>,
    target_schema_scope: Option<&str>,
    source_schema_identity: Option<&str>,
    target_schema_identity: Option<&str>,
) -> PhysicalDatabaseScope {
    let source_dialect = crate::types::normalize_dialect(&source.database_type);
    let target_dialect = crate::types::normalize_dialect(&target.database_type);
    if source_dialect != target_dialect {
        return PhysicalDatabaseScope::Different;
    }

    let source_schema = source_schema_scope
        .and_then(|value| normalized_scope(Some(value)))
        .or_else(|| normalized_scope(source.schema.as_deref()));
    let target_schema = target_schema_scope
        .and_then(|value| normalized_scope(Some(value)))
        .or_else(|| normalized_scope(target.schema.as_deref()));

    if let (Some(left), Some(right)) = (source_identity, target_identity) {
        if left != right {
            return PhysicalDatabaseScope::Different;
        }
        if source_dialect == "postgresql"
            && matches!((source_schema, target_schema), (Some(left), Some(right)) if left != right)
        {
            return PhysicalDatabaseScope::Different;
        }
        // SQL Server identifier equality follows the database collation, so
        // only driver-proven schema catalog identities can establish distinct
        // scopes. Object kinds without a proven definition rewrite remain
        // fail-closed in the unified planner's scope-mapping checks.
        if source_dialect == "sqlserver"
            && matches!((source_schema_identity, target_schema_identity), (Some(left), Some(right)) if left != right)
        {
            return PhysicalDatabaseScope::Different;
        }
        return PhysicalDatabaseScope::Same;
    }

    // PostgreSQL database names are case-sensitive catalog identities, and
    // schemas are separate namespaces inside a database. These names can
    // prove distinct scopes even when the server does not expose an identity.
    if source_dialect == "postgresql" {
        if matches!(
            (normalized_scope(source.database.as_deref()), normalized_scope(target.database.as_deref())),
            (Some(left), Some(right)) if left != right
        ) || matches!((source_schema, target_schema), (Some(left), Some(right)) if left != right)
        {
            return PhysicalDatabaseScope::Different;
        }
    }

    // MySQL can fold database names according to lower_case_table_names,
    // while SQLite paths can be aliases (including hard links). If the
    // driver could not provide a physical identity, spelling alone is not a
    // safe reason to allow a migration.
    PhysicalDatabaseScope::Unknown
}

pub fn validate_snapshot(
    table: &str,
    reviewed: &TableSchema,
    current: &TableSchema,
) -> Result<(), String> {
    fn relation_presentation_matches(identity: &str, reported_name: &str) -> bool {
        let (identity_scope, identity_relation) = identity
            .rsplit_once('.')
            .map_or((None, identity), |(scope, relation)| {
                (Some(scope), relation)
            });
        let (reported_scope, reported_relation) = reported_name
            .rsplit_once('.')
            .map_or((None, reported_name), |(scope, relation)| {
                (Some(scope), relation)
            });

        identity_relation == reported_relation
            && match (identity_scope, reported_scope) {
                (Some(identity_scope), Some(reported_scope)) => identity_scope == reported_scope,
                // Drivers may report an unqualified name after a schema has
                // already been supplied as a separate argument. The frozen
                // snapshot key remains the authoritative scoped identity.
                (Some(_), None) | (None, None) => true,
                // An unqualified frozen identity cannot prove that a
                // newly-qualified driver name belongs to the same schema.
                (None, Some(_)) => false,
            }
    }

    if !relation_presentation_matches(table, &reviewed.table_name)
        || !relation_presentation_matches(table, &current.table_name)
    {
        return Err(format!("Target schema changed for {table}; compare again"));
    }

    let mut reviewed_value = serde_json::to_value(reviewed).map_err(|e| e.to_string())?;
    let mut current_value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    if let Some(object) = reviewed_value.as_object_mut() {
        object.remove("tableName");
    }
    if let Some(object) = current_value.as_object_mut() {
        object.remove("tableName");
    }
    if reviewed_value != current_value {
        return Err(format!("Target schema changed for {table}; compare again"));
    }
    Ok(())
}

pub fn validate_object_snapshot(
    reviewed: &SchemaObjectSnapshot,
    current: &SchemaObjectSnapshot,
) -> Result<(), String> {
    if reviewed.kind != current.kind
        || reviewed.schema != current.schema
        || reviewed.name != current.name
        || reviewed.signature != current.signature
        || reviewed.target_schema != current.target_schema
        || reviewed.target_name != current.target_name
        || reviewed.definition.trim() != current.definition.trim()
        || reviewed.mysql_view_metadata != current.mysql_view_metadata
    {
        return Err(format!(
            "Target object {} changed after review; compare again",
            reviewed.name
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "reviewed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reviewed_repository_tests.rs"]
mod repository_tests;
