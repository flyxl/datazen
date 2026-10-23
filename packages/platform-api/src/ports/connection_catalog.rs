//! `ConnectionCatalog`：连接目录（连接配置 + 分组）的持久化端口。
//!
//! # 为什么这个端口存在
//!
//! 连接列表 / 表单 / 导入是**每个形态都要有**的能力，但它们原先由宿主命令直接
//! 读桌面 `DesktopRepository`（`{appData}/connections.json`）。于是「连接目录」
//! 这个聚合的读取路径住在了 `src-tauri/src/commands/` 里，而用例（校验、合并
//! 缺失凭据、批量导入去重）就跟着散在命令里。本端口把那半边的**存储语义**
//! 抽出来，命令改为只调用例（见 `docs/development/platform-development-plan.md`
//! §5.1.2 第六步）。
//!
//! # 词汇表：直接复用 `ConnectionConfig`
//!
//! 连接配置的形状本来就住在驱动契约 crate `datazen-driver-api`——那是**每个形态
//! 都已经链接**的那一份，且是 IPC 载荷与前端表单的唯一形状。本端口**不另造**
//! 投影类型（不像 `Profile` 那样另有一套服务端词汇表）：再造一份就必然双写漂移，
//! 而连接表单要求「打开即逐字回显」，漂移会直接变成用户可见的数据丢失。
//!
//! # 明文语义与脱敏边界
//!
//! 端口是**内存内明文**语义：口令、SSH / 代理凭据原样进出，加密与落盘格式由
//! 实现方负责（桌面：与连接密码同源的 AES-256-GCM）。端口**不脱敏**——脱敏发生
//! 在 IPC 错误载荷与日志边界（[`docs/architecture/README.md`] 的全局 IPC 错误
//! 处理），把它挪进端口会让建连路径拿不到真口令。
//!
//! # 组织限定
//!
//! 每个方法都带 [`OrganizationScope`]：越组织访问按**「不存在」**处理
//! （`list` → 空列表、`get` → `None`、写入 → [`PortError::NotFound`]），**不**返回
//! 「无权限」——后者会泄漏「存在但无权」（CM-05）。桌面是单租户：它只承认自己那
//! 一个组织，别的组织一律按「不存在」处理（与 `TunnelRepository` 同一口径）。
//!
//! # 分组为什么是整表读写
//!
//! 分组只是一串**名字**：没有独立 id、没有元数据、顺序即用户排序。因此端口提供
//! [`ConnectionCatalog::groups`] / [`ConnectionCatalog::save_groups`] 的整表语义，
//! 而不是逐条 CRUD——后者需要给分组编造身份，还要定义「重命名时引用怎么办」，
//! 而产品里根本没有那个概念。整表写也正好匹配导入路径的行为（读-合并-写回）。
//!
//! # `touch_last_connected_at` 为什么单独一个方法
//!
//! 记一次「已连接」只该改一个时间戳。若走 `get` → 改 → `save`，就会把整份配置
//! 写回去，从而覆盖同一时刻用户在前端正在做的编辑（lost update）；单独的方法让
//! 实现方能在锁内只改那一个字段。

use async_trait::async_trait;
use datazen_driver_api::ConnectionConfig;

use crate::error::PortError;
use crate::ports::scope::OrganizationScope;

/// 连接目录（连接配置 + 分组）的持久化端口。
#[async_trait]
pub trait ConnectionCatalog: Send + Sync + 'static {
    /// 全部连接配置，按用户排序返回。越组织 → 空列表。
    async fn list(&self, scope: &OrganizationScope) -> Result<Vec<ConnectionConfig>, PortError>;

    /// 单条连接配置（含明文凭据）。越组织 → `None`。
    async fn get(
        &self,
        scope: &OrganizationScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, PortError>;

    /// 新建或更新一条连接配置。越组织 → [`PortError::NotFound`]。
    async fn save(
        &self,
        scope: &OrganizationScope,
        config: ConnectionConfig,
    ) -> Result<(), PortError>;

    /// 删除一条连接配置。越组织 → [`PortError::NotFound`]。
    ///
    /// 只删配置，**不**负责断开已建立的会话：会话生命周期是运行时的事，
    /// 端口不该顺手做副作用（否则「删配置」会连带杀掉正在跑的查询）。
    async fn delete(&self, scope: &OrganizationScope, connection_id: &str)
        -> Result<(), PortError>;

    /// 按给定 id 顺序重排；未列出的按原有顺序追加在末尾，未知 id 忽略。
    ///
    /// 契约里写明这条语义，是因为排序是**用户可见**的：前端拖拽后只上传 id 列表，
    /// 若实现方把「未列出的」丢掉，用户的其它连接就会从列表里消失。
    async fn reorder(
        &self,
        scope: &OrganizationScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), PortError>;

    /// 只更新「最后连接时间」。越组织 → [`PortError::NotFound`]。
    async fn touch_last_connected_at(
        &self,
        scope: &OrganizationScope,
        connection_id: &str,
    ) -> Result<(), PortError>;

    /// 分组名列表（顺序即用户排序）。越组织 → 空列表。
    async fn groups(&self, scope: &OrganizationScope) -> Result<Vec<String>, PortError>;

    /// 整表替换分组名。越组织 → [`PortError::NotFound`]。
    async fn save_groups(
        &self,
        scope: &OrganizationScope,
        groups: Vec<String>,
    ) -> Result<(), PortError>;
}
