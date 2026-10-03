//! 提交屏障与目录发布（CM-68 的「不许成功但无记录」那一条）。
//!
//! 这三件东西被单独成文件，是因为它们是**发布契约**而不是替换流程的内部步骤：
//! 提交只产出内存证据，目录落成才产出成功信号，两者的分界必须一眼可见。

use crate::connection::{ConfigRevision, DbSessionId};

/// 提交屏障。
///
/// §7.4 要求「confirm ⇒ 落一条内存替换操作记录」作为提交证据。屏障就是那份记录的
/// 位置标记：**它被放行只代表内存提交完成，不代表目录里已经有了新会话。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitBarrier {
    committed_at_nanos: Option<u64>,
}

impl CommitBarrier {
    pub const fn open() -> Self {
        Self {
            committed_at_nanos: None,
        }
    }

    /// 放行屏障（写内存替换操作记录）。幂等：重复提交返回第一次的时间戳。
    pub fn commit(&mut self, now_nanos: u64) -> u64 {
        match self.committed_at_nanos {
            Some(at) => at,
            None => {
                self.committed_at_nanos = Some(now_nanos);
                now_nanos
            }
        }
    }

    /// 屏障是否**已经放行**（即内存提交是否落成）。命名与语义必须一致，
    /// 否则「已提交的候选不得再放弃」这类判断会被读反。
    pub const fn is_committed(&self) -> bool {
        self.committed_at_nanos.is_some()
    }

    pub const fn committed_at_nanos(&self) -> Option<u64> {
        self.committed_at_nanos
    }
}

/// 替换回执。**一旦生成就不再改变** —— 恢复时取回的是同一份，不是重算的一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementReceipt {
    pub new_session_id: DbSessionId,
    pub old_session_id: DbSessionId,
    pub connection_id: crate::connection::ConnectionId,
    /// 旧会话上承载的附属令牌（文件/流等），随替换一并转移，调用方据此续订。
    pub attachment_token: String,
    pub config_revision: ConfigRevision,
    pub committed_at_nanos: u64,
    pub published_at_nanos: Option<u64>,
}

/// 目录写入故障。宿主把故障**如实上报**，不吞、不改判成成功。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryFault {
    /// 目录暂时不可用（并发写/超时/存储抖动）。候选留在已提交态等待重试。
    Unavailable(&'static str),
    /// 目录明确拒绝这条记录。属于需要人工裁决的缺陷，不允许重试掩盖。
    Rejected(&'static str),
}

impl DirectoryFault {
    pub const fn reason_code(self) -> &'static str {
        match self {
            Self::Unavailable(_) => "directoryUnavailable",
            Self::Rejected(_) => "directoryRejected",
        }
    }
}

/// 目录里已经存在的记录形状。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    pub session_id: DbSessionId,
    pub attachment_token: String,
    pub published_at_nanos: u64,
}

/// 公开目录端口。**本模块自己不实现它** —— 目录由 Wave 2 接缝负责，
/// 宿主资源层只声明需要什么、以及失败时怎么表现。
pub trait DirectoryPublisher: Send + Sync + 'static {
    /// 原子地把新会话写入目录。成功即已可见；失败**必须**如实返回故障。
    fn publish(&self, receipt: &ReplacementReceipt) -> Result<Publication, DirectoryFault>;
    /// 按会话 ID 读目录记录（恢复路径需要它来核对「到底落没落」）。
    fn lookup(&self, session_id: &DbSessionId) -> Result<Option<Publication>, DirectoryFault>;
}

/// 目录发布结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    /// 目录记录已落：这是**唯一**的「替换完成」信号。
    Published { publication: Publication },
    /// 提交已完成但目录还没落。**不是成功**。调用方必须重试或升级为错误。
    Deferred { fault: DirectoryFault },
}

impl PublishOutcome {
    pub const fn published(&self) -> bool {
        matches!(self, Self::Published { .. })
    }
}
