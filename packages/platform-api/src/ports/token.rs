//! `SubmissionTokenIssuer` 迁移垫片（D3，2026-10-26）。
//!
//! 提交令牌签发端口与 `SubmissionPresentation`/`VerifiedSubmission` 的唯一定义
//! 自 D3 起在 `datazen-modules::execution::application::submission`。本模块只
//! 再导出，旧 import 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::{
    SubmissionPresentation, SubmissionTokenIssuer, VerifiedSubmission,
};
