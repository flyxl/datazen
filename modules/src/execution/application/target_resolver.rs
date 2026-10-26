//! `ExecutionTargetResolver`：目标规范化的窄端口。
//!
//! ## 它存在的**首要**理由：打破引用环
//!
//! 桌面宿主里「把提交态目标解析成规范化目标」这件事由 `PlatformAdapter` 承担
//! （`src-tauri/src/platform/adapter.rs` 的 `resolve_execution_target`）：它需要**桌面
//! driver registry** 与**目标元数据缓存**，因此是形态特有的。
//!
//! 共享 `Application` 对象需要这项能力，但它**绝不能**持有 `Arc<PlatformAdapter>`
//! ——`PlatformAdapter` 持 `Arc<ApplicationServices>`，两边都持有就是引用环。
//! 因此这里给出窄端口：`Application` 持 `Arc<dyn ExecutionTargetResolver>`，
//! 桌面实现在**宿主侧**实现它。
//!
//! ## 与 `application` 侧 `TargetResolver` 的分工
//!
//! `application` 侧的 `TargetResolver`（`packages/application/src/target.rs`）是
//! §4.3 六步计算的**纯函数部分**：形状合法性、别名折叠、操作级要求校验。
//! 本端口是它缺的**环境部分**：目标所属连接的驱动形状与命令要求**从哪来**
//! （桌面 = driver registry + 目标元数据缓存；服务端 = 另一份来源）。
//!
//! ## 词汇表
//!
//! `ExecutionTarget`（提交态）与 `CanonicalTarget`（规范化后）复用
//! `crate::execution::domain`（规范化目标）与 `crate::connection::domain`（提交态命名空间），不另起一份形状；`TargetResolution` 是本端口的结果形状。
//!
//! ## 边界
//!
//! * 端口方法**不得**出现宿主类型：这里只有 platform-api 自己的 ID newtype 与
//!   目标形状，桌面 driver registry 一律藏在实现里。
//! * 失败一律 `Result<_, PortError>`；端口**不重试、不鉴权、不缓存**。
//! * 「不被支持」是**能力事实**，按端口约定由端口作为**值**返回
//!   （[`TargetResolution::Unsupported`]），由用例层翻译成 `ApiErrorCode`——
//!   端口不把业务拒绝当错误抛。

use async_trait::async_trait;

use crate::execution::domain::{CanonicalTarget, ExecutionTarget};
use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::ConfigRevision;

/// 目标解析的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetResolution {
    /// 规范化成功：这一份身份由 ResourceManager、权限、缓存与执行**共用**。
    Resolved(CanonicalTarget),
    /// 该连接 / 该命令 / 该目标不被支持（驱动不认这个命名空间，或命令未注册）。
    ///
    /// 这是**能力事实**，不是错误：用例层据此翻译成 `ApiErrorCode::TargetUnsupported`。
    Unsupported { reason_code: &'static str },
}

impl TargetResolution {
    /// 规范化结果的命名空间指纹（未解析时为空串）。
    ///
    /// 便利方法：调用方常要拿指纹做缓存键 / 比较键，不必先解包枚举。
    pub fn namespace_fingerprint_of(&self) -> String {
        match self {
            Self::Resolved(target) => target.namespace_fingerprint(),
            Self::Unsupported { .. } => String::new(),
        }
    }
}

/// 目标规范化的窄端口。
#[async_trait]
pub trait ExecutionTargetResolver: Send + Sync + 'static {
    /// 把提交态目标解析成规范化目标。
    ///
    /// `expected_config_revision` 是调用方读到的**连接配置版本**：与目标所属
    /// 连接当前的配置版本不一致 ⇒ 失败关闭（`PortError::CasConflict`）——规范化
    /// 结论是对**那一版配置**成立的，配置已变就得重读再解析。
    ///
    /// 目标所属连接不存在或已禁用 ⇒ `PortError::NotFound`；驱动 registry 等环境
    /// 事实取不到 ⇒ `PortError::BackendUnavailable`。
    async fn resolve(
        &self,
        ctx: &RequestContext,
        expected_config_revision: ConfigRevision,
        command: &str,
        target: &ExecutionTarget,
    ) -> Result<TargetResolution, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::domain::NamespaceTarget;
    use crate::execution::domain::{CanonicalNamespace, CanonicalNamespaceId};
    use crate::shared::id::{
        ClientInstanceId, ConnectionId, OrganizationId, PrincipalId, RequestId,
    };
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

    fn target() -> ExecutionTarget {
        ExecutionTarget::new(
            ConnectionId::new("conn-1"),
            NamespaceTarget::database("app"),
        )
    }

    fn canonical() -> CanonicalTarget {
        CanonicalTarget::new(
            ConnectionId::new("conn-1"),
            CanonicalNamespace {
                database: Some(CanonicalNamespaceId::new("app")),
                ..CanonicalNamespace::default()
            },
            None,
        )
    }

    struct StubResolver {
        outcome: TargetResolution,
    }

    #[async_trait]
    impl ExecutionTargetResolver for StubResolver {
        async fn resolve(
            &self,
            _ctx: &RequestContext,
            _expected_config_revision: ConfigRevision,
            _command: &str,
            _target: &ExecutionTarget,
        ) -> Result<TargetResolution, PortError> {
            Ok(self.outcome.clone())
        }
    }

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn ExecutionTargetResolver>();
        let resolver: Arc<dyn ExecutionTargetResolver> = Arc::new(StubResolver {
            outcome: TargetResolution::Resolved(canonical()),
        });
        let resolution = resolver
            .resolve(&context(), ConfigRevision::new(3), "query", &target())
            .await
            .expect("解析失败必须是端口事实");
        assert_eq!(resolution, TargetResolution::Resolved(canonical()));
    }

    #[tokio::test]
    async fn an_unsupported_target_is_a_capability_fact_not_an_error() {
        let resolver: Arc<dyn ExecutionTargetResolver> = Arc::new(StubResolver {
            outcome: TargetResolution::Unsupported {
                reason_code: "driver-command-unregistered",
            },
        });
        let resolution = resolver
            .resolve(&context(), ConfigRevision::new(3), "query", &target())
            .await
            .expect("不被支持不是错误");
        assert_eq!(
            resolution,
            TargetResolution::Unsupported {
                reason_code: "driver-command-unregistered"
            }
        );
    }

    #[test]
    fn the_signature_carries_no_host_type() {
        // 端口方法只认 platform-api 自己的类型：请求上下文、配置版本、目标形状。
        let source = code_only(include_str!("target_resolver.rs"));
        let declaration = source
            .split("pub trait ExecutionTargetResolver")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        for forbidden in [
            "crate::store",
            "crate::platform",
            "tauri",
            "AppDb",
            "Store",
            "PlatformAdapter",
            "DriverRegistry",
        ] {
            assert!(
                !declaration.contains(forbidden),
                "端口签名不得出现宿主类型 `{forbidden}`：{declaration}"
            );
        }
        assert!(declaration.contains("target: &ExecutionTarget"));
        assert!(declaration.contains("expected_config_revision: ConfigRevision"));
    }

    #[test]
    fn the_resolution_reports_the_canonical_identity_only() {
        let resolution = TargetResolution::Resolved(canonical());
        assert_eq!(resolution, TargetResolution::Resolved(canonical()));
        // 规范化结果里只有身份，没有展示名。
        assert_eq!(
            TargetResolution::Resolved(canonical()).namespace_fingerprint_of(),
            "app//"
        );
    }
}
