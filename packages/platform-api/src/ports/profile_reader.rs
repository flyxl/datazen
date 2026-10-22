//! `ProfileReader`：连接档案的**只读**窄端口。
//!
//! ## 为什么它与 `ProfileRepository` 并存而不合并
//!
//! [`ProfileRepository`](crate::ports::profile::ProfileRepository) 是连接档案的**写侧
//! 权威**：建档、改档、禁用、推进凭据版本都在它上面。共享 `Application` 对象里
//! 大多数地方**只读**档案（解析目标、列连接、核对配置版本），却因此被迫依赖一个
//! 带写能力的端口——写能力一旦出现在依赖面上，「只读」就只是约定，不是类型事实。
//!
//! 因此这里给出**同一份事实的只读投影**：两个方法、零写路径。**写路径只留在
//! `ProfileRepository`**；实现方（桌面的 `DesktopProfiles`）**同时实现两个 trait**，
//! 事实仍然只有一份；消费方按需注入窄的那个。这正是 `ports/mod.rs` 模块头那条
//! 「repositories 是一张表，不是一个合并 trait」的同一原则：按**聚合**拆，
//! **按读写再切一刀也不合并**。
//!
//! ## 词汇表
//!
//! `ProfileRecord` / `ProfileScope` 直接复用 [`crate::ports::profile`] 的定义，
//! 不另起一份形状。
//!
//! ## 边界
//!
//! * **只读**：没有建档、改档、禁用、推进凭据版本的方法。
//! * 失败一律 `Result<_, PortError>`；端口**不重试、不鉴权、不缓存**。
//! * 越组织访问按「不存在」处理（`None` / 空列表），不泄漏「存在但无权」（CM-05）。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::error::PortError;
use crate::id::ConnectionId;
use crate::ports::profile::{ProfileRecord, ProfileScope};

/// 连接档案的只读窄端口。
///
/// 与 [`ProfileRepository`](crate::ports::profile::ProfileRepository) 读同一份事实，
/// 但**只暴露读**：注入这个端口的组件在类型层面就写不了档案。
#[async_trait]
pub trait ProfileReader: Send + Sync + 'static {
    /// 按 `connectionId` 读档案。缺失或越组织均返回 `None`——**不是** `NotFound`。
    ///
    /// 身份参数是**持久化**的 `ConnectionId`；运行时会话 id（`DbSessionId`）在这里
    /// 不可表达（`docs/architecture/naming.md` 的 ID 术语规范）。
    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError>;

    /// 列出作用域内的全部档案。越组织返回**空列表**而不是错误（CM-05）。
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
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

    struct StubProfiles;

    #[async_trait]
    impl ProfileReader for StubProfiles {
        async fn get(
            &self,
            _ctx: &RequestContext,
            _connection_id: ConnectionId,
        ) -> Result<Option<ProfileRecord>, PortError> {
            Ok(None)
        }

        async fn list(
            &self,
            _ctx: &RequestContext,
            _scope: ProfileScope,
        ) -> Result<Vec<ProfileRecord>, PortError> {
            Ok(Vec::new())
        }
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

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn ProfileReader>();
        let reader: Arc<dyn ProfileReader> = Arc::new(StubProfiles);
        let record = reader
            .get(&context(), ConnectionId::new("conn-1"))
            .await
            .expect("读取失败必须是端口事实");
        assert!(record.is_none());
        let records = reader
            .list(&context(), ProfileScope::new(OrganizationId::new("org-1")))
            .await
            .expect("列举失败必须是端口事实");
        assert!(records.is_empty());
    }

    #[test]
    fn the_reader_declares_no_write_path() {
        let source = code_only(include_str!("profile_reader.rs"));
        let declaration = source
            .split("pub trait ProfileReader")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            2,
            "只读端口只允许两个方法：{declaration}"
        );
        for forbidden in [
            "create",
            "compare_and_set",
            "disable",
            "advance_credential_revision",
        ] {
            assert!(
                !declaration.contains(forbidden),
                "只读端口不得出现写方法 `{forbidden}`：{declaration}"
            );
        }
    }

    #[test]
    fn the_reader_reuses_the_repository_vocabulary_instead_of_reshaping_it() {
        // 只读端口复用既有词汇表：档案记录与作用域都来自 `ports::profile`，
        // 没有另起一份同义形状。
        let source = code_only(include_str!("profile_reader.rs"));
        assert!(source.contains("use crate::ports::profile::{ProfileRecord, ProfileScope};"));
    }
}
