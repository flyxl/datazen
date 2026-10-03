//! 等待队列（connection-management.md §9.5「调度策略」）。
//!
//! 两级调度，互不干扰：
//!
//! - **同类内按主体轮转 FIFO**：一个类里可能有多个用户在等，谁都不能被同一个用户压死，
//!   所以队列不是一条平铺的 FIFO，而是「每个主体一条 lane + lane 组成轮转环」。
//! - **共享池按 4:2:1 加权轮转**：`interactive:metadata:job = 4:2:1`（§9.5），
//!   跳过空队列，保证任何非空队列在有限轮内一定会被放行（无饿死）。
//!
//! 保留额度不参与轮转：它是按类**专属**的，谁空谁拿（§9.5 首版不可借用）。
//!
//! 队列满（每类每服务 32、每用户 32）直接 `QueueFull`，绝不静默阻塞——见
//! [`crate::budget::ledger::BudgetLedger::enqueue`]。

use std::collections::VecDeque;

use datazen_platform_api::id::{ConnectionId, OrganizationId, PrincipalId};
use datazen_platform_api::ports::budget::ResourceClass;

use crate::budget::classes::shared_members;

/// 排队者的身份。permit 带 id 一样，waiter 也带 id：取消与放行都按 id 精确命中，
/// 不存在「按位置猜是哪一个」这种会让额度泄漏的做法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WaiterId(pub u64);

/// 一个等待中的申请。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiter {
    pub id: WaiterId,
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub principal: PrincipalId,
    pub class: ResourceClass,
    pub slots: u32,
    pub enqueued_at_ms: u64,
    /// 等待期限（毫秒，单调）。到达即超时离队，不占任何额度。
    pub deadline_ms: u64,
}

impl Waiter {
    /// 是否已经过了期限。
    pub fn expired_at(&self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms
    }

    /// 在给定时刻是否还在有效期内。
    pub fn alive_at(&self, now_ms: u64) -> bool {
        now_ms < self.deadline_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Lane {
    principal: PrincipalId,
    waiters: VecDeque<Waiter>,
}

/// 单一类别的等待队列：主体轮转 FIFO。
///
/// `lanes` 本身就是轮转环——队首是本轮该服务的主体，放行后移到环尾；
/// 该主体没人了就整个摘掉。因此环内不会出现空 lane，也不会漏主体。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassQueue {
    lanes: VecDeque<Lane>,
    depth: usize,
}

impl ClassQueue {
    /// 入队。同一主体追加到自己的 lane 尾部，新主体在环尾开一条 lane。
    pub fn push(&mut self, waiter: Waiter) {
        match self
            .lanes
            .iter_mut()
            .find(|lane| lane.principal == waiter.principal)
        {
            Some(lane) => lane.waiters.push_back(waiter),
            None => {
                let mut lane = Lane {
                    principal: waiter.principal.clone(),
                    waiters: VecDeque::new(),
                };
                lane.waiters.push_back(waiter);
                self.lanes.push_back(lane);
            }
        }
        self.depth = self.depth.saturating_add(1);
    }

    /// 取出下一个主体队首，并把这主体轮转到环尾（若它还有人等）。
    ///
    /// 环上按约定不该出现空 lane；万一出现就跳过并摘掉它——绝不把「环首是空的」
    /// 变成一个 `None` 交回给 [`crate::budget::dispatch`] 的放行循环空转。
    pub fn pop(&mut self) -> Option<Waiter> {
        while let Some(lane) = self.lanes.front_mut() {
            if lane.waiters.is_empty() {
                self.lanes.pop_front();
                continue;
            }
            let waiter = lane.waiters.pop_front()?;
            self.depth = self.depth.saturating_sub(1);
            let lane = self.lanes.pop_front()?;
            if !lane.waiters.is_empty() {
                self.lanes.push_back(lane);
            }
            return Some(waiter);
        }
        None
    }

    /// 按 id 精确移除。返回被移除的等待者；`None` 表示它已经不在队里（取消是幂等的）。
    pub fn cancel(&mut self, id: WaiterId) -> Option<Waiter> {
        let index = self
            .lanes
            .iter()
            .position(|lane| lane.waiters.iter().any(|waiter| waiter.id == id))?;
        let removed = {
            let lane = self.lanes.get_mut(index)?;
            let position = lane.waiters.iter().position(|waiter| waiter.id == id)?;
            lane.waiters.remove(position)
        };
        self.depth = self.depth.saturating_sub(1);
        if self.lanes[index].waiters.is_empty() {
            self.lanes.remove(index);
        }
        removed
    }

    /// 摘掉所有已过期的等待者。过期者**不持有**任何额度，所以这里只改队列。
    ///
    /// lane 的存废以「过滤后是否还有等待者」为准：只要还剩一个就留在环里。
    /// （判据不能写成「过滤前后长度是否变化」——那样一次**什么都没摘掉**的
    /// [`crate::budget::ledger::BudgetLedger::pump`] 会把整条队列的 lane 清空，
    /// 而 `depth` 仍是原值，随后 `pop` 永远返回 `None`，等待者就此人间蒸发。）
    pub fn expire(&mut self, now_ms: u64) -> Vec<Waiter> {
        let mut expired = Vec::new();
        self.lanes.retain_mut(|lane| {
            lane.waiters.retain(|waiter| {
                if waiter.expired_at(now_ms) {
                    expired.push(waiter.clone());
                    false
                } else {
                    true
                }
            });
            !lane.waiters.is_empty()
        });
        self.depth = self.depth.saturating_sub(expired.len());
        expired
    }

    /// 队列里还剩几个。
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// 某个主体还在等几个。
    pub fn depth_for(&self, principal: &PrincipalId) -> usize {
        self.lanes
            .iter()
            .find(|lane| &lane.principal == principal)
            .map_or(0, |lane| lane.waiters.len())
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.depth == 0
    }

    /// 本轮轮到的下一个主体是谁（只看，不消费）。
    pub fn next_principal(&self) -> Option<&PrincipalId> {
        self.lanes.front().map(|lane| &lane.principal)
    }

    /// 队首的期限，用来断言「最早的等待者先超时」。
    pub fn earliest_deadline(&self) -> Option<u64> {
        self.lanes
            .iter()
            .flat_map(|lane| lane.waiters.iter())
            .map(|waiter| waiter.deadline_ms)
            .min()
    }

    /// 快照出全部等待者（观测用）。
    pub fn snapshot(&self) -> Vec<Waiter> {
        self.lanes
            .iter()
            .flat_map(|lane| lane.waiters.iter().cloned())
            .collect()
    }
}

/// 共享池的平滑加权轮转（nginx smooth WRR）。
///
/// 只在 [`shared_members`]——interactive(4) / metadata(2) / job(1)——之间转，
/// `Control` 根本不参与（§9.5：control 只用于取消与健康恢复）。
///
/// 不饿死的原因：每一轮给**非空**队列各自加上自己的权重，然后只被选中的那个减掉本轮权重和。
/// 一个非空队列每轮至少加 `w`，最多被减 `total`，因此最多 `ceil(total / w)` 轮必然被选中一次
/// （`total ≤ 7`，`w ≥ 1` ⇒ 最多 7 轮）。空队列不加分也不减分，因此「先空后满」的类别
/// 攒下的信用会在它重新有人的时候立刻兑现，不会被后来的队列永久压住。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SharedRoundRobin {
    credit: [i64; 4],
}

impl SharedRoundRobin {
    /// 构造一个信用全零的轮转器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 选出下一个应当放行的类别；全空时返回 `None`。
    pub fn pick(&mut self, queues: &[ClassQueue; 4]) -> Option<ResourceClass> {
        let mut weights = [0i64; 4];
        let mut total = 0i64;
        for class in shared_members() {
            if queues[class.index()].is_empty() {
                continue;
            }
            let weight = i64::from(class.shared_weight().unwrap_or(0));
            weights[class.index()] = weight;
            total += weight;
            self.credit[class.index()] += weight;
        }
        if total == 0 {
            return None;
        }
        let mut chosen: Option<ResourceClass> = None;
        for class in shared_members() {
            if weights[class.index()] == 0 {
                continue;
            }
            chosen = match chosen {
                None => Some(class),
                Some(current) if self.credit[class.index()] > self.credit[current.index()] => {
                    Some(class)
                }
                Some(current) => Some(current),
            };
        }
        let class = chosen?;
        self.credit[class.index()] -= total;
        Some(class)
    }

    /// 某类当前的信用（测试可观测，不参与判定）。
    pub fn credit(&self, class: ResourceClass) -> i64 {
        self.credit[class.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(name: &str) -> PrincipalId {
        PrincipalId::new(name)
    }

    fn waiter(id: u64, name: &str, deadline_ms: u64) -> Waiter {
        Waiter {
            id: WaiterId(id),
            organization_id: OrganizationId::new("org"),
            connection_id: ConnectionId::new("conn"),
            principal: principal(name),
            class: ResourceClass::Interactive,
            slots: 1,
            enqueued_at_ms: 0,
            deadline_ms,
        }
    }

    #[test]
    fn in_class_rotation_is_principal_round_robin_fifo() {
        let mut queue = ClassQueue::default();
        queue.push(waiter(1, "alice", 100));
        queue.push(waiter(2, "alice", 100));
        queue.push(waiter(3, "bob", 100));
        queue.push(waiter(4, "bob", 100));

        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(1)));
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(3)));
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(2)));
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(4)));
        assert_eq!(queue.depth(), 0);
        assert!(queue.pop().is_none());
    }

    #[test]
    fn cancelling_one_waiter_leaves_the_lane_of_the_other_intact() {
        let mut queue = ClassQueue::default();
        queue.push(waiter(1, "alice", 100));
        queue.push(waiter(2, "bob", 100));

        assert_eq!(queue.cancel(WaiterId(1)).map(|w| w.id), Some(WaiterId(1)));
        assert_eq!(queue.depth(), 1);
        assert_eq!(queue.depth_for(&principal("alice")), 0);
        assert_eq!(queue.depth_for(&principal("bob")), 1);
        assert_eq!(queue.next_principal(), Some(&principal("bob")));
        // 取消是幂等的：第二次取消同一 id 返回 None，且不改变队列。
        assert!(queue.cancel(WaiterId(1)).is_none());
        assert_eq!(queue.depth(), 1);
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(2)));
        assert_eq!(queue.depth(), 0);
    }

    #[test]
    fn expiry_removes_every_due_waiter_and_keeps_the_rest() {
        let mut queue = ClassQueue::default();
        queue.push(waiter(1, "alice", 10));
        queue.push(waiter(2, "alice", 50));
        queue.push(waiter(3, "bob", 50));

        let expired = queue.expire(10);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, WaiterId(1));
        assert_eq!(queue.depth(), 2);
        assert_eq!(queue.earliest_deadline(), Some(50));
    }

    /// 回归：一次**什么都没摘掉**的 `expire`（每次 `pump` 都会先做一遍）不得动队列。
    /// 判据若写成「过滤前后长度是否变化」，这种调用会把 lane 全清空而 `depth` 照旧，
    /// 之后 `pop` 永远给不出等待者，等待者就再也不会被放行。
    #[test]
    fn expiring_nothing_keeps_the_queue_bit_identical() {
        let mut queue = ClassQueue::default();
        queue.push(waiter(1, "alice", 100));
        queue.push(waiter(2, "alice", 100));
        queue.push(waiter(3, "bob", 100));
        let before = queue.clone();

        assert!(queue.expire(0).is_empty());
        assert_eq!(queue, before, "没有到期者时队列必须逐位不变");
        assert_eq!(queue.depth(), 3);
        assert_eq!(queue.depth_for(&principal("bob")), 1);
        // 队列还在，所以等待者仍能被逐一放行。
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(1)));
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(3)));
        assert_eq!(queue.pop().map(|w| w.id), Some(WaiterId(2)));
        assert!(queue.pop().is_none());
        assert!(queue.is_empty());
    }

    /// 回归：lane 里的人全部过期后，lane 必须整条摘掉，不能留下一条空 lane 顶着 `depth`。
    #[test]
    fn a_lane_whose_waiters_all_expire_is_removed_entirely() {
        let mut queue = ClassQueue::default();
        queue.push(waiter(1, "alice", 10));
        queue.push(waiter(2, "bob", 10));
        queue.push(waiter(3, "bob", 10));

        let expired = queue.expire(10);
        assert_eq!(expired.len(), 3);
        assert_eq!(queue.depth(), 0);
        assert!(queue.is_empty());
        assert!(queue.pop().is_none());
    }

    fn queues_with(classes: &[ResourceClass]) -> [ClassQueue; 4] {
        let mut queues: [ClassQueue; 4] = Default::default();
        for (offset, class) in classes.iter().enumerate() {
            let mut waiter = waiter(offset as u64 + 1, "alice", 1_000);
            waiter.class = *class;
            queues[class.index()].push(waiter);
        }
        queues
    }

    #[test]
    fn shared_rotation_follows_four_two_one_over_a_full_cycle() {
        let queues = queues_with(&[
            ResourceClass::Interactive,
            ResourceClass::Metadata,
            ResourceClass::Job,
        ]);
        let mut rr = SharedRoundRobin::new();
        let mut counts = [0usize; 4];
        for _ in 0..7 {
            let class = rr.pick(&queues).expect("三类都有等待者");
            counts[class.index()] += 1;
        }
        assert_eq!(counts[ResourceClass::Interactive.index()], 4);
        assert_eq!(counts[ResourceClass::Metadata.index()], 2);
        assert_eq!(counts[ResourceClass::Job.index()], 1);
    }

    #[test]
    fn empty_queues_are_skipped_without_starving_the_rest() {
        let queues = queues_with(&[ResourceClass::Interactive, ResourceClass::Job]);
        let mut rr = SharedRoundRobin::new();
        for _ in 0..5 {
            let class = rr.pick(&queues).expect("至少一类非空");
            assert!(
                class == ResourceClass::Interactive || class == ResourceClass::Job,
                "空队列不得被选中"
            );
        }
    }

    #[test]
    fn a_non_empty_queue_is_served_within_the_credit_window() {
        let queues = queues_with(&[ResourceClass::Interactive]);
        let mut rr = SharedRoundRobin::new();
        assert_eq!(rr.pick(&queues), Some(ResourceClass::Interactive));
    }
}
