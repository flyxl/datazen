//! `ArtifactRepository` 迁移垫片（D3，2026-10-26）。
//!
//! 产物仓储端口（写入、按字节/按块读取、导出、吊销）与 `ArtifactSpec`/
//! `ArtifactWriter`/`ChunkPayload`/`ByteRange`/`ExportSink`/`ExportReceipt`/
//! `RevokeReason` 的唯一定义自 D3 起在
//! `datazen-modules::execution::domain::artifact_repository`（§5.1：结果
//! artifact 的归属、发布状态和仓储接口归 execution::domain；文件/对象存储
//! 实现由宿主提供）。本模块只再导出，旧 import 路径不变；D8 退役本 crate
//! 时一并删除。

pub use datazen_modules::execution::domain::{
    provenance_fingerprint, ArtifactRepository, ArtifactSpec, ArtifactWriter, ByteRange,
    ChunkIndex, ChunkPayload, ExportReceipt, ExportSink, RevokeReason, ARTIFACT_BYTE_QUOTA,
};
