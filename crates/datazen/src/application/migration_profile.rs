//! `MigrationProfileReader`：迁移 profile 的只读应用端口。
//!
//! ## 它替代什么
//!
//! 桌面侧曾经有一个 `impl MigrationHost for AppState`（位于
//! `src-tauri/src/workflow/migration.rs`），它的 `transfer_profiles()` /
//! `sync_profiles()` / `schema_profiles()` 三个方法**只做一件事**：从 `DesktopRepository` 里
//! 读出已保存的迁移 profile 列表——而 `AppState` 因此拖着整个 `DesktopRepository`。
//!
//! 那个 workflow 迁移 step 能力已被移除（它从未真正发布；见
//! `docs/architecture/backend/workflow.md` 的「迁移 step（已移除）」）。**端口本身
//! 保留**：它是三件套的只读 profile 投影边界，与 workflow 无关。
//!
//! ## 它**不**包含什么
//!
//! 迁移的**写**（保存 / 更新 profile）与**执行**（跑一次传输 / 同步 / 结构比对）
//! **不**在这里：前者是迁移仓储的事，后者是三件套自己的命令路径的事。端口按聚合
//! 拆开，实现方不必实现与自己无关的方法（与 `ports/mod.rs` 模块头那条
//! 「repositories 是一张表，不是一个合并 trait」同一原则）。
//!
//! ## 词汇表
//!
//! `MigrationProfileSummary`：迁移 profile 的只读投影。三种操作（传输 / 同步 /
//! 结构比对）的领域 profile 字段集不同，但**共有**的那部分——身份、名字、两端的
//! 连接与库 / schema——在这里；各自特有的部分（表映射、写入模式、破坏性开关、
//! SQL 文件目的地）仍留在领域 crate 的 profile 类型里
//! （`datazen_data_transfer` / `datazen_data_sync` / `datazen_schema_diff`）。
//!
//! ## 边界
//!
//! * **只读**，按 `RequestContext` 限定；失败一律 `Result<_, PortError>`。
//! * 端口**不重试、不鉴权、不缓存**。

use async_trait::async_trait;

use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::{ConnectionId, Timestamp};

/// 迁移 profile 的只读投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationProfileSummary {
    /// profile id（持久化标识）。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 源端连接（**持久化** `connectionId`；运行时会话 id 在这里不可表达）。
    pub source_connection_id: ConnectionId,
    /// 目标端连接；导出到 SQL 文件的传输没有目标连接。
    pub target_connection_id: Option<ConnectionId>,
    /// 源端数据库。
    pub source_database: Option<String>,
    /// 目标端数据库。
    pub target_database: Option<String>,
    /// 源端 schema。
    pub source_schema: Option<String>,
    /// 目标端 schema。
    pub target_schema: Option<String>,
    /// Revision used to block execution of a profile that changed after plan preparation.
    pub updated_at: Timestamp,
}

/// 迁移 profile 的只读窄端口。
#[async_trait]
pub trait MigrationProfileReader: Send + Sync + 'static {
    /// 已保存的数据传输（data transfer）profile。
    async fn list_transfer_profiles(
        &self,
        ctx: &RequestContext,
    ) -> Result<Vec<MigrationProfileSummary>, PortError>;

    /// 已保存的数据同步（data sync）profile。
    async fn list_sync_profiles(
        &self,
        ctx: &RequestContext,
    ) -> Result<Vec<MigrationProfileSummary>, PortError>;

    /// 已保存的结构比对（schema diff）profile。
    async fn list_schema_diff_profiles(
        &self,
        ctx: &RequestContext,
    ) -> Result<Vec<MigrationProfileSummary>, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
    use std::sync::Arc;

    /// 端口必须**对象安全**：能以 `Arc<dyn Trait>` 注入（§4.1 的注入形态）。
    ///
    /// `?Sized` 是承重的：`dyn Trait` 本身不是 `Sized`，少了它这个断言退化成
    /// 「某个 `Sized` 类型满足这几个约束」，与对象安全无关。
    fn assert_object_safe<T: Send + Sync + 'static + ?Sized>() {}

    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn summary() -> MigrationProfileSummary {
        MigrationProfileSummary {
            id: "profile-1".into(),
            name: "prod → staging".into(),
            source_connection_id: ConnectionId::new("conn-src"),
            target_connection_id: Some(ConnectionId::new("conn-dst")),
            source_database: Some("app".into()),
            target_database: Some("app".into()),
            source_schema: Some("public".into()),
            target_schema: None,
            updated_at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    struct StubReader;

    #[async_trait]
    impl MigrationProfileReader for StubReader {
        async fn list_transfer_profiles(
            &self,
            _ctx: &RequestContext,
        ) -> Result<Vec<MigrationProfileSummary>, PortError> {
            Ok(vec![summary()])
        }

        async fn list_sync_profiles(
            &self,
            _ctx: &RequestContext,
        ) -> Result<Vec<MigrationProfileSummary>, PortError> {
            Ok(Vec::new())
        }

        async fn list_schema_diff_profiles(
            &self,
            _ctx: &RequestContext,
        ) -> Result<Vec<MigrationProfileSummary>, PortError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn MigrationProfileReader>();
        let reader: Arc<dyn MigrationProfileReader> = Arc::new(StubReader);
        let profiles = reader
            .list_transfer_profiles(&context())
            .await
            .expect("读 profile 失败必须是端口事实");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "profile-1");
        assert_eq!(
            profiles[0].source_connection_id,
            ConnectionId::new("conn-src")
        );
        assert_eq!(
            profiles[0].target_connection_id,
            Some(ConnectionId::new("conn-dst"))
        );
        assert_eq!(
            profiles[0].updated_at,
            Timestamp::new("2026-01-01T00:00:00Z")
        );
        assert!(reader
            .list_sync_profiles(&context())
            .await
            .expect("读 profile 失败必须是端口事实")
            .is_empty());
    }

    #[test]
    fn the_summary_uses_the_persisted_connection_id_not_a_session_id() {
        let summary = summary();
        assert_eq!(summary.source_connection_id, ConnectionId::new("conn-src"));
        // 运行时会话 id 在这个形状里不可表达。
        let declaration = code_only(include_str!("migration_profile.rs"));
        assert!(declaration.contains("source_connection_id: ConnectionId"));
        // 探针本身不得把被禁的字面量整段写进源码：拆成两段再拼。
        let session_id = ["DbSession", "Id"].concat();
        assert!(!declaration.contains(&session_id));
    }

    #[test]
    fn the_reader_declares_no_write_or_execution_path() {
        let source = code_only(include_str!("migration_profile.rs"));
        let declaration = source
            .split("pub trait MigrationProfileReader")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            3,
            "迁移 profile 端口只允许三个只读方法：{declaration}"
        );
        for forbidden in [
            "save",
            "delete",
            "run_transfer",
            "run_sync",
            "run_schema",
            "execute",
            "start_run",
            "finish_run",
        ] {
            assert!(
                !declaration.contains(forbidden),
                "只读端口不得出现写 / 执行方法 `{forbidden}`：{declaration}"
            );
        }
    }
}
