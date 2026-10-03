//! 会话登记表接缝（`registry`）。
//!
//! **职责边界**：本模块只提供一件事——registry 与它的消费者之间**唯一**的契约面
//! [`SessionPort`]。它是 P3 Wave 2 的 `p3-registry`（实现方）与 `p3-gateway`（消费方）
//! 之间的接缝：两条轨道各自只依赖这一个 trait，因此可以并行推进而不必等对方落地。
//! 契约本体在 [`port`]，模块文档（边界纪律与四条约束）在那里。
//!
//! **本模块明确不导出**（shared-boundaries-and-ports.md §3.2 第 5 行的边界要求）：
//!
//! - `dbSessionId` 的生成与归属校验 —— 归 `datazen_platform_api` 的 `SessionDirectory`，
//!   registry 只消费，不重新定义一遍编号规则；
//! - actor 邮箱、调度队列、登记表的读写锁 —— §6.3 的内部实现，永不跨模块；
//! - 资源层句柄（`ResourceHandle`、Lease、预算许可）—— 只在 §5.1 的资源端口内流转，
//!   不上浮到会话面；会话面看见的只是「预算不够」这一事实，即 `RuntimeError::BudgetExhausted`；
//! - 任何实现体：本模块是纯签名 + 纯 DTO，零状态、零 I/O、零 `tokio` 对象。
//!
//! **为什么方法集只有四个**：§7 描述的完整会话面有 `openSession` / `executeInSession` /
//! `executeAtTarget` / `setSessionContext` / `closeSession` / `cancelExecution` /
//! `attachSession` / `detachSession` 共八个方法，其中 `openSession`、
//! `setSessionContext`（需要 `ContextChangeReceipt`）、`executeAtTarget`、
//! `attachSession` / `detachSession`（需要 `AttachmentRequest` 与 attachmentToken 载体）
//! 的 DTO 目前**尚未冻结**。在这些 DTO 落地之前把签名写出来，等于在本仓库里埋一份
//! 必然与 Wave 1 漂移的第二份定义——那正是冻结契约要避免的事。因此本模块只冻结
//! 四个方法**所依赖的类型已经全部就位**的那部分面，缺口逐条写在
//! [`SessionPort::cancel_execution`] 的文档与各方法的失败条件里，交由 Wave 1 补齐。

pub mod port;

pub use port::SessionPort;
