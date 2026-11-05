//! W8 刀1：[`TableColumnSource`] 端口的 server 实现（变更二端点的命令面列源
//! 备料件）。
//!
//! 共享层端口（`datazen::table_editing::TableColumnSource`）要求宿主回答
//! 「这张表的真实列与主键」；答不出必须**显式报错**——空答案与
//! 「没有主键」不可区分（端口硬规则，`table_editing/mod.rs` 模块头）。
//! 桌面宿主镜像（`datazen-desktop::commands::data::table_columns` 的
//! `HostTableColumns`）走 schema 缓存 + 活句柄目录读；server v1 无直连
//! 句柄面，本件走**命令面**：`get_table_schema`（`datazen-driver-api`
//! schema 目录命令）经 [`TargetCommandRunner`] 的 target 分支
//! （`manual_handle: None`）执行——不携带 `dbSessionId`、不解析会话
//! （CM-63 与本件正交）、不依赖刀2 活句柄面。
//!
//! ## 执行流（设计档 §2.2 六步）
//!
//! 1. **方言驱动解析**：registry 无该 `driver_type` →
//!    `NotConfigured("Driver unavailable")`（桌面镜像同文案）。
//! 2. **schema 梯子**：`driver.has_schema_level()` 过滤——无 schema 级的
//!    驱动拒绝任何 schema 实参（`public.users` 在 MySQL 不是东西）；有效
//!    schema 同时决定命令 input 与 target 命名空间的 schema 层。database
//!    必填由上游 `validate_table_context` 保证，本件不兜底。
//! 3. **铸造 target 绑定**：object target 是 `get_table_schema` 的目标准入
//!    硬要求（`requires_object`，`command_api.rs` 规则钉测试；server 先例
//!    `workflow/resource_execution.rs` 同映射）；`revision` = 调用方已读
//!    config_revision（`prepare` 的 CAS 闸语义，与读三端点同款）。
//! 4. **经 runner 跑命令**：`run(&ctx, &binding, "get_table_schema",
//!    input)`，input 的 `table` + `database` 必填（命令定义 required）。
//! 5. **解析产物**：outputs 里 `kind == "commandResult"` 块的 `data` 用
//!    `parse_table_schema_from_command` 反序列化为 `TableSchema`；缺块 /
//!    坏形状 → `Validation("Table schema unavailable for '<table>': …")`
//!    （桌面文案同族）——绝不出「看似成功」的空列答案。
//! 6. **映射 `TableSchema` → `CachedColumns`**：primary_keys 字段优先、
//!    空则回退 is_primary_key 列标记（`TableSchema::effective_primary_keys`
//!    既有规则）；**columns 为空 → 显式 `Validation`**（端口硬规则）。
//!
//! ## 错误面与装配
//!
//! 错误面零新码：runner 失败的 `ApiError` 直接走共享层现成的
//! `From<ApiError> for TableEditingError`（逐臂映射已定：
//! `InvalidArgument`/`ConfigRevisionMismatch` → `Validation` 桶，
//! `ServiceUnavailable` → `NotConfigured` 桶）。组件按请求构造
//! （`ctx`/`revision` 是每请求事实），不进 bootstrap 装配图——刀3 的
//! handler 在调用点铸造（与桌面 `HostTableColumns<'a>` 持 `&AppState`
//! 的每请求形状同构）。

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{json, Value as Json};

use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::target::{ExecutionTarget, NamespaceTarget, ObjectTarget};
use datazen::schema::cache::CachedColumns;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ConnectionId, Counter};
use datazen::table_browsing::api::{TargetBinding, TargetCommandRunner};
use datazen::table_editing::change_set::RowChangeTableContext;
use datazen::table_editing::TableColumnSource;
use datazen::table_editing::TableEditingError;
use datazen_driver_api::parse_table_schema_from_command;

/// server 形态的变更列源（刀1 备料件）。
///
/// `runner`/`registry` 与 tableData 域的 [`crate::execution::target_runner::ServerTargetRunner`]
/// 同实例（`TableDataState` 既有零件）；`ctx` 是请求真主体
/// （`SessionIdentity.ctx`）；`revision` 是 handler 已读的 profile
/// config_revision。
pub struct ServerTableColumnSource {
    runner: Arc<dyn TargetCommandRunner>,
    registry: Arc<DriverRegistry>,
    ctx: RequestContext,
    revision: Counter,
}

impl ServerTableColumnSource {
    /// 构造（刀3 handler 调用点铸造；不进全局装配图）。
    pub fn new(
        runner: Arc<dyn TargetCommandRunner>,
        registry: Arc<DriverRegistry>,
        ctx: RequestContext,
        revision: Counter,
    ) -> Self {
        Self {
            runner,
            registry,
            ctx,
            revision,
        }
    }
}

#[async_trait]
impl TableColumnSource for ServerTableColumnSource {
    async fn table_columns(
        &self,
        table: &RowChangeTableContext,
    ) -> Result<CachedColumns, TableEditingError> {
        // (1) 方言驱动解析：未知驱动是配置事实，不是调用方的输入错误。
        let driver = self
            .registry
            .get_sql_driver_by_name(&table.driver_type)
            .await
            .ok_or_else(|| TableEditingError::NotConfigured("Driver unavailable".into()))?;

        // (2) schema 梯子：无 schema 级的驱动拒绝任何 schema 实参——
        //     过滤发生在铸造绑定与命令 input **之前**（桌面镜像同序）。
        let schema = table
            .schema
            .as_deref()
            .map(str::trim)
            .filter(|schema| !schema.is_empty() && driver.has_schema_level());

        // (3) 铸造 target 绑定：object target 是 get_table_schema 的硬准入
        //     要求（requires_object）；命名空间 schema 层随有效 schema
        //     （无 schema 级的驱动连命名空间都不携带 schema 值）。
        let binding = TargetBinding {
            target: ExecutionTarget::new(
                ConnectionId::new(table.connection_id.clone()),
                NamespaceTarget::new(
                    table.database.clone(),
                    None,
                    schema.map(str::to_owned),
                    Vec::new(),
                ),
            )
            .with_object(ObjectTarget::new("table", table.table.clone())),
            revision: self.revision,
            manual_handle: None,
        };

        // (4) 经 runner 跑命令：table + database 必填（命令定义 required）；
        //     有效 schema 在场才携带 schema 键。
        let mut input = json!({
            "table": table.table,
            "database": table.database,
        });
        if let Some(schema) = schema {
            input["schema"] = Json::String(schema.to_owned());
        }
        let outputs = self
            .runner
            .run(&self.ctx, &binding, "get_table_schema", input)
            .await?;

        // (5) 解析产物：缺 commandResult 块 / data.schema 缺失 / 反序列化
        //     失败都收敛为同族 Validation——空答案不等于无主键。
        let parsed = outputs
            .iter()
            .find(|output| output["kind"] == "commandResult")
            .and_then(|output| parse_table_schema_from_command(&output["data"]))
            .ok_or_else(|| {
                TableEditingError::Validation(format!(
                    "Table schema unavailable for '{}': the command outputs carried no \
                     readable table schema; the primary key cannot be verified",
                    table.table
                ))
            })?;

        // (6) TableSchema → CachedColumns：空列显式报错（端口硬规则 +
        //     桌面镜像同文案）。
        if parsed.columns.is_empty() {
            return Err(TableEditingError::Validation(format!(
                "Table schema unavailable for '{}': the driver reported no columns, so \
                 the primary key cannot be verified",
                table.table
            )));
        }
        Ok(CachedColumns {
            columns: parsed.columns,
            primary_keys: parsed.effective_primary_keys(),
            table_name: parsed.table_name,
            cached_at: Instant::now(),
        })
    }
}
