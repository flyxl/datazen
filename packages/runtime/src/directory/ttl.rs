//! 会话期限（TTL）。
//!
//! 期限存的是**服务端单调时刻**，不是墙上时间。墙上时间只在被投影成 `expiresAt`
//! 时才出现一次，而且只是为了给调用方看；判定路径一次都不碰它，因此 NTP 回拨
//! 或者调用方乱传时间都不会让会话提前或延后过期。
//!
//! 一个条目同时最多挂三类期限，`expiresAt` = **最早**那个；同类期限后挂的覆盖先挂的。

use std::time::Duration;

use datazen_platform_api::id::Timestamp;

use super::{DirectoryClock, MonoInstant};

/// 期限种类。决定到期后的裁决路径，也是调用方能看到的 `expiresAt` 语义标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeadlineKind {
    /// 断连宽限：客户端掉线后还愿意等多久。
    DisconnectGrace,
    /// 事务空闲：开着事务却没人动。
    TransactionIdle,
    /// 会话空闲：完全没人动。
    SessionIdle,
}

impl DeadlineKind {
    /// 稳定的字符串标签，进日志与事件用。
    pub const fn as_str(self) -> &'static str {
        match self {
            DeadlineKind::DisconnectGrace => "disconnectGrace",
            DeadlineKind::TransactionIdle => "transactionIdle",
            DeadlineKind::SessionIdle => "sessionIdle",
        }
    }

    /// 同一刻上有多类期限到点时的裁决优先级。小的先裁决。
    const fn tie_break(self) -> u8 {
        match self {
            DeadlineKind::DisconnectGrace => 0,
            DeadlineKind::TransactionIdle => 1,
            DeadlineKind::SessionIdle => 2,
        }
    }
}

/// 一个已挂上的期限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    kind: DeadlineKind,
    at: MonoInstant,
}

impl Deadline {
    pub fn new(kind: DeadlineKind, at: MonoInstant) -> Self {
        Self { kind, at }
    }

    /// 从 `armed_at` 起 `ttl` 之后。
    pub fn after(kind: DeadlineKind, armed_at: MonoInstant, ttl: Duration) -> Self {
        Self {
            kind,
            at: armed_at + ttl,
        }
    }

    pub const fn kind(&self) -> DeadlineKind {
        self.kind
    }

    pub const fn at(&self) -> MonoInstant {
        self.at
    }

    /// 相对 `now` 还剩多久。到点或过期返回 0。
    pub fn remaining(&self, now: MonoInstant) -> Duration {
        self.at.saturating_duration_since(now)
    }

    pub fn is_due(&self, now: MonoInstant) -> bool {
        self.at <= now
    }

    /// UTC 投影。只是给人看的字符串，不参与判定。
    pub fn project(&self, clock: &dyn DirectoryClock) -> Timestamp {
        clock.project(self.at)
    }
}

/// 期限集合：每类至多一个，`earliest` 是最早的那个。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadlineSet {
    slots: [Option<Deadline>; 3],
}

impl DeadlineSet {
    pub fn new() -> Self {
        Self::default()
    }

    fn slot_index(kind: DeadlineKind) -> usize {
        usize::from(kind.tie_break())
    }

    /// 挂一个期限。同类覆盖：重新挂期就是推迟到新的时刻。
    pub fn arm(&mut self, deadline: Deadline) {
        self.slots[Self::slot_index(deadline.kind)] = Some(deadline);
    }

    /// 摘掉某类期限。返回它是否本来挂着——「本来没挂」不是错误，
    /// 因为取消一个从未设置的期限是调用方的正常动作。
    pub fn cancel(&mut self, kind: DeadlineKind) -> bool {
        self.slots[Self::slot_index(kind)].take().is_some()
    }

    /// 摘掉所有期限。
    pub fn clear(&mut self) {
        self.slots = [None; 3];
    }

    pub fn get(&self, kind: DeadlineKind) -> Option<&Deadline> {
        self.slots[Self::slot_index(kind)].as_ref()
    }

    pub fn has(&self, kind: DeadlineKind) -> bool {
        self.slots[Self::slot_index(kind)].is_some()
    }

    /// 挂着的期限数。
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// 最早的那个期限。并列时按 [`DeadlineKind::tie_break`] 定序，保证结果唯一。
    pub fn earliest(&self) -> Option<Deadline> {
        self.slots.iter().flatten().copied().min_by(|a, b| {
            a.at.cmp(&b.at)
                .then(a.kind.tie_break().cmp(&b.kind.tie_break()))
        })
    }

    /// `expiresAt` 的唯一来源：最早期限的 UTC 投影。没有期限就是 `None`
    /// （执行中且没有适用期限时正是这个情形）。
    pub fn expiration_projection(&self, clock: &dyn DirectoryClock) -> Option<Timestamp> {
        self.earliest().map(|deadline| clock.project(deadline.at))
    }

    /// 是否至少有一个期限已经到点。用**单调** `now` 判定。
    pub fn is_due(&self, now: MonoInstant) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|deadline| deadline.is_due(now))
    }

    /// 第一个该被裁决的期限种类；没有则 `None`。同刻并列时按 tie_break 定序。
    pub fn due_kind(&self, now: MonoInstant) -> Option<DeadlineKind> {
        self.slots
            .iter()
            .flatten()
            .filter(|deadline| deadline.is_due(now))
            .min_by(|a, b| {
                a.at.cmp(&b.at)
                    .then(a.kind.tie_break().cmp(&b.kind.tie_break()))
            })
            .map(|deadline| deadline.kind)
    }

    /// 挂着的期限里有没有哪一类已经到点（供状态机判定关闭路径）。
    pub fn is_due_for(&self, now: MonoInstant, kind: DeadlineKind) -> bool {
        self.get(kind).is_some_and(|deadline| deadline.is_due(now))
    }
}
