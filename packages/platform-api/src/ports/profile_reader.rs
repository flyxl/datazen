//! `ProfileReader` 迁移垫片（D2，2026-10-26）。
//!
//! 连接档案只读窄端口的唯一定义自 D2 起在
//! `datazen-modules::connection::domain::profile`（与 `ProfileRepository`
//! 同文件：按聚合拆、按读写再切一刀也不合并）。本模块只再导出，旧
//! import 路径不变；D8 退役本 crate 时一并删除。
//!
//! 设计要点保持原样：**只读**（零写路径）、失败一律 `PortError`、越组织
//! 访问按「不存在」处理（CM-05）；实现方同时实现读写双侧端口，事实只有一份。

pub use datazen_modules::connection::domain::ProfileReader;
