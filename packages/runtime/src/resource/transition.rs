//! 租约状态机：合法转移表与其三要素（状态机纪律：进入条件 / 状态内行为 / 退出条件）。
//!
//! 单独成文件是因为**表**才是契约：`LeaseState` 只是名字，转移表才写死了每一条边
//! 允许在什么条件下发生、以及不离开这个状态会怎样。

use crate::resource::lease::LeaseState;

/// 一次转移的三要素（状态机纪律：进入条件 / 状态内行为 / 退出条件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionRule {
    pub from: LeaseState,
    pub to: LeaseState,
    /// 进入条件：满足才允许做这条边。
    pub enters_when: &'static str,
    /// 状态内行为：停在这个状态里允许发生什么。
    pub behaves_as: &'static str,
    /// 退出条件：什么情况下必须离开这个状态。
    pub exits_when: &'static str,
}

/// 合法转移表。
///
/// 三条不变量：
/// 1. `Closed` 没有任何出边（终态不可复活）；
/// 2. 每个**非终态**都至少有一条出边（不存在只有进入没有退出的单向死锁）；
/// 3. 任何非终态都能到达 `Closed`（不存在死资源）。
pub const LEASE_TRANSITIONS: &[TransitionRule] = &[
    TransitionRule {
        from: LeaseState::Acquired,
        to: LeaseState::InUse,
        enters_when: "第一次执行挂上，或固定会话开始服务请求",
        behaves_as: "接受执行；归还时进入宿主+驱动的双条件裁决",
        exits_when: "执行全部终态且调用者释放 ⇒ 回池（回到 Acquired）或关闭",
    },
    TransitionRule {
        from: LeaseState::Acquired,
        to: LeaseState::Closing,
        enters_when: "申请后一次都没用过（预算超时回收、空闲 TTL、键轮换）",
        behaves_as: "跑关闭协议，不再接受执行",
        exits_when: "关闭确认 ⇒ Closed",
    },
    TransitionRule {
        from: LeaseState::Acquired,
        to: LeaseState::Quarantined,
        enters_when: "空闲复查时发现这条空闲资源已经不满足宿主条件（归属被禁用、句柄泄漏未清）",
        behaves_as: "立刻摘出空闲池；禁止再次签发；仍占用物理预算",
        exits_when: "排障流程强制关闭并确认 ⇒ Closed（绝不退回 Acquired）",
    },
    TransitionRule {
        from: LeaseState::InUse,
        to: LeaseState::Acquired,
        enters_when: "归还裁决通过：驱动 Clean **且**宿主四项检查全清（双条件）",
        behaves_as: "进入空闲池等待再次签发；TTL 从此刻起算",
        exits_when: "被再次签发 ⇒ InUse；空闲 TTL 到期或键轮换 ⇒ Closing",
    },
    TransitionRule {
        from: LeaseState::InUse,
        to: LeaseState::Closing,
        enters_when: "归还裁决判定不复用（驱动不干净或宿主条件不满足且可安全关闭）",
        behaves_as: "释放未完成协议与句柄，跑关闭协议",
        exits_when: "关闭确认 ⇒ Closed；若关闭过程本身状态不明 ⇒ Quarantined",
    },
    TransitionRule {
        from: LeaseState::InUse,
        to: LeaseState::Quarantined,
        enters_when: "归还裁决判定不复用，且事务/协议状态不明或取消失败",
        behaves_as: "禁止任何复用；等待人工或运维裁决",
        exits_when: "显式排障放行后关闭 ⇒ Closed（Quarantined 本身永不复用）",
    },
    TransitionRule {
        from: LeaseState::Closing,
        to: LeaseState::Quarantined,
        enters_when: "关闭握手失败或事务回滚结果不明",
        behaves_as: "占用预算但绝不复用，禁止静默丢弃",
        exits_when: "排障后强制关闭 ⇒ Closed",
    },
    TransitionRule {
        from: LeaseState::Closing,
        to: LeaseState::Closed,
        enters_when: "物理关闭已确认（CloseOutcome::Closed）",
        behaves_as: "预算已释放，物理句柄已从宿主表里摘除",
        exits_when: "无出边：终态",
    },
    TransitionRule {
        from: LeaseState::Quarantined,
        to: LeaseState::Closed,
        enters_when: "排障流程或关闭流程完成后强制关闭并确认",
        behaves_as: "在关闭期间仍占用预算（回池不得释放物理预算，反之关闭必释放）",
        exits_when: "无出边：终态",
    },
];

pub fn lookup_transition(from: LeaseState, to: LeaseState) -> Option<&'static TransitionRule> {
    LEASE_TRANSITIONS
        .iter()
        .find(|rule| rule.from == from && rule.to == to)
}
