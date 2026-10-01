//! `datazen-application`：平台用例层。
//!
//! ## 边界
//!
//! * 依赖：`datazen-platform-api`（+ 可选 `datazen-driver-api`）。**不依赖** `datazen-runtime`、
//!   **不依赖** Tauri、**不依赖** 任何 HTTP 框架（§2 依赖图：`APP → PA, DAPI`）。
//!   业务因此可以在没有 Tauri 的测试进程里调用。
//! * 职责：请求 DTO、用例签名、§4.3 目标计算、身份/授权判定顺序、对外错误形状
//!   [`ApiError`]。
//! * 不负责：把端口接上实现、HTTP/Tauri 状态映射、把端口错误翻译成业务错误。
//!   这些属于组装层（§6.2 第 7 步之后）。
//!
//! ## 模块
//!
//! | 模块 | 内容 |
//! |---|---|
//! | [`error`] | [`ApiError`]、[`ApiErrorCode`]、[`RetryDisposition`] |
//! | [`dto`] | 用例请求 DTO、持久化记录，以及 platform-api 契约 DTO 的再导出 |
//! | [`target`] | §4.3 六步目标计算 |
//! | [`identity_policy`] | §5.1 身份与授权判定顺序 |
//! | [`sessions`] | [`ConnectionUseCases`](sessions::ConnectionUseCases) 用例面 |
//!
//! [`ApplicationServices`] 按 §6.2 第 7 步的顺序组装：
//! `target → identity_policy → sessions`。它的构造函数对桌面与团队两种形态**完全相同**，
//! 只有传入的 `Arc<dyn …>` 实现不同——不允许为团队形态另写一套用例。

pub mod dto;
pub mod error;
pub mod identity_policy;
pub mod sessions;
pub mod target;

use std::sync::Arc;

use crate::identity_policy::IdentityPolicy;
use crate::sessions::ConnectionUseCases;
use crate::target::TargetResolver;

/// 用例层服务聚合。字段顺序即 §6.2 第 7 步的组装顺序。
#[derive(Clone)]
pub struct ApplicationServices {
    /// §4.3 目标计算。
    pub target: TargetResolver,
    /// §5.1 身份与授权判定顺序。
    pub identity_policy: IdentityPolicy,
    /// 连接用例面（实现在组装层注入）。
    pub sessions: Arc<dyn ConnectionUseCases>,
}

impl ApplicationServices {
    pub fn new(
        target: TargetResolver,
        identity_policy: IdentityPolicy,
        sessions: Arc<dyn ConnectionUseCases>,
    ) -> Self {
        Self {
            target,
            identity_policy,
            sessions,
        }
    }
}

#[cfg(test)]
mod tests {
    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn crate_root_code() -> String {
        code_only(
            include_str!("lib.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap_or_default(),
        )
    }

    #[test]
    fn the_service_aggregate_follows_the_assembly_order() {
        let code = crate_root_code();
        let target = code
            .find("pub target: TargetResolver")
            .expect("target 字段");
        let identity = code
            .find("pub identity_policy: IdentityPolicy")
            .expect("identity_policy 字段");
        let sessions = code
            .find("pub sessions: Arc<dyn ConnectionUseCases>")
            .expect("sessions 字段");
        assert!(target < identity, "顺序必须是 target → identity_policy");
        assert!(identity < sessions, "顺序必须是 identity_policy → sessions");
    }

    #[test]
    fn the_crate_does_not_depend_on_the_runtime_or_on_tauri() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("datazen-runtime"), "APP 不得依赖 RT");
        assert!(!manifest.contains("tauri"), "APP 不得依赖 tauri");
        for banned in ["axum", "actix-web", "warp", "tonic", "reqwest", "hyper"] {
            assert!(
                !manifest.contains(banned),
                "APP 不得依赖 HTTP 框架：{banned}"
            );
        }
        assert!(
            manifest.contains("datazen-platform-api"),
            "必须依赖 platform-api"
        );
    }

    #[test]
    fn the_module_list_is_exactly_the_five_documented_units() {
        let code = crate_root_code();
        for module in [
            "pub mod dto;",
            "pub mod error;",
            "pub mod identity_policy;",
            "pub mod sessions;",
            "pub mod target;",
        ] {
            assert!(code.contains(module), "缺少模块声明：{module}");
        }
        // 平台层的其余部分（runtime、Tauri、传输）不在本 crate 内。
        assert!(!code.contains("pub mod transport"));
        assert!(!code.contains("pub mod runtime"));
    }

    #[test]
    fn the_error_type_is_constructible_from_the_code_enum() {
        // 宿主映射层只需要 `datazen_application::error::ApiError`，不必知道模块内部形状。
        let error = crate::error::ApiError::new(
            crate::error::ApiErrorCode::TargetRequired,
            "缺少 database 层".to_string(),
        );
        assert_eq!(error.code, crate::error::ApiErrorCode::TargetRequired);
    }
}
