//! Redact secrets before writing them to application logs.
//!
//! 实现已下沉到 `datazen-platform-infra`（`packages/platform-infra/src/redaction.rs`）；
//! 本模块只做再导出，宿主侧 `crate::log_redact::*` 调用点零改动。

pub use datazen_platform_infra::redaction::{
    redact_paths_for_ipc, redact_secrets_for_log, redact_url_for_log, sql_preview_for_log,
};
